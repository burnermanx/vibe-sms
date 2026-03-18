use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::path::PathBuf;

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version};
use glutin::config::GlConfig;
use glutin::display::GlDisplay;
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use cpal::Stream;
use gilrs::{Button, Event as GilrsEvent, Gilrs};
#[cfg(target_os = "linux")]
use glib;

use crate::core::Emulator;
use crate::platform::Platform;
use crate::frontend::egui_ui::{DialogState, EguiState};
use crate::frontend::input::{KeyConfig, PadState};
use crate::frontend::menu::{AppMenu, MenuAction};
use crate::frontend::renderer::Renderer;

const SMS_W: usize = 256;
const SMS_H: usize = 192;
const GG_W:  usize = 160;
const GG_H:  usize = 144;
const SMS_FRAME_US: i64 = 16_683;

fn sram_path(p: &PathBuf) -> PathBuf { p.with_extension("sav") }
fn eeprom_path(p: &PathBuf) -> PathBuf { p.with_extension("eep") }

fn save_sram(emu: &Emulator, rom_path: &PathBuf) {
    let data = emu.get_cart_ram();
    match std::fs::write(sram_path(rom_path), &data) {
        Ok(_) => { emu.clear_sram_dirty(); }
        Err(e) => eprintln!("Failed to save SRAM: {e}"),
    }
}

fn load_sram_into(emu: &Emulator, rom_path: &PathBuf) {
    if let Ok(data) = std::fs::read(sram_path(rom_path)) {
        emu.load_cart_ram(&data);
    }
}

fn save_eeprom(emu: &Emulator, rom_path: &PathBuf) {
    if let Some(data) = emu.get_eeprom_data() {
        match std::fs::write(eeprom_path(rom_path), &data) {
            Ok(_) => { emu.clear_eeprom_dirty(); }
            Err(e) => eprintln!("Failed to save EEPROM: {e}"),
        }
    }
}

fn load_eeprom_into(emu: &Emulator, rom_path: &PathBuf) {
    if !emu.has_eeprom() { return; }
    if let Ok(data) = std::fs::read(eeprom_path(rom_path)) {
        emu.load_eeprom_data(&data);
    }
}

fn savestate_path(rom_path: &PathBuf, slot: usize) -> PathBuf {
    let stem = rom_path.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
    let ext  = rom_path.extension().and_then(|s| s.to_str()).unwrap_or("sms");
    rom_path.with_file_name(format!("{}.{}.ss{}", stem, ext, slot))
}

fn save_state_to_slot(emu: &Emulator, rom_path: &PathBuf, slot: usize) {
    let bytes = emu.save_state().serialize();
    if let Err(e) = std::fs::write(savestate_path(rom_path, slot), &bytes) {
        eprintln!("Failed to write save state: {e}");
    }
}

fn load_state_from_slot(emu: &mut Emulator, rom_path: &PathBuf, slot: usize) {
    let path = savestate_path(rom_path, slot);
    match std::fs::read(&path) {
        Ok(data) => match crate::savestate::SaveState::deserialize(&data) {
            Some(state) => emu.load_state(state),
            None => eprintln!("Save state in slot {} is invalid", slot),
        },
        Err(_) => eprintln!("No save state in slot {}", slot),
    }
}

pub fn load_rom(path: &PathBuf, sample_rate: f32, fm_disabled: bool) -> Option<Emulator> {
    match std::fs::read(path) {
        Ok(data) => {
            let platform = match path.extension()
                .and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref()
            {
                Some("gg") => Platform::GameGear,
                Some("sg") => Platform::Sg1000,
                Some("sc") => Platform::Sc3000,
                _          => Platform::MasterSystem,
            };
            let emu = Emulator::new(data, platform, sample_rate);
            emu.cpu.io.bus.borrow_mut().mixer.fm.user_disabled =
                platform != Platform::MasterSystem || fm_disabled;
            load_sram_into(&emu, path);
            load_eeprom_into(&emu, path);
            println!("Loaded ROM: {} ({:?})",
                path.file_stem().and_then(|n| n.to_str()).unwrap_or("?"), platform);
            Some(emu)
        }
        Err(e) => { eprintln!("Failed to load ROM: {e}"); None }
    }
}

struct GlState {
    ctx:     PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    gl:      Arc<glow::Context>,
}

fn init_gl(window: &Window) -> GlState {
    let display_handle = window.display_handle().unwrap();
    let window_handle  = window.window_handle().unwrap();

    let display = unsafe {
        #[cfg(target_os = "linux")]
        let pref = glutin::display::DisplayApiPreference::EglThenGlx(Box::new(
            winit::platform::x11::register_xlib_error_hook
        ));
        #[cfg(target_os = "windows")]
        let pref = glutin::display::DisplayApiPreference::Wgl(Some(window_handle.as_raw()));
        #[cfg(target_os = "macos")]
        let pref = glutin::display::DisplayApiPreference::Cgl;
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        let pref = glutin::display::DisplayApiPreference::Egl;

        glutin::display::Display::new(display_handle.as_raw(), pref).unwrap()
    };

    let template = ConfigTemplateBuilder::new().build();
    let config = unsafe {
        display.find_configs(template).unwrap()
            .reduce(|a, b| if a.num_samples() > b.num_samples() { a } else { b })
            .unwrap()
    };

    let size = window.inner_size();
    let surf_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window_handle.as_raw(),
        NonZeroU32::new(size.width.max(1)).unwrap(),
        NonZeroU32::new(size.height.max(1)).unwrap(),
    );
    let surface = unsafe { display.create_window_surface(&config, &surf_attrs).unwrap() };

    let ctx_attrs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(window_handle.as_raw()));
    let ctx_attrs_gles = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(Some(Version::new(2, 0))))
        .build(Some(window_handle.as_raw()));

    let not_current = unsafe {
        display.create_context(&config, &ctx_attrs)
            .or_else(|_| display.create_context(&config, &ctx_attrs_gles))
            .unwrap()
    };
    let ctx = not_current.make_current(&surface).unwrap();

    let gl = Arc::new(unsafe {
        glow::Context::from_loader_function_cstr(|s| {
            display.get_proc_address(s) as *const _
        })
    });

    GlState { ctx, surface, gl }
}

pub struct VibeApp {
    // Pre-init
    initial_rom: Option<String>,
    audio_buf:   Arc<Mutex<Vec<f32>>>,
    sample_rate: f32,
    _stream:     Option<Stream>,
    gilrs:       Gilrs,
    menu:        AppMenu,
    proxy:       EventLoopProxy<MenuAction>,

    // Deferred (created in resumed())
    window:      Option<Arc<Window>>,
    gl_state:    Option<GlState>,
    renderer:    Option<Renderer>,
    egui_state:  Option<EguiState>,

    // Emulation
    emu:             Option<Emulator>,
    rom_path:        Option<PathBuf>,
    fb:              Vec<u32>,
    pad:             PadState,
    pressed_keys:    HashSet<KeyCode>,
    mx: u16, my: u16,
    trigger_frames:  u8,
    last_frame:      Instant,
    time_debt_us:    i64,
    sram_save_timer: u32,

    // UI state
    dialog: DialogState,
}

impl VibeApp {
    pub fn new(
        initial_rom: Option<String>,
        audio_buf: Arc<Mutex<Vec<f32>>>,
        sample_rate: f32,
        stream: Option<Stream>,
        gilrs: Gilrs,
        menu: AppMenu,
        proxy: EventLoopProxy<MenuAction>,
    ) -> Self {
        Self {
            initial_rom,
            audio_buf,
            sample_rate,
            _stream: stream,
            gilrs,
            menu,
            proxy,
            window: None,
            gl_state: None,
            renderer: None,
            egui_state: None,
            emu: None,
            rom_path: None,
            fb: vec![0u32; SMS_W * SMS_H],
            pad: PadState::default(),
            pressed_keys: HashSet::new(),
            mx: 0, my: 0,
            trigger_frames: 0,
            last_frame: Instant::now(),
            time_debt_us: 0,
            sram_save_timer: 0,
            dialog: DialogState {
                show_key_config: false,
                show_about:      false,
                show_fm_notice:  false,
                show_slot_hud:   0,
                save_slot:       1,
                binding:         None,
                key_config:      KeyConfig::default(),
                fm_disabled:      false,
                rom_loaded:       false,
                menu_bar_height:  0.0,
            },
        }
    }

    fn render(&mut self) {
        let window = match self.window.as_ref() { Some(w) => w.clone(), None => return };
        let gl = match self.gl_state.as_ref().map(|s| s.gl.clone()) { Some(g) => g, None => return };

        // Timing
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame).as_micros().min(50_000) as i64;
        self.last_frame = now;
        self.time_debt_us = (self.time_debt_us + elapsed).min(SMS_FRAME_US * 2);

        // Gamepad input
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

        let is_gg = self.emu.as_ref().map(|e| e.platform.is_gg()).unwrap_or(false);
        let is_sg = self.emu.as_ref().map(|e| e.platform.is_sg_family()).unwrap_or(false);
        let pk = &self.pressed_keys;
        let kc = &self.dialog.key_config;
        let p  = &self.pad;
        let ku     = pk.contains(&kc.p1.up)    || p.up;
        let kd     = pk.contains(&kc.p1.down)  || p.down;
        let kl     = pk.contains(&kc.p1.left)  || p.left;
        let kr     = pk.contains(&kc.p1.right) || p.right;
        let kb1    = pk.contains(&kc.p1.b1)    || p.b1;
        let kb2    = pk.contains(&kc.p1.b2)    || p.b2;
        let kstart = pk.contains(&kc.p1.start) || p.start;

        // Step emulation
        if self.time_debt_us >= SMS_FRAME_US {
            self.time_debt_us -= SMS_FRAME_US;
            let trigger_active = self.trigger_frames > 0;
            if self.trigger_frames > 0 { self.trigger_frames -= 1; }

            if let Some(ref mut e) = self.emu {
                e.cpu.io.bus.borrow_mut().mixer.fm.user_disabled =
                    is_sg || is_gg || self.dialog.fm_disabled;
                e.set_input(ku, kd, kl, kr, kb1 || trigger_active, kb2, kstart);
                e.set_lightgun(trigger_active, self.mx.min(255), self.my.min(191));

                let (_, mut samples) = e.step_frame();
                if let Ok(mut buf) = self.audio_buf.try_lock() {
                    buf.append(&mut samples);
                    if buf.len() > 8192 { let excess = buf.len() - 8192; buf.drain(0..excess); }
                }

                self.sram_save_timer += 1;
                if self.sram_save_timer >= 300 {
                    self.sram_save_timer = 0;
                    if let Some(ref p) = self.rom_path {
                        if e.is_sram_dirty()   { save_sram(e, p); }
                        if e.is_eeprom_dirty() { save_eeprom(e, p); }
                    }
                }

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

        // Render frame
        if let Some(ref renderer) = self.renderer {
            renderer.upload_frame(&gl, &self.fb);
            let size = window.inner_size();
            // Convert egui-point menu bar height → physical pixels
            let top_px = (self.dialog.menu_bar_height * window.scale_factor() as f32) as u32;
            renderer.draw(&gl, (size.width, size.height), is_gg, top_px);
        }

        // UI overlay
        let proxy = self.proxy.clone();
        self.dialog.rom_loaded = self.rom_path.is_some();
        if let Some(ref mut egui_state) = self.egui_state {
            egui_state.run_frame(
                &window, &gl, &mut self.dialog,
                &|action| { let _ = proxy.send_event(action); },
                is_gg, is_sg,
            );
        }

        // Present
        if let Some(ref state) = self.gl_state {
            state.surface.swap_buffers(&state.ctx).ok();
        }
    }

    fn handle_menu_action(&mut self, action: MenuAction, elwt: &ActiveEventLoop) {
        match action {
            MenuAction::OpenRom => {
                if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                    if e.is_sram_dirty()   { save_sram(e, p); }
                    if e.is_eeprom_dirty() { save_eeprom(e, p); }
                }
                // GTK is single-threaded: spawn the async dialog on the glib main
                // context (main thread).  about_to_wait() pumps that context each
                // frame so the dialog renders without blocking winit.
                #[cfg(target_os = "linux")]
                {
                    let proxy = self.proxy.clone();
                    glib::MainContext::default().spawn_local(async move {
                        if let Some(handle) = rfd::AsyncFileDialog::new()
                            .add_filter("Sega 8-bit ROMs", &["sms", "sg", "sc", "gg", "SMS", "SG", "SC", "GG"])
                            .pick_file()
                            .await
                        {
                            let _ = proxy.send_event(MenuAction::RomSelected(handle.path().to_path_buf()));
                        }
                    });
                }
                #[cfg(not(target_os = "linux"))]
                {
                    // Use AsyncFileDialog so that on macOS rfd can internally
                    // dispatch to the main thread via GCD (NSOpenPanel requires it).
                    // pollster::block_on parks the spawned thread until GCD signals
                    // completion — the winit main thread keeps running normally.
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        let handle = pollster::block_on(
                            rfd::AsyncFileDialog::new()
                                .add_filter("Sega 8-bit ROMs", &["sms", "sg", "sc", "gg", "SMS", "SG", "SC", "GG"])
                                .pick_file(),
                        );
                        if let Some(h) = handle {
                            let _ = proxy.send_event(MenuAction::RomSelected(h.path().to_path_buf()));
                        }
                    });
                }
            }
            MenuAction::RomSelected(p) => {
                if let Some(e) = load_rom(&p, self.sample_rate, self.dialog.fm_disabled) {
                    self.rom_path = Some(p);
                    self.emu = Some(e);
                    self.sram_save_timer = 0;
                }
            }
            MenuAction::Reset => {
                if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                    if e.is_sram_dirty()   { save_sram(e, p); }
                    if e.is_eeprom_dirty() { save_eeprom(e, p); }
                }
                if let Some(ref p) = self.rom_path.clone() {
                    self.emu = load_rom(p, self.sample_rate, self.dialog.fm_disabled);
                    self.sram_save_timer = 0;
                }
            }
            MenuAction::Stop => {
                if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                    if e.is_sram_dirty()   { save_sram(e, p); }
                    if e.is_eeprom_dirty() { save_eeprom(e, p); }
                }
                self.emu = None;
                self.rom_path = None;
                self.fb.iter_mut().for_each(|p| *p = 0);
            }
            MenuAction::Quit => {
                if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                    if e.is_sram_dirty()   { save_sram(e, p); }
                    if e.is_eeprom_dirty() { save_eeprom(e, p); }
                }
                self.shutdown_gl();
                elwt.exit();
            }
            MenuAction::SaveState => {
                if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                    save_state_to_slot(e, p, self.dialog.save_slot);
                    self.dialog.show_slot_hud = 90;
                }
            }
            MenuAction::LoadState => {
                let slot = self.dialog.save_slot;
                let rom_path = self.rom_path.clone();
                if let (Some(ref mut e), Some(ref p)) = (&mut self.emu, &rom_path) {
                    load_state_from_slot(e, p, slot);
                    self.dialog.show_slot_hud = 90;
                }
            }
            MenuAction::SetSlot(slot) => {
                self.dialog.save_slot = slot;
                self.dialog.show_slot_hud = 90;
            }
            MenuAction::ToggleFm => {
                self.dialog.fm_disabled = !self.dialog.fm_disabled;
                self.dialog.show_fm_notice = true;
            }
            MenuAction::ShowControls => { self.dialog.show_key_config = true; }
            MenuAction::ShowAbout    => { self.dialog.show_about = true; }
        }
    }

    /// Free GPU resources in the correct order before the GL context is destroyed.
    ///
    /// Must be called before `event_loop.exit()` so the context is still current.
    /// After this returns, `renderer`, `egui_state`, and `gl_state` are all `None`.
    fn shutdown_gl(&mut self) {
        // 1. Free renderer GPU objects (shaders, VAO, VBO, texture)
        if let (Some(ref renderer), Some(ref state)) = (&self.renderer, &self.gl_state) {
            renderer.destroy(&state.gl);
        }
        self.renderer = None;

        // 2. Free egui painter GPU objects (textures, buffers)
        if let Some(ref mut es) = self.egui_state {
            es.destroy();
        }
        self.egui_state = None;

        // 3. Now safe to drop the GL context and surface
        self.gl_state = None;

        // 4. Finally release the window
        self.window = None;
    }
}

impl ApplicationHandler<MenuAction> for VibeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }

        let mut attrs = WindowAttributes::default()
            .with_title("vibe-sms")
            .with_inner_size(LogicalSize::new(512u32, 446u32))
            .with_resizable(true);

        if let Some(icon) = load_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        self.menu.attach_to_window(&window);

        let gl_state = init_gl(&window);
        let renderer = Renderer::new(&gl_state.gl);
        let egui_state = EguiState::new(gl_state.gl.clone(), &window);

        self.renderer   = Some(renderer);
        self.egui_state = Some(egui_state);
        self.gl_state   = Some(gl_state);
        self.window     = Some(window);

        if let Some(path_str) = self.initial_rom.take() {
            let p = PathBuf::from(path_str);
            if let Some(e) = load_rom(&p, self.sample_rate, self.dialog.fm_disabled) {
                self.rom_path = Some(p);
                self.emu = Some(e);
            }
        }

        self.last_frame = Instant::now();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let consumed = if let (Some(ref mut es), Some(ref w)) = (&mut self.egui_state, &self.window) {
            es.handle_window_event(w, &event)
        } else { false };

        match &event {
            WindowEvent::CloseRequested => {
                if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                    if e.is_sram_dirty()   { save_sram(e, p); }
                    if e.is_eeprom_dirty() { save_eeprom(e, p); }
                }
                self.shutdown_gl();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(ref state) = self.gl_state {
                    state.surface.resize(
                        &state.ctx,
                        NonZeroU32::new(size.width.max(1)).unwrap(),
                        NonZeroU32::new(size.height.max(1)).unwrap(),
                    );
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. }, ..
            } if !consumed => {
                match state {
                    ElementState::Pressed  => { self.pressed_keys.insert(*code); }
                    ElementState::Released => { self.pressed_keys.remove(code); }
                }

                if *state == ElementState::Pressed {
                    // Key binding capture
                    if let Some((player, action)) = self.dialog.binding.take() {
                        if player == 0 { self.dialog.key_config.p1.set(action, *code); }
                        else           { self.dialog.key_config.p2.set(action, *code); }
                        return;
                    }

                    // Save-state hotkeys (only when not binding)
                    match code {
                        KeyCode::F7 => {
                            if let (Some(ref e), Some(ref p)) = (&self.emu, &self.rom_path) {
                                save_state_to_slot(e, p, self.dialog.save_slot);
                                self.dialog.show_slot_hud = 90;
                            }
                        }
                        KeyCode::F5 => {
                            let slot = self.dialog.save_slot;
                            let rom_path = self.rom_path.clone();
                            if let (Some(ref mut e), Some(ref p)) = (&mut self.emu, &rom_path) {
                                load_state_from_slot(e, p, slot);
                                self.dialog.show_slot_hud = 90;
                            }
                        }
                        KeyCode::Digit1 => { self.dialog.save_slot = 1; self.dialog.show_slot_hud = 90; }
                        KeyCode::Digit2 => { self.dialog.save_slot = 2; self.dialog.show_slot_hud = 90; }
                        KeyCode::Digit3 => { self.dialog.save_slot = 3; self.dialog.show_slot_hud = 90; }
                        KeyCode::Digit4 => { self.dialog.save_slot = 4; self.dialog.show_slot_hud = 90; }
                        KeyCode::Digit5 => { self.dialog.save_slot = 5; self.dialog.show_slot_hud = 90; }
                        KeyCode::Digit6 => { self.dialog.save_slot = 6; self.dialog.show_slot_hud = 90; }
                        KeyCode::Digit7 => { self.dialog.save_slot = 7; self.dialog.show_slot_hud = 90; }
                        KeyCode::Digit8 => { self.dialog.save_slot = 8; self.dialog.show_slot_hud = 90; }
                        KeyCode::Digit9 => { self.dialog.save_slot = 9; self.dialog.show_slot_hud = 90; }
                        _ => {}
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } if !consumed => {
                if let Some(ref w) = self.window {
                    let size = w.inner_size();
                    let is_gg = self.emu.as_ref().map(|e| e.platform.is_gg()).unwrap_or(false);
                    let (emu_w, emu_h) = if is_gg { (GG_W as f32, GG_H as f32) } else { (SMS_W as f32, SMS_H as f32) };
                    let win_w  = size.width as f32;
                    let top_px = self.dialog.menu_bar_height * w.scale_factor() as f32;
                    let avail_h = size.height as f32 - top_px;
                    let aspect = emu_w / emu_h;
                    let (rw, rh) = if win_w / avail_h > aspect { (avail_h * aspect, avail_h) } else { (win_w, win_w / aspect) };
                    let rx0 = (win_w - rw) * 0.5;
                    let ry0 = top_px + (avail_h - rh) * 0.5;
                    let rx = (position.x as f32 - rx0) / rw;
                    let ry = (position.y as f32 - ry0) / rh;
                    if (0.0..=1.0).contains(&rx) { self.mx = (rx * emu_w) as u16; }
                    if (0.0..=1.0).contains(&ry) { self.my = (ry * emu_h) as u16; }
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } if !consumed => {
                if self.trigger_frames == 0 { self.trigger_frames = 6; }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MenuAction) {
        self.handle_menu_action(event, event_loop);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Pump the glib main context so that async GTK tasks (e.g. file dialog)
        // make progress between winit frames.
        #[cfg(target_os = "linux")]
        while glib::MainContext::default().iteration(false) {}

        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Safety net: if shutdown_gl() wasn't called before exit() (e.g. via
        // OS kill), free GPU resources here while the context is still valid.
        self.shutdown_gl();
    }
}

fn load_icon() -> Option<winit::window::Icon> {
    let bytes = include_bytes!("../../assets/icon.png");
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}
