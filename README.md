# vibe-sms

**vibe-sms** is a highly accurate and portable emulator for the **Sega Master System** and **Sega Game Gear**, built entirely from scratch in **Rust**. It runs natively on **Linux**, **Windows**, and **macOS** with a lightweight cross-platform window powered by **minifb**.

This project was initiated from scratch using **Google Antigravity** powered by **Gemini**.

---

## Features

### 🎮 Platform Support
- **Sega Master System** (Mark III / SMS1 / SMS2) — full support
- **Sega Game Gear** — full support, including hardware-accurate 12-bit CRAM, cropped 160×144 viewport, and Start button via I/O port `$00`

ROMs are auto-detected by file extension (`.sms`, `.sg` → Master System; `.gg` → Game Gear).

### 🖥️ Video (VDP — TMS9918 / 315-5246)
- Background tile rendering with scroll registers and **priority bit** (BG-over-sprite layering)
- Sprite rendering with per-line priority and flicker
- Accurate line interrupts and VBlank (INT) generation
- **H counter tracking** — approximated from Z80 cycle position within each scanline
- CRAM write updates the read buffer correctly per SMS spec

### 🔊 Audio
- **PSG (SN76489)** — complete rewrite using **integer decrementing counters** matching hardware behavior; accurate pitch, LFSR noise (rising-edge clocked), and PCM playback via register 0/1
- **FM Synthesizer (YM2413 / OPLL)** — full 9-channel melodic + 5-channel rhythm; closely matched to the emu2413 C reference (v1.5.9)
- **FM toggle** — press `M` to switch between FM and PSG-only mode (triggers emulator reset for accurate game detection)
- **Game Gear Stereo Panning** — I/O port `$06` routes each PSG channel to Left/Right independently
- Audio output via **`cpal`** using the actual device sample rate (44100 Hz or 48000 Hz)

### ⚡ Rendering & Timing
- **Software framebuffer rendering** via `minifb` — works on any GPU (or no GPU), zero driver requirements
- Nearest-neighbor scaling with **letterboxed aspect ratio** preservation
- **Time-debt accumulator** pacing to SMS native ~59.922 Hz regardless of monitor refresh rate

### 🕹️ Input
- **Keyboard**: Arrow Keys for movement, `Z` / `X` for buttons 1 and 2, `Enter` for Start/Pause
- **Gamepad / Joystick**: native support via `gilrs` — D-Pad, South/West for button 1, East/North for button 2, Start
- **SMS Pause button**: `Enter` triggers a real CPU NMI on Master System
- **Light Gun (Phaser)**: left mouse button fires, coordinates scaled to emulated screen space

---

## Platform Requirements

### Linux
```bash
# Arch Linux
sudo pacman -S base-devel rustup

# Ubuntu / Debian
sudo apt install build-essential cargo rustc \
  libasound2-dev libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev pkg-config
```

### Windows
No extra dependencies. The `.exe` is fully self-contained — no runtime DLLs needed.

### macOS
No extra dependencies. Uses the system Cocoa window via minifb.

---

## Building & Running

```bash
# Run in release mode (recommended for full performance)
cargo run --release

# Or build and run separately
cargo build --release
./target/release/vibe-sms

# Pass a ROM directly as a CLI argument
./target/release/vibe-sms path/to/game.sms
./target/release/vibe-sms path/to/game.gg
```

---

## Usage

1. Launch the emulator with `cargo run --release`
2. Press **`O`** to open a ROM via the native file dialog
3. Select a `.sms`, `.sg`, or `.gg` file — platform is detected automatically
4. The game boots immediately

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `O` | Open ROM (native file dialog) |
| `R` | Reset |
| `S` | Stop emulation |
| `M` | Toggle FM Sound on/off (SMS only) |
| `Q` | Quit |

### Game Controls

| Action | Keyboard | Gamepad |
|--------|----------|---------|
| Move | Arrow Keys | D-Pad |
| Button 1 | `Z` | South / West (A/X) |
| Button 2 | `X` | East / North (B/Y) |
| Start / Pause | `Enter` | Start |
| Light Gun | Left Mouse Click | — |

---

## Architecture

```
vibe-sms/
├── .github/
│   └── workflows/
│       └── build.yml   # CI/CD: Linux, Windows, macOS, Apple Silicon
├── src/
│   ├── main.rs         # Entry point
│   ├── core.rs         # Emulator loop (Z80 step, VDP, H counter, audio sync)
│   ├── bus.rs          # Z80 I/O bus: memory, VDP, audio, joypad (with port mirroring)
│   ├── mmu.rs          # Memory mapper (ROM paging, SRAM)
│   ├── vdp.rs          # Video Display Processor (TMS9918 / 315-5246)
│   ├── joypad.rs       # Joypad + light gun + GG Start button
│   ├── audio/
│   │   ├── psg.rs      # SN76489 PSG (integer counters, LFSR, GG stereo)
│   │   ├── ym2413.rs   # YM2413 FM synthesizer (OPLL)
│   │   ├── fm.rs       # FM chip interface + user_disabled flag
│   │   └── mixer.rs    # Combines PSG + FM into stereo output
│   └── frontend/
│       └── mod.rs      # minifb window, cpal audio, gilrs input, rfd dialogs
```

The emulation core (`audio/`, `mmu.rs`, `vdp.rs`, `core.rs`) is fully decoupled from the frontend.

---

## Continuous Integration

Every push builds on all three platforms automatically via GitHub Actions:

| Platform | Target | Artifact |
|----------|--------|---------|
| Linux | `x86_64-unknown-linux-gnu` | `vibe-sms` |
| Windows | `x86_64-pc-windows-msvc` | `vibe-sms.exe` |
| macOS Intel | `x86_64-apple-darwin` | `vibe-sms` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `vibe-sms` |

Release binaries are automatically attached to GitHub releases.
