use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::path::PathBuf;
use minifb::{Key, KeyRepeat, Window, WindowOptions, Scale, MouseButton, MouseMode};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gilrs::{Gilrs, Button, Event};
use crate::core::Emulator;

// SMS/GG emulator screen dimensions
const SMS_W: usize = 256;
const SMS_H: usize = 192;
const GG_W: usize = 160;
const GG_H: usize = 144;

// ── Audio helper ─────────────────────────────────────────────────────────────

fn build_audio_stream() -> (Arc<Mutex<Vec<f32>>>, f32, Option<impl StreamTrait>) {
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(4096)));
    let buf2 = buffer.clone();
    let mut sample_rate = 44100.0f32;

    let stream = cpal::default_host()
        .default_output_device()
        .and_then(|dev| {
            let config = dev.default_output_config().ok()?;
            sample_rate = config.sample_rate().0 as f32;
            let stream = dev.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    let mut buf = buf2.lock().unwrap();
                    let mut idx = 0;
                    for s in data.iter_mut() {
                        *s = if idx < buf.len() { let v = buf[idx]; idx += 1; v } else { 0.0 };
                    }
                    if idx < buf.len() { buf.drain(0..idx); } else { buf.clear(); }
                },
                |e| eprintln!("Audio error: {e}"),
                None,
            ).ok()?;
            let _ = stream.play();
            Some(stream)
        });

    (buffer, sample_rate, stream)
}

// ── ROM loader helper ─────────────────────────────────────────────────────────

fn load_rom(path: &PathBuf, sample_rate: f32, fm_disabled: bool) -> Option<Emulator> {
    match std::fs::read(path) {
        Ok(data) => {
            let is_gg = path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("gg"))
                .unwrap_or(false);
            let emu = Emulator::new(data, is_gg, sample_rate);
            emu.cpu.io.bus.borrow_mut().mixer.fm.user_disabled = !is_gg && fm_disabled;
            let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("ROM");
            println!("Loaded ROM: {} (Game Gear: {})", name, is_gg);
            Some(emu)
        }
        Err(e) => { eprintln!("Failed to load ROM: {e}"); None }
    }
}

// ── Main entry point ──────────────────────────────────────────────────────────

pub fn launch_frontend(initial_rom: Option<String>) {
    let (audio_buf, sample_rate, _stream) = build_audio_stream();
    let mut gilrs = Gilrs::new().expect("Failed to init gilrs");

    // --- State ---
    let mut emu: Option<Emulator> = None;
    let mut rom_path: Option<PathBuf> = None;
    let mut fm_disabled = false;

    // Load from CLI if given
    if let Some(ref path_str) = initial_rom {
        let p = PathBuf::from(path_str);
        if let Some(e) = load_rom(&p, sample_rate, fm_disabled) {
            rom_path = Some(p);
            emu = Some(e);
        }
    }

    // --- Window ---
    let init_w = SMS_W * 2;
    let init_h = SMS_H * 2 + 20; // extra for status bar

    let mut window = Window::new(
        "vibe-sms",
        init_w,
        init_h,
        WindowOptions {
            resize: true,
            scale: Scale::X1,
            ..Default::default()
        },
    ).expect("Failed to create window");

    // Target ~60 fps
    window.set_target_fps(60);

    // Pixel buffer for the framebuffer (SMS resolution, ARGB XRGB)
    let mut fb: Vec<u32> = vec![0; SMS_W * SMS_H];

    // --- Input state ---
    let (mut pu, mut pd, mut pl, mut pr, mut pb1, mut pb2, mut pstart) =
        (false, false, false, false, false, false, false);
    let (mut mx, mut my) = (0u16, 0u16);
    let mut mouse_pressed;
    let mut trigger_frames: u8 = 0;

    // --- Timing ---
    let mut last_frame = Instant::now();
    let mut time_debt_us: i64 = 0;
    const SMS_FRAME_US: i64 = 16_683;

    println!("vibe-sms — Cross-platform emulator started");
    println!("Controls: O=Open ROM, R=Reset, S=Stop, F=Fullscreen, M=FM toggle, Esc=Exit fullscreen");
    println!("Keyboard: Arrow keys=D-Pad, Z=Button1, X=Button2, Enter=Start/Pause");

    while window.is_open() && !window.is_key_down(Key::Q) {
        let now = Instant::now();
        let elapsed_us = now.duration_since(last_frame).as_micros().min(50_000) as i64;
        last_frame = now;
        time_debt_us = (time_debt_us + elapsed_us).min(SMS_FRAME_US * 2);

        // --- Gamepad polling ---
        while let Some(Event { id, .. }) = gilrs.next_event() {
            let gp = gilrs.gamepad(id);
            pu    = gp.is_pressed(Button::DPadUp);
            pd    = gp.is_pressed(Button::DPadDown);
            pl    = gp.is_pressed(Button::DPadLeft);
            pr    = gp.is_pressed(Button::DPadRight);
            pb1   = gp.is_pressed(Button::South) || gp.is_pressed(Button::West);
            pb2   = gp.is_pressed(Button::East)  || gp.is_pressed(Button::North);
            pstart = gp.is_pressed(Button::Start);
        }

        // --- Keyboard state (read fresh each frame) ---
        let ku     = window.is_key_down(Key::Up);
        let kd     = window.is_key_down(Key::Down);
        let kl     = window.is_key_down(Key::Left);
        let kr     = window.is_key_down(Key::Right);
        let kb1    = window.is_key_down(Key::Z);
        let kb2    = window.is_key_down(Key::X);
        let kstart = window.is_key_down(Key::Enter);

        // --- Menu hotkeys (one-shot on key-press, no repeat) ---

        // O → Open ROM via native dialog
        if window.is_key_pressed(Key::O, KeyRepeat::No) {
            let path = rfd::FileDialog::new()
                .add_filter("Sega 8-bit ROMs", &["sms", "sg", "gg", "SMS", "SG", "GG"])
                .pick_file();
            if let Some(p) = path {
                if let Some(e) = load_rom(&p, sample_rate, fm_disabled) {
                    rom_path = Some(p);
                    emu = Some(e);
                }
            }
        }

        // R → Reset
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            if let Some(ref p) = rom_path.clone() {
                if let Some(e) = load_rom(p, sample_rate, fm_disabled) {
                    emu = Some(e);
                    println!("Reset!");
                }
            }
        }

        // S → Stop
        if window.is_key_pressed(Key::S, KeyRepeat::No) {
            emu = None;
            rom_path = None;
            fb.iter_mut().for_each(|p| *p = 0);
        }

        // F → Fullscreen toggle
        if window.is_key_pressed(Key::F, KeyRepeat::No) {
            // minifb doesn't have a direct fullscreen toggle, but we can resize to display size
            // This is a limitation of minifb; a workaround in the future would use a different backend
        }

        // M → FM toggle (SMS only)
        if window.is_key_pressed(Key::M, KeyRepeat::No) {
            let is_gg = emu.as_ref().map(|e| e.is_gg).unwrap_or(false);
            if !is_gg {
                fm_disabled = !fm_disabled;
                println!("FM Sound: {}", if fm_disabled { "OFF (PSG only)" } else { "ON" });
                // Reset so game re-detects FM hardware
                if let Some(ref p) = rom_path.clone() {
                    if let Some(e) = load_rom(p, sample_rate, fm_disabled) {
                        emu = Some(e);
                    }
                }
            }
        }

        // Escape → exit (window.is_open() + Q handles exit)
        if window.is_key_pressed(Key::Escape, KeyRepeat::No) {
            // On fullscreen, exit fullscreen — minifb limitation: just clear state
        }

        // --- Mouse (Light Phaser) ---
        if let Some((win_mx, win_my)) = window.get_mouse_pos(MouseMode::Clamp) {
            let win_w = window.get_size().0 as f32;
            let win_h = window.get_size().1 as f32;
            // Map window mouse pos to emulator screen coords (preserving aspect ratio)
            let (emu_w, emu_h) = (SMS_W as f32, SMS_H as f32);
            let aspect = emu_w / emu_h;
            let win_aspect = win_w / win_h;
            let (render_w, render_h, off_x, off_y) = if win_aspect > aspect {
                let h = win_h; let w = h * aspect;
                (w, h, (win_w - w) / 2.0, 0.0)
            } else {
                let w = win_w; let h = w / aspect;
                (w, h, 0.0, (win_h - h) / 2.0)
            };
            let rel_x = win_mx - off_x;
            let rel_y = win_my - off_y;
            if rel_x >= 0.0 && rel_x <= render_w { mx = ((rel_x / render_w) * emu_w) as u16; }
            if rel_y >= 0.0 && rel_y <= render_h { my = ((rel_y / render_h) * emu_h) as u16; }
        }
        mouse_pressed = window.get_mouse_down(MouseButton::Left);
        if mouse_pressed && trigger_frames == 0 { trigger_frames = 6; }

        // --- Emulation step ---
        if time_debt_us >= SMS_FRAME_US {
            time_debt_us -= SMS_FRAME_US;

            let trigger_active = mouse_pressed || trigger_frames > 0;
            if trigger_frames > 0 { trigger_frames -= 1; }

            if let Some(ref mut e) = emu {
                // Sync FM state
                e.cpu.io.bus.borrow_mut().mixer.fm.user_disabled = fm_disabled;

                // Send input
                e.set_input(
                    ku || pu, kd || pd, kl || pl, kr || pr,
                    kb1 || pb1 || trigger_active, kb2 || pb2,
                    kstart || pstart,
                );

                // Light phaser
                e.set_lightgun(trigger_active, mx.min(255), my.min(191));

                let (_ready, mut samples) = e.step_frame();

                // Push audio
                if let Ok(mut buf) = audio_buf.try_lock() {
                    buf.append(&mut samples);
                    if buf.len() > 8192 {
                        let excess = buf.len() - 8192;
                        buf.drain(0..excess);
                    }
                }

                // Render to framebuffer
                let frame = e.get_framebuffer();
                let is_gg = e.is_gg;
                let (render_w, render_h, x_off, y_off) =
                    if is_gg { (GG_W, GG_H, 48, 24) } else { (SMS_W, SMS_H, 0, 0) };

                // Create a render_w x render_h buffer then scale-blit into fb (SMS_W x SMS_H)
                // For GG: clear fb to black then blit centered GG output
                fb.iter_mut().for_each(|p| *p = 0);
                let blit_x = if is_gg { (SMS_W - GG_W) / 2 } else { 0 };
                let blit_y = if is_gg { (SMS_H - GG_H) / 2 } else { 0 };

                for y in 0..render_h {
                    for x in 0..render_w {
                        let px = frame[(y + y_off) * SMS_W + (x + x_off)];
                        let r = (px >> 16) & 0xFF;
                        let g = (px >> 8) & 0xFF;
                        let b = px & 0xFF;
                        // minifb uses 0x00RRGGBB
                        fb[(blit_y + y) * SMS_W + (blit_x + x)] = (r << 16) | (g << 8) | b;
                    }
                }
            } else {
                // No ROM: show black
                fb.iter_mut().for_each(|p| *p = 0);
            }
        }

        // --- Update window title ---
        let title = if let Some(ref e) = emu {
            let is_gg = e.is_gg;
            let fm_str = if !is_gg && fm_disabled { " [FM OFF]" } else { "" };
            let rom_str = rom_path.as_ref()
                .and_then(|p| p.file_stem())
                .and_then(|n| n.to_str())
                .unwrap_or("ROM");
            format!("vibe-sms — {rom_str}{fm_str}  [O]Open [R]Reset [S]Stop [M]FM toggle")
        } else {
            "vibe-sms  [O] Open ROM".to_string()
        };
        window.set_title(&title);

        // --- Scale fb to window size and display ---
        let win_w = window.get_size().0;
        let win_h = window.get_size().1;

        // Scale fb to fit window preserving aspect ratio
        let scaled = scale_buffer(&fb, SMS_W, SMS_H, win_w, win_h);
        window.update_with_buffer(&scaled, win_w, win_h).unwrap_or_else(|e| eprintln!("Display error: {e}"));
    }
}

// ── Scale a ARGB buffer to dst_w×dst_h with nearest-neighbor, letterboxed ───

fn scale_buffer(src: &[u32], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Vec<u32> {
    let mut out = vec![0u32; dst_w * dst_h];
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 { return out; }

    let aspect_src = src_w as f64 / src_h as f64;
    let aspect_dst = dst_w as f64 / dst_h as f64;

    let (render_w, render_h, x_off, y_off) = if aspect_dst > aspect_src {
        let h = dst_h; let w = (h as f64 * aspect_src) as usize;
        (w, h, (dst_w - w) / 2, 0)
    } else {
        let w = dst_w; let h = (w as f64 / aspect_src) as usize;
        (w, h, 0, (dst_h - h) / 2)
    };

    for y in 0..render_h {
        let src_y = (y * src_h) / render_h;
        for x in 0..render_w {
            let src_x = (x * src_w) / render_w;
            let pixel = src[src_y * src_w + src_x];
            out[(y + y_off) * dst_w + (x + x_off)] = pixel;
        }
    }
    out
}
