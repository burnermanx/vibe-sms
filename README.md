# vibe-sms

**vibe-sms** is a Sega 8-bit emulator written in Rust, supporting the **Master System**, **Game Gear**, **SG-1000** and **SC-3000** with hardware-accurate video, audio, input and save states. Runs natively on Linux, Windows and macOS.

> Created using **Google Antigravity** powered by **Gemini**.

---

## Features

### Platforms
| System | Extensions | Notes |
|--------|-----------|-------|
| Sega Master System | `.sms` | Full support, FM sound |
| Sega Game Gear | `.gg` | 160×144 viewport, 12-bit CRAM, stereo PSG |
| SG-1000 | `.sg` | Flat ROM, no mapper |
| SC-3000 | `.sc` | Home computer variant of SG-1000 |

Platform is auto-detected from the ROM file extension.

### Video — VDP (TMS9918A / 315-5246)
- **Mode 4** (SMS/GG): background tiles, scrolling, sprites with per-line priority and flicker
- **TMS9918A modes** (SG-1000/SC-3000): Mode 0 (Text), Mode 1 (Graphics I), Mode 2 (Graphics II), Mode 3 (Multicolor)
- Accurate line interrupts and VBlank (NMI/INT) generation
- H/V counter tracking for light gun detection
- Hardware-accurate sprite overflow and collision flags

### Audio
- **PSG (SN76489)** — integer decrementing counters, LFSR noise (rising-edge clocked), PCM via register 0/1
- **FM Synthesizer (YM2413 / OPLL)** — 9 melodic channels + 5 rhythm channels, based on the emu2413 reference
- **Game Gear stereo** — I/O port `$06` routes each PSG channel to L/R independently
- **FM auto-detection** — ports `$F0–$F2` exposed to Z80; games detect FM capability automatically
- Output via `cpal` at native device sample rate (44100 / 48000 Hz)

### Input
- **Keyboard** — fully remappable per-player bindings via the Controls dialog
- **Gamepad** — native support via `gilrs` (D-Pad, face buttons, Start)
- **Light Phaser** — mouse cursor maps to screen coords; left click fires; pixel brightness threshold triggers latch
- Two-player support

### Save System
- **Save states** — 9 slots, `F7` save / `F5` load, slot selector `1–9`; HUD overlay on screen
- **Battery saves (SRAM)** — auto-saved every ~5 seconds when dirty; `.sav` file beside ROM
- **EEPROM** — supported for compatible cartridges; `.eep` file beside ROM

### GUI
- Native OS menus via **muda** (macOS menu bar, Windows Win32 menu)
- egui menu bar fallback on Linux (Wayland-compatible)
- Letterboxed display with correct aspect ratio at any window size
- In-window dialogs: key bindings config, FM notice, about

---

## Installation

### Pre-built binaries

Download the latest release from the [Releases](../../releases) page:

| Platform | File |
|----------|------|
| Linux x86_64 | `vibe-sms-linux-x86_64` + `vibe-sms-linux-x86_64.AppImage` |
| Windows x86_64 | `vibe-sms-windows-x86_64.zip` |
| macOS Intel | `vibe-sms-macos-x86_64.zip` |
| macOS Apple Silicon | `vibe-sms-macos-arm64.zip` |

### Build from source

**Linux (Ubuntu/Debian)**
```bash
sudo apt install build-essential cargo rustc \
  libasound2-dev \
  libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev \
  libxkbcommon-dev libgl1-mesa-dev \
  libgtk-3-dev libxdo-dev libwayland-dev \
  libudev-dev pkg-config
```

**Arch Linux**
```bash
sudo pacman -S base-devel rustup
```

**Windows / macOS** — no extra dependencies.

```bash
git clone https://github.com/burnermanx/vibe-sms
cd vibe-sms
cargo build --release
```

---

## Usage

```bash
# Launch with file picker
cargo run --release

# Load a ROM directly
cargo run --release -- path/to/game.sms
cargo run --release -- path/to/game.gg
```

### Menu

| Menu | Item | Action |
|------|------|--------|
| Emulator | Open ROM… | Open file picker |
| Emulator | Reset | Soft reset (reloads ROM) |
| Emulator | Stop | Stop emulation |
| Emulator | Quit | Exit |
| State | Save State `F7` | Save to current slot |
| State | Load State `F5` | Load from current slot |
| State | Slot `1–9` | Select save slot |
| Configuration | Controls… | Remap keys |
| Configuration | FM Sound | Toggle FM (requires reset) |

### Default key bindings

| Action | Player 1 | Player 2 |
|--------|----------|----------|
| Up / Down / Left / Right | Arrow keys | W / S / A / D |
| Button 1 | `Z` | `1` |
| Button 2 | `X` | `2` |
| Start / Pause | `Enter` | `3` |
| Save state | `F7` | — |
| Load state | `F5` | — |
| Select slot | `1`–`9` | — |

All bindings are remappable via **Configuration → Controls**.

---

## Architecture

```
src/
├── main.rs              Entry point — parses CLI args, calls launch_frontend()
├── core.rs              Emulator struct; step_frame (262 lines × 228 cycles)
├── bus.rs               Bus + System; Z80_io impl; port I/O dispatch
├── mmu.rs               Sega mapper (ROM paging, SRAM, EEPROM); SG-1000 flat ROM
├── vdp.rs               TMS9918A / 315-5246; Mode 4 + TMS modes; sprites
├── joypad.rs            Input ports; light gun TH pin; GG Start
├── eeprom.rs            Microwire EEPROM (93C46 / 93C66)
├── savestate.rs         Binary serialisation of full machine state
├── platform.rs          Platform enum (MasterSystem, GameGear, Sg1000, Sc3000)
└── audio/
│   ├── mixer.rs         PSG + FM summing
│   ├── psg.rs           SN76489 (tone × 3, noise × 1, GG stereo)
│   ├── fm.rs            YM2413 wrapper + user_disabled flag
│   └── ym2413.rs        OPLL FM: 9 melodic + 5 rhythm channels
└── frontend/
    ├── mod.rs           launch_frontend(); GTK init; cpal audio stream
    ├── app.rs           VibeApp: ApplicationHandler<MenuAction>; render loop
    ├── renderer.rs      glow/OpenGL quad shader; letterbox blit
    ├── egui_ui.rs       EguiState; in-window dialogs; Linux menu bar
    ├── menu.rs          MenuAction enum; muda native menus
    └── input.rs         PlayerKeys (winit::KeyCode); KeyConfig; PadState
```

The emulation core is fully decoupled from the frontend and communicates only through `Emulator`'s public API.

---

## CI / CD

Every push to `main` builds, tests and packages on all platforms:

| Platform | Target | Artifact |
|----------|--------|---------|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | binary + AppImage |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `.zip` |
| macOS Intel | `x86_64-apple-darwin` | `.app` bundle in `.zip` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.app` bundle in `.zip` |

Release binaries are attached automatically when a tag is pushed.

---

## License

_TODO: add license_
