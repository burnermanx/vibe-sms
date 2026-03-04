use gtk::{prelude::*, gdk, glib, gio};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
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
            .title("vibe-sms")
            .default_width(512)
            .default_height(384)
            .build();

        // ── Header bar with hamburger menu ──────────────────────────────────
        let header_bar = adw::HeaderBar::new();



        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("open-menu-symbolic");
        menu_button.set_tooltip_text(Some("Menu"));
        header_bar.pack_end(&menu_button);

        // GIO menu model
        let menu = gio::Menu::new();

        let emulator_section = gio::Menu::new();
        emulator_section.append(Some("Load ROM"), Some("win.load-rom"));
        emulator_section.append(Some("Reset"), Some("win.reset"));
        emulator_section.append(Some("Stop Emulation"), Some("win.stop-emulation"));
        menu.append_section(None, &emulator_section);

        let settings_section = gio::Menu::new();
        settings_section.append(Some("Configure Controls"), Some("win.configure-controls"));
        // Stateful bool action → GTK4 PopoverMenu renders it as a checkbox automatically
        settings_section.append(Some("FM Sound (Master System only)"), Some("win.toggle-fm"));
        menu.append_section(None, &settings_section);

        let view_section = gio::Menu::new();
        view_section.append(Some("Full Screen"), Some("win.fullscreen"));
        menu.append_section(None, &view_section);

        let help_section = gio::Menu::new();
        help_section.append(Some("About"), Some("win.about"));
        menu.append_section(None, &help_section);

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        menu_button.set_popover(Some(&popover));

        // ── Layout ─────────────────────────────────────────────────────────
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header_bar);

        let picture = gtk::Picture::new();
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_can_shrink(true);
        content.append(&picture);

        window.set_content(Some(&content));

        // ── Audio ──────────────────────────────────────────────────────────
        let audio_buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(4096)));
        let audio_buffer_clone_for_stream = audio_buffer.clone();

        let audio_sample_rate_rc = Rc::new(Cell::new(44100.0f32));

        let host = cpal::default_host();
        let _stream = if let Some(device) = host.default_output_device() {
            let config = device.default_output_config().unwrap();
            let device_sample_rate = config.sample_rate().0 as f32;
            audio_sample_rate_rc.set(device_sample_rate);

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
                None,
            ).unwrap();

            stream.play().unwrap();
            Some(stream)
        } else {
            eprintln!("No audio output device found.");
            None
        };
        let stream_keepalive = _stream;

        // ── Emulator state ─────────────────────────────────────────────────
        let emu: Rc<RefCell<Option<Emulator>>> = Rc::new(RefCell::new(None));

        // Currently loaded ROM name (for window title) and path (for reset)
        let rom_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let rom_path: Rc<RefCell<Option<std::path::PathBuf>>> = Rc::new(RefCell::new(None));

        // True when FM is user-disabled (synced to fm.user_disabled each tick)
        // false = FM ON (default), true = FM blocked at hardware level → game uses PSG
        let fm_user_disabled: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // Whether the current ROM is Game Gear (FM is SMS-only)
        let is_gg_loaded: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // ── GIO Actions ────────────────────────────────────────────────────

        // --- load-rom ---
        let load_action = gio::SimpleAction::new("load-rom", None);
        {
            let emu_for_load = emu.clone();
            let rom_name_for_load = rom_name.clone();
            let rom_path_for_load = rom_path.clone();
            let is_gg_loaded_for_load = is_gg_loaded.clone();
            let sample_rate_for_load = audio_sample_rate_rc.clone();
            let fm_user_disabled_for_load = fm_user_disabled.clone();
            let window_for_load = window.clone();
            load_action.connect_activate(move |_action, _| {
                let dialog = gtk::FileChooserNative::new(
                    Some("Open System ROM"),
                    Some(&window_for_load),
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

                let emu_inner = emu_for_load.clone();
                let rom_name_inner = rom_name_for_load.clone();
                let rom_path_inner = rom_path_for_load.clone();
                let is_gg_loaded_inner = is_gg_loaded_for_load.clone();
                let fm_user_disabled_inner = fm_user_disabled_for_load.clone();
                let sample_rate = sample_rate_for_load.get();
                let window_inner = window_for_load.clone();

                dialog.connect_response(move |d, response| {
                    if response == gtk::ResponseType::Accept {
                        if let Some(file) = d.file() {
                            if let Some(path) = file.path() {
                                match std::fs::read(&path) {
                                    Ok(rom_data) => {
                                        let is_gg = path.extension()
                                            .and_then(|e| e.to_str())
                                            .map(|e| e.eq_ignore_ascii_case("gg"))
                                            .unwrap_or(false);
                                        let new_emu = Emulator::new(rom_data, is_gg, sample_rate);
                                        // GG has no FM chip; only apply user-disable for SMS
                                        let apply = !is_gg && fm_user_disabled_inner.get();
                                        new_emu.cpu.io.bus.borrow_mut().mixer.fm.user_disabled = apply;
                                        *emu_inner.borrow_mut() = Some(new_emu);
                                        is_gg_loaded_inner.set(is_gg);
                                        // Update window title with ROM name (no extension)
                                        let name = path.file_stem()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("ROM")
                                            .to_string();
                                        *rom_name_inner.borrow_mut() = Some(name.clone());
                                        *rom_path_inner.borrow_mut() = Some(path.clone());
                                        window_inner.set_title(Some(&name));
                                        println!("Loaded ROM: {} (Game Gear: {})", name, is_gg);
                                    }
                                    Err(e) => eprintln!("Failed to load ROM: {}", e),
                                }
                            }
                        }
                    }
                    d.destroy();
                });

                dialog.show();
            });
        }
        window.add_action(&load_action);

        // --- stop-emulation (initially disabled) ---
        let stop_action = gio::SimpleAction::new("stop-emulation", None);
        stop_action.set_enabled(false);
        {
            let emu_for_stop = emu.clone();
            let rom_name_for_stop = rom_name.clone();
            let is_gg_loaded_for_stop = is_gg_loaded.clone();
            let stop_action_clone = stop_action.clone();
            let window_for_stop = window.clone();
            stop_action.connect_activate(move |_, _| {
                *emu_for_stop.borrow_mut() = None;
                *rom_name_for_stop.borrow_mut() = None;
                is_gg_loaded_for_stop.set(false);
                window_for_stop.set_title(Some("vibe-sms"));
                stop_action_clone.set_enabled(false);
            });
        }
        window.add_action(&stop_action);

        // Wire stop_action enable into the load callback via a tick-check approach.
        // We enable it every tick when emu is Some — lightweight and correct.
        let stop_action_for_tick = stop_action.clone();

        // --- reset ---
        let reset_action = gio::SimpleAction::new("reset", None);
        reset_action.set_enabled(false);
        {
            let emu_for_reset = emu.clone();
            let rom_path_for_reset = rom_path.clone();
            let fm_user_disabled_for_reset = fm_user_disabled.clone();
            let sample_rate_for_reset = audio_sample_rate_rc.clone();
            reset_action.connect_activate(move |_, _| {
                if let Some(ref path) = *rom_path_for_reset.borrow() {
                    match std::fs::read(path) {
                        Ok(rom_data) => {
                            let is_gg = path.extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.eq_ignore_ascii_case("gg"))
                                .unwrap_or(false);
                            let new_emu = Emulator::new(rom_data, is_gg, sample_rate_for_reset.get());
                            let apply = !is_gg && fm_user_disabled_for_reset.get();
                            new_emu.cpu.io.bus.borrow_mut().mixer.fm.user_disabled = apply;
                            *emu_for_reset.borrow_mut() = Some(new_emu);
                            println!("Reset!");
                        }
                        Err(e) => eprintln!("Reset failed: {}", e),
                    }
                }
            });
        }
        window.add_action(&reset_action);
        let reset_action_for_tick = reset_action.clone();

        // --- configure-controls ---
        let controls_action = gio::SimpleAction::new("configure-controls", None);
        {
            let window_for_controls = window.clone();
            controls_action.connect_activate(move |_, _| {
                show_controls_dialog(&window_for_controls);
            });
        }
        window.add_action(&controls_action);

        // --- toggle-fm  (stateful bool → renders as checkbox in PopoverMenu) ---
        // State: true = FM ON, false = FM OFF (PSG only)
        let fm_action = gio::SimpleAction::new_stateful(
            "toggle-fm",
            None,
            &true.to_variant(), // starts FM ON
        );
        // Initially disabled until an SMS ROM is loaded
        fm_action.set_enabled(false);
        {
            let fm_user_disabled_for_action = fm_user_disabled.clone();
            let emu_for_fm = emu.clone();
            let rom_path_for_fm = rom_path.clone();
            let sample_rate_for_fm = audio_sample_rate_rc.clone();
            fm_action.connect_activate(move |action, _| {
                let current: bool = action.state().and_then(|v| v.get()).unwrap_or(true);
                let fm_on = !current;
                action.set_state(&fm_on.to_variant());
                let disabled = !fm_on;
                fm_user_disabled_for_action.set(disabled);

                // The game detects FM at boot — reset so it re-detects correctly.
                // Without a reset the PSG stays silent (game already stopped writing to it).
                if let Some(ref path) = *rom_path_for_fm.borrow() {
                    if let Ok(rom_data) = std::fs::read(path) {
                        let is_gg = path.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case("gg"))
                            .unwrap_or(false);
                        let new_emu = Emulator::new(rom_data, is_gg, sample_rate_for_fm.get());
                        new_emu.cpu.io.bus.borrow_mut().mixer.fm.user_disabled = disabled;
                        *emu_for_fm.borrow_mut() = Some(new_emu);
                    }
                }
            });
        }
        window.add_action(&fm_action);
        let fm_action_for_tick = fm_action.clone();

        // --- fullscreen ---
        let fullscreen_action = gio::SimpleAction::new("fullscreen", None);
        {
            let window_for_fs = window.clone();
            fullscreen_action.connect_activate(move |_, _| {
                window_for_fs.fullscreen();
            });
        }
        window.add_action(&fullscreen_action);

        // --- about ---
        let about_action = gio::SimpleAction::new("about", None);
        {
            let window_for_about = window.clone();
            about_action.connect_activate(move |_, _| {
                show_about_dialog(&window_for_about);
            });
        }
        window.add_action(&about_action);

        // ── Initialize from CLI ROM arg ────────────────────────────────────
        if let Some(path_str) = &*rom_path_rc {
            match std::fs::read(path_str) {
                Ok(rom_data) => {
                    let path = std::path::Path::new(path_str);
                    let is_gg = path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("gg"))
                        .unwrap_or(false);
                    *emu.borrow_mut() =
                        Some(Emulator::new(rom_data, is_gg, audio_sample_rate_rc.get()));
                    is_gg_loaded.set(is_gg);
                    let name = path.file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or("ROM")
                        .to_string();
                    *rom_name.borrow_mut() = Some(name.clone());
                    *rom_path.borrow_mut() = Some(path.to_path_buf());
                    window.set_title(Some(&name));
                }
                Err(e) => eprintln!("Failed to load ROM: {}", e),
            }
        }

        // ── Keyboard ───────────────────────────────────────────────────────
        // (Up, Down, Left, Right, B1, B2, Start)
        let key_state = Rc::new(RefCell::new((false, false, false, false, false, false, false)));

        let key_controller = gtk::EventControllerKey::new();

        let key_state_press = key_state.clone();
        let window_for_esc = window.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            let mut state = key_state_press.borrow_mut();
            match keyval.name().as_deref() {
                Some("Up") => state.0 = true,
                Some("Down") => state.1 = true,
                Some("Left") => state.2 = true,
                Some("Right") => state.3 = true,
                Some("z") | Some("Z") => state.4 = true,
                Some("x") | Some("X") => state.5 = true,
                Some("Return") | Some("KP_Enter") | Some("Enter") => state.6 = true,
                Some("Escape") => {
                    drop(state);
                    window_for_esc.unfullscreen();
                }
                _ => {}
            }
            glib::Propagation::Proceed
        });

        let key_state_release = key_state.clone();
        key_controller.connect_key_released(move |_, keyval, _, _| {
            let mut state = key_state_release.borrow_mut();
            match keyval.name().as_deref() {
                Some("Up") => state.0 = false,
                Some("Down") => state.1 = false,
                Some("Left") => state.2 = false,
                Some("Right") => state.3 = false,
                Some("z") | Some("Z") => state.4 = false,
                Some("x") | Some("X") => state.5 = false,
                Some("Return") | Some("KP_Enter") | Some("Enter") => state.6 = false,
                _ => {}
            }
        });

        window.add_controller(key_controller);

        // ── Mouse (Light Phaser) ───────────────────────────────────────────
        let mouse_state = Rc::new(RefCell::new((false, 0u16, 0u16, 0u8)));

        let motion_controller = gtk::EventControllerMotion::new();
        let mouse_state_motion = mouse_state.clone();
        motion_controller.connect_motion(move |_, x, y| {
            let mut state = mouse_state_motion.borrow_mut();
            state.1 = x as u16;
            state.2 = y as u16;
        });
        picture.add_controller(motion_controller);

        let click_controller = gtk::GestureClick::new();
        let mouse_state_click = mouse_state.clone();
        click_controller.connect_pressed(move |_, _n, _, _| {
            let mut state = mouse_state_click.borrow_mut();
            state.0 = true;
            state.3 = 6;
        });
        let mouse_state_release = mouse_state.clone();
        click_controller.connect_released(move |_, _n, _, _| {
            mouse_state_release.borrow_mut().0 = false;
        });
        picture.add_controller(click_controller);

        // ── Gamepad ────────────────────────────────────────────────────────
        let gilrs = Rc::new(RefCell::new(Gilrs::new().unwrap()));
        let gamepad_state = Rc::new(RefCell::new((false, false, false, false, false, false, false)));

        // ── FM + GG state trackers (for tick callback) ────────────────────
        let fm_user_disabled_for_tick = fm_user_disabled.clone();
        let is_gg_loaded_for_tick = is_gg_loaded.clone();

        // ── Main tick callback ─────────────────────────────────────────────
        let picture_clone = picture.clone();
        let emu_clone = emu.clone();
        let key_state_loop = key_state.clone();
        let gamepad_state_loop = gamepad_state.clone();
        let gilrs_loop = gilrs.clone();
        let audio_buffer_loop = audio_buffer.clone();
        let mouse_state_loop = mouse_state.clone();
        let last_clock_us: Rc<Cell<i64>> = Rc::new(Cell::new(0));
        let time_debt_us: Rc<Cell<i64>> = Rc::new(Cell::new(0));
        let last_clock_cb = last_clock_us.clone();
        let time_debt_cb = time_debt_us.clone();
        let last_texture: Rc<RefCell<Option<gdk::MemoryTexture>>> = Rc::new(RefCell::new(None));
        let last_texture_cb = last_texture.clone();

        picture_clone.add_tick_callback(move |widget, frame_clock| {
            let _keep = &stream_keepalive;

            // Update Stop/Reset action enabled state
            let has_rom = emu_clone.borrow().is_some();
            if stop_action_for_tick.is_enabled() != has_rom {
                stop_action_for_tick.set_enabled(has_rom);
            }
            if reset_action_for_tick.is_enabled() != has_rom {
                reset_action_for_tick.set_enabled(has_rom);
            }
            // FM only available for SMS ROMs
            let fm_should_enable = has_rom && !is_gg_loaded_for_tick.get();
            if fm_action_for_tick.is_enabled() != fm_should_enable {
                fm_action_for_tick.set_enabled(fm_should_enable);
            }

            // Sync fm user_disabled to the hardware layer every tick
            let fm_disabled_now = fm_user_disabled_for_tick.get();
            if let Some(ref mut e) = *emu_clone.borrow_mut() {
                e.cpu.io.bus.borrow_mut().mixer.fm.user_disabled = fm_disabled_now;
            }


            const SMS_FRAME_US: i64 = 16683;
            let now = frame_clock.frame_time();
            let prev = last_clock_cb.get();
            let elapsed = if prev == 0 { SMS_FRAME_US } else { (now - prev).min(50_000) };
            last_clock_cb.set(now);
            let debt = time_debt_cb.get() + elapsed;
            time_debt_cb.set(debt);
            let should_emulate = debt >= SMS_FRAME_US;

            if should_emulate {
                time_debt_cb.set((debt - SMS_FRAME_US).min(SMS_FRAME_US));

                // Poll gamepad
                let mut gilrs_mut = gilrs_loop.borrow_mut();
                while let Some(Event { id, .. }) = gilrs_mut.next_event() {
                    let gamepad = gilrs_mut.gamepad(id);
                    let mut g = gamepad_state_loop.borrow_mut();
                    g.0 = gamepad.is_pressed(Button::DPadUp);
                    g.1 = gamepad.is_pressed(Button::DPadDown);
                    g.2 = gamepad.is_pressed(Button::DPadLeft);
                    g.3 = gamepad.is_pressed(Button::DPadRight);
                    g.4 = gamepad.is_pressed(Button::South) || gamepad.is_pressed(Button::West);
                    g.5 = gamepad.is_pressed(Button::East) || gamepad.is_pressed(Button::North);
                    g.6 = gamepad.is_pressed(Button::Start);
                }
                drop(gilrs_mut);

                if let Some(ref mut emu_mut) = *emu_clone.borrow_mut() {
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
                        ks.4 || gs.4 || trigger_active,
                        ks.5 || gs.5,
                        ks.6 || gs.6,
                    );

                    // Scale mouse for light phaser
                    let widget_w = widget.width() as f64;
                    let widget_h = widget.height() as f64;
                    let mut scaled_x = 0u16;
                    let mut scaled_y = 0u16;
                    if widget_w > 0.0 && widget_h > 0.0 {
                        let aspect_ratio = 256.0 / 192.0;
                        let widget_aspect = widget_w / widget_h;
                        let (rendered_w, rendered_h, ox, oy) = if widget_aspect > aspect_ratio {
                            let h = widget_h; let w = h * aspect_ratio;
                            (w, h, (widget_w - w) / 2.0, 0.0)
                        } else {
                            let w = widget_w; let h = w / aspect_ratio;
                            (w, h, 0.0, (widget_h - h) / 2.0)
                        };
                        let mx = mouse_state_loop.borrow().1 as f64;
                        let my = mouse_state_loop.borrow().2 as f64;
                        let rel_x = mx - ox;
                        let rel_y = my - oy;
                        if rel_x >= 0.0 && rel_x <= rendered_w {
                            scaled_x = ((rel_x / rendered_w) * 256.0) as u16;
                        }
                        if rel_y >= 0.0 && rel_y <= rendered_h {
                            scaled_y = ((rel_y / rendered_h) * 192.0) as u16;
                        }
                    }
                    emu_mut.set_lightgun(trigger_active, scaled_x.min(255), scaled_y.min(191));

                    let (_frame_ready, mut audio_samples) = emu_mut.step_frame();
                    let frame = emu_mut.get_framebuffer();

                    if let Ok(mut buf) = audio_buffer_loop.try_lock() {
                        buf.append(&mut audio_samples);
                        if buf.len() > 8192 {
                            let excess = buf.len() - 8192;
                            buf.drain(0..excess);
                        }
                    }

                    // Build RGBA texture
                    let is_gg = emu_mut.is_gg;
                    let (render_w, render_h, x_off, y_off): (usize, usize, usize, usize) =
                        if is_gg { (160, 144, 48, 24) } else { (256, 192, 0, 0) };
                    let mut bytes: Vec<u8> = Vec::with_capacity(render_w * render_h * 4);
                    for y in 0..render_h {
                        for x in 0..render_w {
                            let pixel = frame[(y + y_off) * 256 + (x + x_off)];
                            bytes.push(((pixel >> 16) & 0xFF) as u8);
                            bytes.push(((pixel >> 8) & 0xFF) as u8);
                            bytes.push((pixel & 0xFF) as u8);
                            bytes.push(((pixel >> 24) & 0xFF) as u8);
                        }
                    }
                    let bytes = glib::Bytes::from(&bytes);
                    let texture = gdk::MemoryTexture::new(
                        render_w as i32, render_h as i32,
                        gdk::MemoryFormat::R8g8b8a8,
                        &bytes, render_w * 4,
                    );
                    *last_texture_cb.borrow_mut() = Some(texture);
                }
            }

            if has_rom {
                if let Some(ref tex) = *last_texture_cb.borrow() {
                    widget.set_paintable(Some(tex));
                }
            } else if last_texture_cb.borrow().is_some() {
                // Emulation stopped — clear cached frame and go black
                *last_texture_cb.borrow_mut() = None;
                widget.set_paintable(None::<&gdk::MemoryTexture>);
            }

            glib::ControlFlow::Continue
        });

        window.present();
    });

    app.run();
}

// ── Configure Controls Dialog ──────────────────────────────────────────────

fn show_controls_dialog(parent: &adw::ApplicationWindow) {
    let dialog = gtk::Window::builder()
        .title("Configure Controls")
        .transient_for(parent)
        .modal(true)
        .default_width(480)
        .default_height(340)
        .deletable(true)
        .build();

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 16);
    vbox.set_margin_top(20);
    vbox.set_margin_bottom(20);
    vbox.set_margin_start(24);
    vbox.set_margin_end(24);

    // Title label
    let title = gtk::Label::new(Some("Control Mappings"));
    title.add_css_class("title-2");
    title.set_halign(gtk::Align::Start);
    vbox.append(&title);

    // Grid of mappings
    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(24);

    // Header row
    let headers = ["Action", "Keyboard", "Gamepad / Joystick"];
    for (col, text) in headers.iter().enumerate() {
        let lbl = gtk::Label::new(Some(text));
        lbl.add_css_class("heading");
        lbl.set_halign(gtk::Align::Start);
        grid.attach(&lbl, col as i32, 0, 1, 1);
    }

    // Separator
    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    grid.attach(&sep, 0, 1, 3, 1);

    let mappings: &[(&str, &str, &str)] = &[
        ("D-Pad Up",        "↑  Arrow Up",      "D-Pad Up"),
        ("D-Pad Down",      "↓  Arrow Down",     "D-Pad Down"),
        ("D-Pad Left",      "←  Arrow Left",     "D-Pad Left"),
        ("D-Pad Right",     "→  Arrow Right",    "D-Pad Right"),
        ("Button 1",        "Z",                 "South / West"),
        ("Button 2",        "X",                 "East / North"),
        ("Start / Pause",   "Enter / Return",    "Start"),
    ];

    for (row, (action, keyboard, gamepad)) in mappings.iter().enumerate() {
        let row_idx = (row + 2) as i32;

        let lbl_action = gtk::Label::new(Some(action));
        lbl_action.set_halign(gtk::Align::Start);

        let lbl_key = gtk::Label::new(Some(keyboard));
        lbl_key.set_halign(gtk::Align::Start);
        lbl_key.add_css_class("monospace");

        let lbl_gp = gtk::Label::new(Some(gamepad));
        lbl_gp.set_halign(gtk::Align::Start);
        lbl_gp.add_css_class("monospace");

        grid.attach(&lbl_action, 0, row_idx, 1, 1);
        grid.attach(&lbl_key,    1, row_idx, 1, 1);
        grid.attach(&lbl_gp,     2, row_idx, 1, 1);
    }

    vbox.append(&grid);

    // Close button
    let close_btn = gtk::Button::with_label("Close");
    close_btn.set_halign(gtk::Align::End);
    close_btn.add_css_class("suggested-action");
    let dialog_clone = dialog.clone();
    close_btn.connect_clicked(move |_| dialog_clone.close());
    vbox.append(&close_btn);

    dialog.set_child(Some(&vbox));
    dialog.present();
}

// ── About Dialog ─────────────────────────────────────────────────────────

fn show_about_dialog(parent: &adw::ApplicationWindow) {
    let about = gtk::AboutDialog::builder()
        .transient_for(parent)
        .modal(true)
        .program_name("vibe-sms")
        .comments("A vibe-coded Master System/Game Gear emulator in Rust by BurnermanX")
        .website("https://github.com/burnermanx/vibe-sms")
        .website_label("GitHub")
        .version(env!("CARGO_PKG_VERSION"))
        .build();
    about.present();
}
