# CLAUDE.md

## Project

**vibe-sms** — Sega Master System, Game Gear, SG-1000 and SC-3000 emulator in Rust.
Hardware-accurate Z80 CPU, VDP (TMS9918/315-5246), PSG (SN76489), FM (YM2413), light gun, gamepad.

## Commands

```bash
cargo run --release                        # run (debug is too slow for realtime)
cargo run --release -- path/to/game.sms   # load ROM at startup
cargo build --release
cargo check
cargo clippy
cargo test
```

### Linux build deps (Ubuntu/Debian)

```bash
sudo apt install build-essential cargo rustc \
  libasound2-dev \
  libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev \
  libxkbcommon-dev libgl1-mesa-dev \
  libgtk-3-dev libxdo-dev libwayland-dev \
  libudev-dev pkg-config
```

### Arch

```bash
sudo pacman -S base-devel rustup
```

## Architecture

Emulation core is fully decoupled from the frontend.

```
Emulator (core.rs)
  └── Z80<System> (z80 crate)
        └── System (bus.rs)
              └── Bus (bus.rs)
                    ├── Mmu (mmu.rs)            — ROM paging, SRAM, EEPROM
                    ├── Vdp (vdp.rs)            — TMS9918/315-5246 video
                    ├── Joypad (joypad.rs)      — input + light gun (TH pin)
                    └── AudioMixer (audio/mixer.rs)
                          ├── Psg (audio/psg.rs)     — SN76489
                          └── Fm  (audio/fm.rs)
                                └── Ym2413 (audio/ym2413.rs) — OPLL FM
```

### Frontend stack

```
src/frontend/
├── mod.rs       — launch_frontend(); GTK init (Linux); cpal audio stream
├── app.rs       — VibeApp: ApplicationHandler<MenuAction>; GL init; render loop
├── renderer.rs  — glow/OpenGL quad shader; 256×192 RGBA texture blit; letterbox
├── egui_ui.rs   — EguiState (egui-winit + egui-glow); dialogs; Linux menu bar
├── menu.rs      — MenuAction enum; muda native menus (macOS/Windows only)
└── input.rs     — PlayerKeys (winit::KeyCode); KeyConfig; PadState
```

**Key libs:** `winit 0.30` (window/events) · `glutin 0.32` (GL context/EGL/GLX) · `glow 0.16` (OpenGL) · `muda 0.17` (native OS menus) · `egui 0.33` + `egui-winit` + `egui_glow` (dialogs only) · `cpal` (audio) · `gilrs` (gamepad) · `rfd` (file dialog, gtk3 on Linux)

### Critical implementation notes

**Interior mutability everywhere in core**: `Bus` is `RefCell<Bus>` inside `System` because the `z80` crate's `Z80_io` trait takes `&self`. Expect `bus.borrow_mut()` throughout `core.rs`.

**Platform detection**: ROM extension → `.gg` = Game Gear, `.sg` = SG-1000, `.sc` = SC-3000, else Master System. The `Platform` enum propagates through `Emulator`, `Bus`, `Vdp`, `Joypad`, `AudioMixer`.

**Frame loop** (`core.rs::step_frame`): 262 lines × 228 cycles = 59,736 cycles/frame (NTSC). VDP renders lines 0–191; line interrupts fire per-scanline; VBlank at line 192. Audio samples are generated interleaved with CPU execution.

**Render loop** (`app.rs::render`): time-debt accumulator drives emulation at 60 Hz. Each frame: step emulator → blit framebuffer → `renderer.draw` (OpenGL) → `egui_state.run_frame` (dialogs on top) → `surface.swap_buffers`.

**GL shutdown order** (`app.rs::shutdown_gl`): must free `Renderer` → `EguiState` (Painter) → `GlState` in that order while the context is current, or SIGSEGV on exit.

**File dialog on Linux/Wayland**: `rfd::AsyncFileDialog` spawned via `glib::MainContext::default().spawn_local()`; the glib context is pumped each frame in `about_to_wait()`. GTK is single-threaded — never call rfd from a background thread.

**FM/PSG balance**: YM2413 per-channel output tops at ~±0.063 after /32768 normalisation; PSG MAX_VOLUME = 0.25. Mixer applies `FM_GAIN = 4.0` before summing.

**Light Phaser**: mouse position maps to emulated screen coords. When the rendered pixel exceeds brightness threshold 750 (R+G+B sum), H/V counters are latched and TH pin pulled low.

**Assets**: `assets/icon.png` embedded at compile time via `include_bytes!` in `app.rs`.

**egui menu bar (Linux only)**: rendered by `egui_ui.rs::draw_linux_menu`; height stored in `DialogState::menu_bar_height` and converted to physical pixels to offset the OpenGL letterbox rect.
