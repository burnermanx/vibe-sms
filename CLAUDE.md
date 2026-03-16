# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**vibe-sms** is a Sega Master System and Game Gear emulator written in Rust. It supports both platforms with hardware-accurate video (VDP), audio (PSG + FM/YM2413), input (keyboard, gamepad, light gun), and a native GUI via egui/eframe.

## Build & Run Commands

```bash
# Run in release mode (recommended — debug is too slow for real-time emulation)
cargo run --release

# Pass a ROM directly
cargo run --release -- path/to/game.sms
cargo run --release -- path/to/game.gg

# Build only
cargo build --release

# Check for errors without producing an artifact
cargo check

# Run clippy lints
cargo clippy
```

There are no tests in this project currently.

### Linux Build Dependencies

```bash
# Arch
sudo pacman -S base-devel rustup

# Ubuntu/Debian
sudo apt install build-essential cargo rustc \
  libasound2-dev libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev pkg-config
```

## Architecture

The emulation core is fully decoupled from the frontend. Data flows: `frontend/mod.rs` → `core.rs` → `bus.rs` → hardware modules.

### Core Data Flow

```
Emulator (core.rs)
  └── Z80<System> (z80 crate)
        └── System (bus.rs)
              └── Bus (bus.rs)
                    ├── Mmu (mmu.rs)       — ROM paging, SRAM
                    ├── Vdp (vdp.rs)       — TMS9918/315-5246 video
                    ├── Joypad (joypad.rs) — input + light gun
                    └── AudioMixer (audio/mixer.rs)
                          ├── Psg (audio/psg.rs)    — SN76489
                          └── Fm (audio/fm.rs)
                                └── Ym2413 (audio/ym2413.rs) — OPLL FM
```

### Key Architectural Points

- **`Bus` is wrapped in `RefCell<Bus>` inside `System`** — because the `z80` crate requires `Z80_io` trait methods to take `&self`/`&mut self`, interior mutability is used extensively. Expect `bus.borrow()` / `bus.borrow_mut()` patterns everywhere in `core.rs`.

- **Platform detection**: ROM file extension determines platform — `.gg` → Game Gear, `.sms`/`.sg` → Master System. The `is_gg` flag propagates through `Emulator`, `Bus`, `Vdp`, `Joypad`, and `AudioMixer`.

- **Frame loop** (`core.rs::step_frame`): Runs one full NTSC frame (262 lines × 228 cycles = 59,736 cycles). Per-scanline: renders VDP line 0–191, fires line interrupts, generates audio samples interleaved with CPU execution. VBlank fires at line 192.

- **Frontend** (`frontend/mod.rs`): Uses `eframe` (egui + glow/OpenGL). The `VibeApp::update()` method handles timing (time-debt accumulator for frame pacing), gamepad polling via `gilrs`, keyboard input, and blitting the 256×192 XRGB framebuffer to an egui texture. File dialogs use `rfd` (GTK3 backend on Linux).

- **Audio**: `cpal` opens the default output device and drains a shared `Arc<Mutex<Vec<f32>>>` ring buffer. Emulator pushes interleaved stereo f32 samples into the buffer each frame.

- **FM toggle**: Toggling FM sound sets `user_disabled` on `AudioMixer.fm`; the FM detection port ($F0–$F2) is exposed to the Z80 so games can auto-detect FM capability. A reset is required for the change to take effect.

- **Light Phaser**: Mouse position maps to emulated screen coords. When the pixel rendered at that position exceeds brightness threshold 750 (sum of R+G+B), the H/V counters are latched and the TH pin is pulled low.

- **Assets**: `assets/icon.png` is embedded at compile time via `include_bytes!` in `frontend/mod.rs`.
