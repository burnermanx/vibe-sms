use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use eframe::egui::{self, Key, ColorImage, TextureHandle, TextureOptions};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use gilrs::{Gilrs, Button, Event as GilrsEvent};
use crate::core::Emulator;

const SMS_W: usize = 256;
const SMS_H: usize = 192;
const GG_W:  usize = 160;
const GG_H:  usize = 144;
const SMS_FRAME_US: i64 = 16_683;

// ── Audio ──────────────────────────────────────────────────────────────────────

fn build_audio_stream() -> (Arc<Mutex<Vec<f32>>>, f32, Option<Stream>) {
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

// ── SRAM helpers ───────────────────────────────────────────────────────────────

fn sram_path(rom_path: &PathBuf) -> PathBuf {
    rom_path.with_extension("sav")
}

fn save_sram(emu: &Emulator, rom_path: &PathBuf) {
    let data = emu.get_cart_ram();
    let path = sram_path(rom_path);
    match std::fs::write(&path, &data) {
        Ok(_) => {
            emu.clear_sram_dirty();
            println!("SRAM saved to {}", path.display());
        }
        Err(e) => eprintln!("Failed to save SRAM: {e}"),
    }
}

fn load_sram_into(emu: &Emulator, rom_path: &PathBuf) {
    let path = sram_path(rom_path);
    match std::fs::read(&path) {
        Ok(data) => {
            emu.load_cart_ram(&data);
            println!("SRAM loaded from {}", path.display());
        }
        Err(_) => {} // No save file yet — that's fine
    }
}

// ── EEPROM helpers ─────────────────────────────────────────────────────────────

fn eeprom_path(rom_path: &PathBuf) -> PathBuf {
    rom_path.with_extension("eep")
}

fn save_eeprom(emu: &Emulator, rom_path: &PathBuf) {
    if let Some(data) = emu.get_eeprom_data() {
        let path = eeprom_path(rom_path);
        match std::fs::write(&path, &data) {
            Ok(_) => {
                emu.clear_eeprom_dirty();
                println!("EEPROM saved to {}", path.display());
            }
            Err(e) => eprintln!("Failed to save EEPROM: {e}"),
        }
    }
}

fn load_eeprom_into(emu: &Emulator, rom_path: &PathBuf) {
    if !emu.has_eeprom() { return; }
    let path = eeprom_path(rom_path);
    match std::fs::read(&path) {
        Ok(data) => {
            emu.load_eeprom_data(&data);
            println!("EEPROM loaded from {}", path.display());
        }
        Err(_) => {} // No save file yet — that's fine
    }
}

// ── Save-state helpers ──────────────────────────────────────────────────────────

fn savestate_path(rom_path: &PathBuf, slot: usize) -> PathBuf {
    let stem = rom_path.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
    let ext  = rom_path.extension().and_then(|s| s.to_str()).unwrap_or("sms");
    let name = format!("{}.{}.ss{}", stem, ext, slot);
    rom_path.with_file_name(name)
}

fn save_state_to_slot(emu: &crate::core::Emulator, rom_path: &PathBuf, slot: usize) {
    let state = emu.save_state();
    let bytes = state.serialize();
    let path  = savestate_path(rom_path, slot);
    match std::fs::write(&path, &bytes) {
        Ok(_) => println!("Save state written to slot {} ({})", slot, path.display()),
        Err(e) => eprintln!("Failed to write save state: {e}"),
    }
}

fn load_state_from_slot(emu: &mut crate::core::Emulator, rom_path: &PathBuf, slot: usize) {
    let path = savestate_path(rom_path, slot);
    match std::fs::read(&path) {
        Ok(data) => {
            match crate::savestate::SaveState::deserialize(&data) {
                Some(state) => {
                    emu.load_state(state);
                    println!("Save state loaded from slot {} ({})", slot, path.display());
                }
                None => eprintln!("Save state in slot {} is invalid or incompatible", slot),
            }
        }
        Err(_) => eprintln!("No save state in slot {}", slot),
    }
}

// ── ROM loader ─────────────────────────────────────────────────────────────────

fn load_rom(path: &PathBuf, sample_rate: f32, fm_disabled: bool) -> Option<Emulator> {
    match std::fs::read(path) {
        Ok(data) => {
            let is_gg = path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("gg"))
                .unwrap_or(false);
            let emu = Emulator::new(data, is_gg, sample_rate);
            emu.cpu.io.bus.borrow_mut().mixer.fm.user_disabled = !is_gg && fm_disabled;
            load_sram_into(&emu, path);
            load_eeprom_into(&emu, path);
            println!("Loaded ROM: {} (GG: {})", path.file_stem().and_then(|n| n.to_str()).unwrap_or("?"), is_gg);
            Some(emu)
        }
        Err(e) => { eprintln!("Failed to load ROM: {e}"); None }
    }
}

// ── Key config ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct PlayerKeys {
    up: Key, down: Key, left: Key, right: Key,
    b1: Key, b2: Key, start: Key,
}

impl PlayerKeys {
    fn p1() -> Self {
        Self { up: Key::ArrowUp, down: Key::ArrowDown, left: Key::ArrowLeft, right: Key::ArrowRight,
               b1: Key::Z, b2: Key::X, start: Key::Enter }
    }
    fn p2() -> Self {
        Self { up: Key::W, down: Key::S, left: Key::A, right: Key::D,
               b1: Key::Num1, b2: Key::Num2, start: Key::Num3 }
    }
    fn get(&self, action: usize) -> Key {
        [self.up, self.down, self.left, self.right, self.b1, self.b2, self.start][action]
    }
    fn set(&mut self, action: usize, key: Key) {
        match action {
            0 => self.up = key, 1 => self.down = key, 2 => self.left = key,
            3 => self.right = key, 4 => self.b1 = key, 5 => self.b2 = key,
            6 => self.start = key, _ => {}
        }
    }
}

#[derive(Clone)]
struct KeyConfig { p1: PlayerKeys, p2: PlayerKeys }

impl Default for KeyConfig {
    fn default() -> Self { Self { p1: PlayerKeys::p1(), p2: PlayerKeys::p2() } }
}

fn key_label(k: Key) -> &'static str {
    match k {
        Key::ArrowUp => "↑", Key::ArrowDown => "↓",
        Key::ArrowLeft => "←", Key::ArrowRight => "→",
        Key::Enter => "Enter", Key::Space => "Space",
        Key::Escape => "Esc", Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::A => "A", Key::B => "B", Key::C => "C", Key::D => "D",
        Key::E => "E", Key::F => "F", Key::G => "G", Key::H => "H",
        Key::I => "I", Key::J => "J", Key::K => "K", Key::L => "L",
        Key::M => "M", Key::N => "N", Key::O => "O", Key::P => "P",
        Key::Q => "Q", Key::R => "R", Key::S => "S", Key::T => "T",
        Key::U => "U", Key::V => "V", Key::W => "W", Key::X => "X",
        Key::Y => "Y", Key::Z => "Z",
        Key::Num0 => "0", Key::Num1 => "1", Key::Num2 => "2", Key::Num3 => "3",
        Key::Num4 => "4", Key::Num5 => "5", Key::Num6 => "6", Key::Num7 => "7",
        Key::Num8 => "8", Key::Num9 => "9",
        Key::F1 => "F1", Key::F2 => "F2", Key::F3 => "F3", Key::F4 => "F4",
        Key::F5 => "F5", Key::F6 => "F6", Key::F7 => "F7", Key::F8 => "F8",
        Key::F9 => "F9", Key::F10 => "F10", Key::F11 => "F11", Key::F12 => "F12",
        _ => "?",
    }
}

// ── Pad state ──────────────────────────────────────────────────────────────────

#[derive(Default)]
struct PadState { up: bool, down: bool, left: bool, right: bool, b1: bool, b2: bool, start: bool }

// ── App ────────────────────────────────────────────────────────────────────────

struct VibeApp {
    emu:         Option<Emulator>,
    rom_path:    Option<PathBuf>,
    fm_disabled: bool,

    audio_buf:   Arc<Mutex<Vec<f32>>>,
    sample_rate: f32,
    _stream:     Option<Stream>,

    texture:     Option<TextureHandle>,
    fb:          Vec<u32>,

    gilrs:       Gilrs,
    pad:         PadState,
    key_config:  KeyConfig,
    mx: u16, my: u16,
    trigger_frames: u8,

    last_frame:   Instant,
    time_debt_us: i64,
    sram_save_timer: u32, // frames since last SRAM save check

    // Save state
    save_slot: usize, // 1–9

    // UI state
    show_key_config: bool,
    show_about:      bool,
    show_fm_notice:  bool,
    show_slot_hud:   u8,  // frames remaining to display the slot HUD
    binding:         Option<(usize, usize)>, // (player 0/1, action 0-6)
    window_title:    String,
}

impl VibeApp {
    fn new(_cc: &eframe::CreationContext<'_>, initial_rom: Option<String>) -> Self {
        let (audio_buf, sample_rate, stream) = build_audio_stream();
        let gilrs = Gilrs::new().expect("Failed to init gilrs");
        let mut app = Self {
            emu: None, rom_path: None, fm_disabled: false,
            audio_buf, sample_rate, _stream: stream,
            texture: None, fb: vec![0u32; SMS_W * SMS_H],
            gilrs, pad: PadState::default(), key_config: KeyConfig::default(),
            mx: 0, my: 0, trigger_frames: 0,
            last_frame: Instant::now(), time_debt_us: 0, sram_save_timer: 0,
            save_slot: 1,
            show_key_config: false, show_about: false, show_fm_notice: false,
            show_slot_hud: 0,
            binding: None,
            window_title: "vibe-sms".to_string(),
        };
        if let Some(path_str) = initial_rom {
            let p = PathBuf::from(path_str);
            if let Some(e) = load_rom(&p, app.sample_rate, app.fm_disabled) {
                app.rom_path = Some(p);
                app.emu = Some(e);
            }
        }
        app
    }
}

// ── App::update ────────────────────────────────────────────────────────────────

impl eframe::App for VibeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Timing ────────────────────────────────────────────────────────────
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame).as_micros().min(50_000) as i64;
        self.last_frame = now;
        self.time_debt_us = (self.time_debt_us + elapsed).min(SMS_FRAME_US * 2);

        // ── Gamepad ───────────────────────────────────────────────────────────
        while let Some(GilrsEvent { id, .. }) = self.gilrs.next_event() {
            let gp = self.gilrs.gamepad(id);
            self.pad.up    = gp.is_pressed(Button::DPadUp);
            self.pad.down  = gp.is_pressed(Button::DPadDown);
            self.pad.left  = gp.is_pressed(Button::DPadLeft);
            self.pad.right = gp.is_pressed(Button::DPadRight);
            self.pad.b1    = gp.is_pressed(Button::South) || gp.is_pressed(Button::West);
            self.pad.b2    = gp.is_pressed(Button::East)  || gp.is_pressed(Button::North);
            self.pad.start = gp.is_pressed(Button::Start);
        }

        // ── Key binding capture ───────────────────────────────────────────────
        if self.binding.is_some() {
            if let Some(key) = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, .. } => Some(*key),
                    _ => None,
                })
            }) {
                if let Some((player, action)) = self.binding.take() {
                    if player == 0 { self.key_config.p1.set(action, key); }
                    else           { self.key_config.p2.set(action, key); }
                }
            }
        }

        // ── Save-state keyboard shortcuts ─────────────────────────────────────
        // Only handle when not in the binding dialog
        if self.binding.is_none() {
            let (f5, f7, slot_key) = ctx.input(|i| {
                let f5 = i.key_pressed(Key::F5);
                let f7 = i.key_pressed(Key::F7);
                let slot = [
                    Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5,
                    Key::Num6, Key::Num7, Key::Num8, Key::Num9,
                ].iter().enumerate().find_map(|(idx, &k)| {
                    if i.key_pressed(k) { Some(idx + 1) } else { None }
                });
                (f5, f7, slot)
            });

            if let Some(slot) = slot_key {
                self.save_slot = slot;
                self.show_slot_hud = 90; // show for 90 frames (~1.5s)
            }
            if f7 {
                if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                    save_state_to_slot(e, p, self.save_slot);
                    self.show_slot_hud = 90;
                }
            }
            if f5 {
                let slot = self.save_slot;
                let rom_path = self.rom_path.clone();
                if let (Some(ref mut e), Some(ref p)) = (&mut self.emu, &rom_path) {
                    load_state_from_slot(e, p, slot);
                    self.show_slot_hud = 90;
                }
            }
        }

        // ── Read keyboard + mouse state ───────────────────────────────────────
        let is_gg = self.emu.as_ref().map(|e| e.is_gg).unwrap_or(false);
        let (ku, kd, kl, kr, kb1, kb2, kstart, mouse_down) = ctx.input(|i| {
            let kc = &self.key_config; let p = &self.pad;
            (
                i.key_down(kc.p1.up)    || p.up,
                i.key_down(kc.p1.down)  || p.down,
                i.key_down(kc.p1.left)  || p.left,
                i.key_down(kc.p1.right) || p.right,
                i.key_down(kc.p1.b1)   || p.b1,
                i.key_down(kc.p1.b2)   || p.b2,
                i.key_down(kc.p1.start) || p.start,
                i.pointer.primary_down(),
            )
        });
        if mouse_down && self.trigger_frames == 0 { self.trigger_frames = 6; }

        // ── Menu bar ──────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // ── Emulator ──────────────────────────────────────────────────
                ui.menu_button("Emulator", |ui| {
                    if ui.button("Open ROM…").clicked() {
                        ui.close();
                        let path = rfd::FileDialog::new()
                            .add_filter("Sega 8-bit ROMs", &["sms", "sg", "gg", "SMS", "SG", "GG"])
                            .pick_file();
                        if let Some(p) = path {
                            // Save current SRAM/EEPROM before replacing the emulator
                            if let (Some(ref e), Some(ref rp)) = (&self.emu, &self.rom_path) {
                                if e.is_sram_dirty()   { save_sram(e, rp); }
                                if e.is_eeprom_dirty() { save_eeprom(e, rp); }
                            }
                            if let Some(e) = load_rom(&p, self.sample_rate, self.fm_disabled) {
                                self.rom_path = Some(p);
                                self.emu = Some(e);
                                self.sram_save_timer = 0;
                            }
                        }
                    }
                    ui.separator();
                    ui.add_enabled_ui(self.rom_path.is_some(), |ui| {
                        if ui.button("Reset").clicked() {
                            ui.close();
                            // Save SRAM/EEPROM before reset
                            if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                                if e.is_sram_dirty()   { save_sram(e, p); }
                                if e.is_eeprom_dirty() { save_eeprom(e, p); }
                            }
                            if let Some(ref p) = self.rom_path.clone() {
                                self.emu = load_rom(p, self.sample_rate, self.fm_disabled);
                                self.sram_save_timer = 0;
                            }
                        }
                        if ui.button("Stop").clicked() {
                            ui.close();
                            // Save SRAM/EEPROM before stopping
                            if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                                if e.is_sram_dirty()   { save_sram(e, p); }
                                if e.is_eeprom_dirty() { save_eeprom(e, p); }
                            }
                            self.emu = None;
                            self.rom_path = None;
                            self.fb.iter_mut().for_each(|p| *p = 0);
                        }
                    });
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                // ── State ─────────────────────────────────────────────────────
                ui.menu_button("State", |ui| {
                    ui.add_enabled_ui(self.rom_path.is_some(), |ui| {
                        ui.label(format!("Slot: {}", self.save_slot));
                        ui.separator();
                        for slot in 1..=9usize {
                            let label = format!("Slot {} — Save  [{}+F7]", slot, slot);
                            if ui.button(label).clicked() {
                                ui.close();
                                self.save_slot = slot;
                                if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                                    save_state_to_slot(e, p, slot);
                                }
                            }
                        }
                        ui.separator();
                        for slot in 1..=9usize {
                            let label = format!("Slot {} — Load  [{}+F5]", slot, slot);
                            if ui.button(label).clicked() {
                                ui.close();
                                self.save_slot = slot;
                                let rom_path = self.rom_path.clone();
                                if let (Some(ref mut e), Some(ref p)) = (&mut self.emu, &rom_path) {
                                    load_state_from_slot(e, p, slot);
                                }
                            }
                        }
                        ui.separator();
                        ui.label(egui::RichText::new("Press 1–9 to select slot").small().color(egui::Color32::GRAY));
                        ui.label(egui::RichText::new("F7 = Save  ·  F5 = Load").small().color(egui::Color32::GRAY));
                    });
                });

                // ── Configuration ─────────────────────────────────────────────
                ui.menu_button("Configuration", |ui| {
                    if ui.button("Controls…").clicked() {
                        ui.close();
                        self.show_key_config = true;
                    }
                    ui.separator();
                    let mut fm_on = !self.fm_disabled;
                    let fm_changed = ui.add_enabled(!is_gg, egui::Checkbox::new(&mut fm_on, "FM Sound")).changed();
                    if fm_changed {
                        self.fm_disabled = !fm_on;
                        self.show_fm_notice = true;
                    }
                    if is_gg {
                        ui.label(egui::RichText::new("(SMS only)").small().color(egui::Color32::GRAY));
                    }
                });

                // ── About ─────────────────────────────────────────────────────
                ui.menu_button("About", |ui| {
                    if ui.button("About vibe-sms…").clicked() {
                        ui.close();
                        self.show_about = true;
                    }
                });

                // Title right-aligned — must be LAST (consumes remaining space)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(&self.window_title).strong());
                });
            });
        });

        // ── Central panel — emulator display ──────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            let panel_rect = ui.max_rect();
            let (emu_w, emu_h) = if is_gg { (GG_W as f32, GG_H as f32) } else { (SMS_W as f32, SMS_H as f32) };
            let render_rect = letterbox_rect(panel_rect, emu_w / emu_h);

            // Mouse → light phaser coords
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                let rx = (pos.x - render_rect.min.x) / render_rect.width();
                let ry = (pos.y - render_rect.min.y) / render_rect.height();
                if (0.0..=1.0).contains(&rx) { self.mx = (rx * emu_w) as u16; }
                if (0.0..=1.0).contains(&ry) { self.my = (ry * emu_h) as u16; }
            }

            // Emulation step
            if self.time_debt_us >= SMS_FRAME_US {
                self.time_debt_us -= SMS_FRAME_US;
                let trigger_active = self.trigger_frames > 0;
                if self.trigger_frames > 0 { self.trigger_frames -= 1; }

                if let Some(ref mut e) = self.emu {
                    e.cpu.io.bus.borrow_mut().mixer.fm.user_disabled = self.fm_disabled;
                    e.set_input(ku, kd, kl, kr, kb1 || trigger_active, kb2, kstart);
                    e.set_lightgun(trigger_active, self.mx.min(255), self.my.min(191));

                    let (_, mut samples) = e.step_frame();
                    if let Ok(mut buf) = self.audio_buf.try_lock() {
                        buf.append(&mut samples);
                        if buf.len() > 8192 { let excess = buf.len() - 8192; buf.drain(0..excess); }
                    }

                    // Auto-save SRAM/EEPROM every ~5 seconds (300 frames) when dirty
                    self.sram_save_timer += 1;
                    if self.sram_save_timer >= 300 {
                        self.sram_save_timer = 0;
                        if let Some(ref p) = self.rom_path {
                            if e.is_sram_dirty()   { save_sram(e, p); }
                            if e.is_eeprom_dirty() { save_eeprom(e, p); }
                        }
                    }

                    // Blit framebuffer
                    let frame = e.get_framebuffer();
                    let (rw, rh, xo, yo) = if is_gg { (GG_W, GG_H, 48, 24) } else { (SMS_W, SMS_H, 0, 0) };
                    let (bx, by) = if is_gg { ((SMS_W - GG_W) / 2, (SMS_H - GG_H) / 2) } else { (0, 0) };
                    self.fb.iter_mut().for_each(|p| *p = 0);
                    for y in 0..rh {
                        for x in 0..rw {
                            let px = frame[(y + yo) * SMS_W + (x + xo)];
                            self.fb[(by + y) * SMS_W + (bx + x)] = px & 0x00FF_FFFF;
                        }
                    }
                } else {
                    self.fb.iter_mut().for_each(|p| *p = 0);
                }
            }

            // Convert XRGB → RGBA8 and upload to GPU texture
            let rgba: Vec<u8> = self.fb.iter().flat_map(|&p| {
                [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255u8]
            }).collect();
            let img = ColorImage::from_rgba_unmultiplied([SMS_W, SMS_H], &rgba);
            match &mut self.texture {
                Some(t) => t.set(img, TextureOptions::NEAREST),
                None    => self.texture = Some(ctx.load_texture("fb", img, TextureOptions::NEAREST)),
            }

            // Draw letterboxed
            if let Some(ref tex) = self.texture {
                ui.painter().image(
                    tex.id(), render_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }

            // Slot HUD overlay
            if self.show_slot_hud > 0 {
                self.show_slot_hud -= 1;
                let alpha = ((self.show_slot_hud as f32 / 90.0) * 220.0) as u8;
                let hud = format!("Slot {}", self.save_slot);
                let pos = egui::pos2(render_rect.min.x + 8.0, render_rect.min.y + 8.0);
                ui.painter().text(
                    pos + egui::vec2(1.0, 1.0),
                    egui::Align2::LEFT_TOP,
                    &hud,
                    egui::FontId::proportional(20.0),
                    egui::Color32::from_black_alpha(alpha),
                );
                ui.painter().text(
                    pos,
                    egui::Align2::LEFT_TOP,
                    &hud,
                    egui::FontId::proportional(20.0),
                    egui::Color32::from_rgba_unmultiplied(255, 255, 0, alpha),
                );
            }
        });

        // ── Controls window ───────────────────────────────────────────────────
        let mut show_key_config = self.show_key_config;
        egui::Window::new("Controls")
            .open(&mut show_key_config)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                const ACTIONS: &[&str] = &["Up", "Down", "Left", "Right", "Button 1", "Button 2", "Start/Pause"];
                egui::Grid::new("ctrl_grid").num_columns(3).striped(true).show(ui, |ui| {
                    ui.strong("Action"); ui.strong("Player 1"); ui.strong("Player 2");
                    ui.end_row();
                    for (ai, &name) in ACTIONS.iter().enumerate() {
                        ui.label(name);
                        for pi in 0..2usize {
                            let keys = if pi == 0 { &self.key_config.p1 } else { &self.key_config.p2 };
                            let waiting = self.binding == Some((pi, ai));
                            let lbl = if waiting { "Press any key…".to_string() } else { key_label(keys.get(ai)).to_string() };
                            if ui.button(lbl).clicked() {
                                self.binding = if waiting { None } else { Some((pi, ai)) };
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.separator();
                if ui.button("Reset to defaults").clicked() { self.key_config = KeyConfig::default(); }
            });
        self.show_key_config = show_key_config;

        // ── FM notice ─────────────────────────────────────────────────────────
        if self.show_fm_notice {
            egui::Window::new("FM Sound Changed")
                .collapsible(false).resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("FM sound setting changed.");
                    ui.label("Reset the game (Emulator → Reset) for the change to take effect.");
                    ui.separator();
                    if ui.button("  OK  ").clicked() { self.show_fm_notice = false; }
                });
        }

        // ── About window ──────────────────────────────────────────────────────
        let mut show_about = self.show_about;
        egui::Window::new("About vibe-sms")
            .open(&mut show_about)
            .collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.heading("vibe-sms");
                ui.label("Version 0.1.1");
                ui.separator();
                ui.label("Sega Master System / Game Gear emulator — written in Rust.");
                ui.label("Supports FM sound (YM2413), PSG (SN76489), gamepad, and Light Phaser.");
                ui.separator();
                ui.label("Built with:  egui/eframe · cpal · gilrs · z80");
                ui.separator();
                ui.label("Created using Google Antigravity powered by Gemini.");
            });
        self.show_about = show_about;

        // ── Window title ──────────────────────────────────────────────────────
        // Compute what the title should be
        let desired_title = "vibe-sms".to_string();
        // Update and send every frame — Wayland async delivery needs this
        self.window_title = desired_title;
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title.clone()));

        // Pace repaints to ~60 fps
        ctx.request_repaint_after(Duration::from_micros(SMS_FRAME_US as u64));
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Returns the largest rect inside `panel` that preserves `aspect` (w/h).
fn letterbox_rect(panel: egui::Rect, aspect: f32) -> egui::Rect {
    let pw = panel.width();
    let ph = panel.height();
    let (w, h) = if pw / ph > aspect { (ph * aspect, ph) } else { (pw, pw / aspect) };
    egui::Rect::from_center_size(panel.center(), egui::vec2(w, h))
}

// ── Entry point ────────────────────────────────────────────────────────────────

pub fn launch_frontend(initial_rom: Option<String>) {
    // Load icon from assets at compile time
    let icon = load_icon();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("vibe-sms")
        .with_inner_size([512.0, 406.0])
        .with_resizable(true);
    if let Some(icon) = icon { viewport = viewport.with_icon(icon); }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "vibe-sms",
        native_options,
        Box::new(|cc| Ok(Box::new(VibeApp::new(cc, initial_rom)))),
    ).expect("Failed to run eframe");
}

fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../assets/icon.png");
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width: w, height: h })
}
