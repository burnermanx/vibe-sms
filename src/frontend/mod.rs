use gtk::{prelude::*, gdk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use crate::core::Emulator;
use gilrs::{Gilrs, Button, Event};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub fn launch_frontend(rom_path: Option<String>) {
    let app = adw::Application::builder()
        .application_id("com.github.vibe-sms")
        .build();

    app.connect_startup(|_| {
        adw::init().expect("Failed to initialize libadwaita");
    });

    let rom_path_rc = Rc::new(rom_path);

    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("vibe-sms (Libadwaita)")
            .default_width(512)
            .default_height(384)
            .build();

        // Menu bar setup
        let header_bar = adw::HeaderBar::new();
        let open_button = gtk::Button::with_label("Open ROM");
        header_bar.pack_start(&open_button);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header_bar);

        // Rendering area using a Picture widget
        let picture = gtk::Picture::new();
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        // Ensure to keep aspect ratio typically or stretch. Picture scales nicely.
        picture.set_can_shrink(true);
        content.append(&picture);

        window.set_content(Some(&content));
        
        // Setup Audio using cpal
        let audio_buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(4096)));
        let audio_buffer_clone_for_stream = audio_buffer.clone();
        
        // Sample rate shared across closures
        let audio_sample_rate_rc = Rc::new(std::cell::Cell::new(44100.0f32));
        
        let host = cpal::default_host();
        let _stream = if let Some(device) = host.default_output_device() {
            let config = device.default_output_config().unwrap();
            let device_sample_rate = config.sample_rate().0 as f32;
            audio_sample_rate_rc.set(device_sample_rate);
            
            // Start the audio stream
            let stream = device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = audio_buffer_clone_for_stream.lock().unwrap();
                    let mut src_idx = 0;
                    for out_sample in data.iter_mut() {
                        *out_sample = if src_idx < buf.len() {
                            let s = buf[src_idx];
                            src_idx += 1;
                            s
                        } else {
                            0.0
                        };
                    }
                    if src_idx < buf.len() {
                        buf.drain(0..src_idx);
                    } else {
                        buf.clear();
                    }
                },
                |err| eprintln!("Audio stream error: {}", err),
                None
            ).unwrap();
            
            stream.play().unwrap();
            Some(stream)
        } else {
            eprintln!("No audio output device found.");
            None
        };
        // The stream must be kept alive for audio to play
        let stream_keepalive = _stream;
        
        // Wrap emulator in a type that respects GTK / Rust lifetimes and mutability
        let emu: Rc<RefCell<Option<Emulator>>> = Rc::new(RefCell::new(None));
        
        let emu_clone_for_open = emu.clone();
        let sample_rate_for_open = audio_sample_rate_rc.clone();
        let window_clone = window.clone();
        open_button.connect_clicked(move |_| {
            let dialog = gtk::FileChooserNative::new(
                Some("Open System ROM"),
                Some(&window_clone),
                gtk::FileChooserAction::Open,
                Some("Open"),
                Some("Cancel"),
            );
            
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Sega 8-bit ROMs (*.sms, *.sg, *.gg)"));
            filter.add_pattern("*.sms");
            filter.add_pattern("*.sg");
            filter.add_pattern("*.gg");
            dialog.add_filter(&filter);
            
            let emu_inner_clone = emu_clone_for_open.clone();
            let sample_rate_for_dialog = sample_rate_for_open.get();
            
            dialog.connect_response(move |d, response| {
                if response == gtk::ResponseType::Accept {
                    if let Some(file) = d.file() {
                        if let Some(path) = file.path() {
                            match std::fs::read(&path) {
                                Ok(rom_data) => {
                                    let is_gg = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("gg")).unwrap_or(false);
                                    *emu_inner_clone.borrow_mut() = Some(Emulator::new(rom_data, is_gg, sample_rate_for_dialog));
                                    println!("Loaded ROM! (Game Gear mode: {})", is_gg);
                                },
                                Err(e) => eprintln!("Failed to load ROM: {}", e)
                            }
                        }
                    }
                }
                d.destroy(); // GTK4 FileChooser Native
            });
            
            dialog.show();
        });
        
        // Initialize Emulator core if ROM provided via CLI args
        if let Some(path_str) = &*rom_path_rc {
            match std::fs::read(path_str) {
                Ok(rom_data) => {
                    let path = std::path::Path::new(path_str);
                    let is_gg = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("gg")).unwrap_or(false);
                    *emu.borrow_mut() = Some(Emulator::new(rom_data, is_gg, audio_sample_rate_rc.get()));
                },
                Err(e) => eprintln!("Failed to load ROM: {}", e)
            }
        }
        
        // Setup GTK keyboard tracking
        let key_state = Rc::new(RefCell::new((false, false, false, false, false, false, false))); // Up, Down, Left, Right, B1, B2, Start
        
        let key_controller = gtk::EventControllerKey::new();
        
        let key_state_clone = key_state.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            let mut state = key_state_clone.borrow_mut();
            match keyval.name().as_deref() {
                Some("Up") => state.0 = true,
                Some("Down") => state.1 = true,
                Some("Left") => state.2 = true,
                Some("Right") => state.3 = true,
                Some("z") | Some("Z") => state.4 = true, // Button 1
                Some("x") | Some("X") => state.5 = true, // Button 2
                Some("Return") | Some("KP_Enter") | Some("Enter") => state.6 = true, // Start / Pause
                _ => {}
            }
            glib::Propagation::Proceed
        });

        let key_state_clone = key_state.clone();
        key_controller.connect_key_released(move |_, keyval, _, _| {
            let mut state = key_state_clone.borrow_mut();
            match keyval.name().as_deref() {
                Some("Up") => state.0 = false,
                Some("Down") => state.1 = false,
                Some("Left") => state.2 = false,
                Some("Right") => state.3 = false,
                Some("z") | Some("Z") => state.4 = false, // Button 1
                Some("x") | Some("X") => state.5 = false, // Button 2
                Some("Return") | Some("KP_Enter") | Some("Enter") => state.6 = false, // Start / Pause
                _ => {}
            }
        });

        window.add_controller(key_controller);

        // Setup Mouse tracking for Light Phaser
        let mouse_state = Rc::new(RefCell::new((false, 0u16, 0u16, 0u8))); // physical_active, x, y, frames_left_to_hold
        
        let motion_controller = gtk::EventControllerMotion::new();
        let mouse_state_motion = mouse_state.clone();
        motion_controller.connect_motion(move |_, x, y| {
            let mut state = mouse_state_motion.borrow_mut();
            // Scale x, y (from window dimensions to 256x192)
            // Assuming the picture widget is stretched. We'll approximate:
            // For now, we assume fixed 256x192 or use widget size
            // To do this perfectly we need widget dimensions. Since we don't have it here directly,
            // GTK x, y on the picture widget should be proportionally scaled.
            // But we can extract widget width/height.
            // For simplicity, let's treat x, y as directly hitting the 256x192 texture or scale it later.
            // Actually, motion_controller gives coordinates relative to the widget it's attached to.
            state.1 = x as u16;
            state.2 = y as u16;
        });
        picture.add_controller(motion_controller);
        
        let click_controller = gtk::GestureClick::new();
        let mouse_state_click = mouse_state.clone();
        click_controller.connect_pressed(move |_, _n_press, _, _| {
            let mut state = mouse_state_click.borrow_mut();
            state.0 = true;
            state.3 = 6; // Guarantee at least 6 frames of trigger pull
        });
        
        let mouse_state_release = mouse_state.clone();
        click_controller.connect_released(move |_, _n_press, _, _| {
            let mut state = mouse_state_release.borrow_mut();
            state.0 = false;
        });
        picture.add_controller(click_controller);

        // Setup Gilrs (Gamepad)
        let gilrs = Rc::new(RefCell::new(Gilrs::new().unwrap()));
        let gamepad_state = Rc::new(RefCell::new((false, false, false, false, false, false, false))); // U, D, L, R, B1, B2, Start

        // Initialize Emulator core if ROM provided
        if let Some(path_str) = &*rom_path_rc {
            match std::fs::read(path_str) {
                Ok(rom_data) => {
                    let path = std::path::Path::new(path_str);
                    let is_gg = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("gg")).unwrap_or(false);
                    *emu.borrow_mut() = Some(Emulator::new(rom_data, is_gg, audio_sample_rate_rc.get()));
                },
                Err(e) => eprintln!("Failed to load inicial ROM: {}" , e)
            }
        }
        
        // Setup main loop
        let picture_clone = picture.clone();
        let emu_clone = emu.clone(); // This emu_clone now refers to the outer `emu`
        let key_state_loop = key_state.clone();
        let gamepad_state_loop = gamepad_state.clone();
        let gilrs_loop = gilrs.clone();
        let audio_buffer_loop = audio_buffer.clone();
        let mouse_state_loop = mouse_state.clone();
        
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            // Keep the audio stream alive as long as the loop runs
            let _keep = &stream_keepalive;
            
            // Poll Gamepad events
            let mut gilrs_mut = gilrs_loop.borrow_mut();
            while let Some(Event { id, .. }) = gilrs_mut.next_event() {
                let gamepad = gilrs_mut.gamepad(id);
                let mut g_state = gamepad_state_loop.borrow_mut();
                g_state.0 = gamepad.is_pressed(Button::DPadUp);
                g_state.1 = gamepad.is_pressed(Button::DPadDown);
                g_state.2 = gamepad.is_pressed(Button::DPadLeft);
                g_state.3 = gamepad.is_pressed(Button::DPadRight);
                g_state.4 = gamepad.is_pressed(Button::South) || gamepad.is_pressed(Button::West); // Cross / Square
                g_state.5 = gamepad.is_pressed(Button::East) || gamepad.is_pressed(Button::North); // Circle / Triangle
                g_state.6 = gamepad.is_pressed(Button::Start);
            }
            
            if let Some(ref mut emu_mut) = *emu_clone.borrow_mut() {
                // Combine input
                let ks = key_state_loop.borrow();
                let gs = gamepad_state_loop.borrow();
                let ms = mouse_state_loop.borrow();
                let mut trigger_active = ms.0;
                if ms.3 > 0 {
                    trigger_active = true;
                    drop(ms);
                    mouse_state_loop.borrow_mut().3 -= 1;
                } else {
                    drop(ms);
                }
                
                emu_mut.set_input(
                    ks.0 || gs.0,
                    ks.1 || gs.1,
                    ks.2 || gs.2,
                    ks.3 || gs.3,
                    ks.4 || gs.4 || trigger_active, // Button 1 is also triggered by mouse click
                    ks.5 || gs.5, // Button 2
                    ks.6 || gs.6  // Start / NMI
                );
                
                // Pass lightgun coordinates
                // We need to scale from widget size to 256x192
                let widget_w = picture_clone.width() as f64;
                let widget_h = picture_clone.height() as f64;
                
                let mut scaled_x = 0;
                let mut scaled_y = 0;
                
                if widget_w > 0.0 && widget_h > 0.0 {
                    let aspect_ratio = 256.0 / 192.0;
                    let widget_aspect = widget_w / widget_h;
                    
                    let (rendered_w, rendered_h, offset_x, offset_y) = if widget_aspect > aspect_ratio {
                        // Letterboxed on left/right
                        let h = widget_h;
                        let w = h * aspect_ratio;
                        (w, h, (widget_w - w) / 2.0, 0.0)
                    } else {
                        // Letterboxed on top/bottom
                        let w = widget_w;
                        let h = w / aspect_ratio;
                        (w, h, 0.0, (widget_h - h) / 2.0)
                    };
                    
                    let mx = mouse_state_loop.borrow().1 as f64;
                    let my = mouse_state_loop.borrow().2 as f64;
                    
                    let rel_x = mx - offset_x;
                    let rel_y = my - offset_y;
                    
                    if rel_x >= 0.0 && rel_x <= rendered_w {
                        scaled_x = ((rel_x / rendered_w) * 256.0) as u16;
                    }
                    if rel_y >= 0.0 && rel_y <= rendered_h {
                        scaled_y = ((rel_y / rendered_h) * 192.0) as u16;
                    }
                }
                
                emu_mut.set_lightgun(trigger_active, scaled_x.min(255), scaled_y.min(191));
                
                // Run until a frame is ready
                let (_frame_ready, mut audio_samples) = emu_mut.step_frame(); // Expects our modified step_frame tuple (bool, Vec<f32>)
                let frame = emu_mut.get_framebuffer();
                
                // Send audio
                if let Ok(mut buf) = audio_buffer_loop.try_lock() {
                    buf.append(&mut audio_samples);
                    // Prevent unlimited growth if audio is lagging
                    // 8192 is the maximum we'll hold before forcing a drain.
                    // Instead of dropping massive chunks, we just drop the exact overflow.
                    if buf.len() > 8192 { 
                        let excess = buf.len() - 8192;
                        buf.drain(0..excess); 
                    }
                }
                
                let is_gg = emu_mut.is_gg;
                let (render_w, render_h, offset_x, offset_y) = if is_gg {
                    (160, 144, 48, 24)
                } else {
                    (256, 192, 0, 0)
                };
                
                // Convert [u32] ARGB from minifb style to RGBA for GTK/GDK
                let mut bytes: Vec<u8> = Vec::with_capacity(render_w * render_h * 4);
                for y in 0..render_h {
                    for x in 0..render_w {
                        let pixel = frame[(y + offset_y) * 256 + (x + offset_x)];
                        bytes.push(((pixel >> 16) & 0xFF) as u8); // R
                        bytes.push(((pixel >> 8) & 0xFF) as u8);  // G
                        bytes.push((pixel & 0xFF) as u8);         // B
                        bytes.push(((pixel >> 24) & 0xFF) as u8); // A
                    }
                }
                
                let bytes = glib::Bytes::from(&bytes);
                
                let texture = gdk::MemoryTexture::new(
                    render_w as i32,
                    render_h as i32,
                    gdk::MemoryFormat::R8g8b8a8,
                    &bytes,
                    render_w as usize * 4,
                );
                
                picture_clone.set_paintable(Some(&texture));
            }
            
            glib::ControlFlow::Continue
        });

        window.present();
    });

    app.run();
}
