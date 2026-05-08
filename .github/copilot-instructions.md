# Copilot Instructions for `vibe-sms`

## Build, test, and lint commands

Use these from the repository root:

```bash
cargo check
cargo build --release
cargo clippy
cargo test
```

Run a single test by exact name:

```bash
cargo test core::tests::step_frame_produces_stereo_audio_samples -- --exact
```

Run tests matching a module/file area:

```bash
cargo test bus::tests
```

Run the app (release is expected for realtime emulation):

```bash
cargo run --release
cargo run --release -- path/to/game.sms
```

## High-level architecture

`vibe-sms` is split into a hardware emulation core and a desktop frontend.

- **Core (`src/core.rs`, `src/bus.rs`, `src/mmu.rs`, `src/vdp.rs`, `src/joypad.rs`, `src/audio/*`)**  
  `Emulator` owns `Z80<System>`, where `System` wraps `Bus`. `Bus` coordinates memory mapper, VDP, joypad/light gun, and audio mixer (PSG + YM2413 FM). `step_frame()` advances CPU/VDP timing and interleaves audio generation with CPU execution.
- **Frontend (`src/frontend/*`)**  
  `VibeApp` runs a `winit` event loop, drives emulation at 60 Hz via a time-debt accumulator, uploads framebuffers through OpenGL, and overlays dialogs/UI through `egui`.
- **Platform flow**  
  ROM extension determines platform (`.sms`, `.gg`, `.sg`, `.sc`) in `frontend/app.rs::load_rom`, and that `Platform` value is propagated through core subsystems (MMU/VDP/Joypad/Audio).
- **Per-frame execution path**  
  Frontend `render()` applies inputs/gamepad state, calls `Emulator::step_frame()`, appends interleaved stereo samples to the CPAL buffer, then converts emulator framebuffer output to SMS/GG viewport geometry before drawing with `renderer.draw(...)` and swapping buffers.
- **I/O ownership and dispatch**  
  Z80 memory/port access goes through `System` (`z80::Z80_io` impl in `bus.rs`): MMU for memory space, and port ranges for VDP (`0x80..=0xBF`), PSG (`0x40..=0x7F` writes), joypad mirrors (`0xC0..=0xFF`), Game Gear start (`0x00` on GG), and FM (`0xF0..=0xF2`).

## Key conventions in this repository

- **Interior mutability in core is intentional**: `System` stores `RefCell<Bus>` because the `z80` crate I/O trait uses `&self`; expect `borrow_mut()` in timing, I/O, and state paths.
- **Timing model is scanline-driven**: one frame is `262` lines × `228` cycles; VDP/render/audio work is synchronized to that cadence (`core.rs::step_frame`).
- **GL teardown order matters**: destroy `Renderer` first, then `EguiState`, then drop `GlState` (`frontend/app.rs::shutdown_gl`), or shutdown can crash.
- **Linux file dialogs must stay on GTK/glib main context**: `AsyncFileDialog` is spawned on `glib::MainContext`, and that context is pumped in `about_to_wait()`.
- **Release runtime is the default expectation**: debug builds are too slow for realtime emulation; prefer `cargo run --release` for manual runs.
