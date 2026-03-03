# vibe-sms

**vibe-sms** is a highly accurate and portable emulator for the **Sega Master System** and **Sega Game Gear**, built entirely from scratch in **Rust**. It features a modern, native graphical user interface for Linux developed using **GTK4** and **Libadwaita**.

This project was initiated from scratch using **Google Antigravity** powered by **Gemini**.

---

## Features

### 🎮 Platform Support
- **Sega Master System** (Mark III / SMS1 / SMS2) — full support
- **Sega Game Gear** — full support, including hardware-accurate color palette (CRAM), cropped 160×144 viewport, and Start button mapped to NMI

ROMs are auto-detected by file extension (`.sms`, `.sg` → Master System; `.gg` → Game Gear).

### 🖥️ Video (VDP — TMS9918 / 315-5246)
- Background tile rendering with scroll registers
- Sprite rendering with per-line priority and flicker
- Accurate Line interrupts and VBlank (INT) generation
- Game Gear CRAM: 12-bit color (4096 colors) with hardware latch, cropped to the 160×144 GG display window

### 🔊 Audio
- **PSG (SN76489)** — all 3 tone channels + noise channel, with mathematically accurate phase accumulators clocked from the Z80 master clock. Noise LFSR matches the SMS tap bits for both white and periodic noise.
- **FM Synthesizer (YM2413 / OPLL)** — hardware FM synthesis for Japanese Master System units with full 9-channel (melodic) + 5-channel (rhythm) support
- **Game Gear Stereo Panning** — I/O port `$06` stereo control register, routing each of the 4 PSG channels independently to Left and Right outputs
- Audio output via **`cpal`** with interleaved stereo (`[L, R, L, R, ...]`) using the actual device sample rate (44100Hz or 48000Hz) for accurate pitch

### ⚡ Rendering & VSync
- **VSync-locked rendering** via GTK4 `FrameClock` (`add_tick_callback`), synchronized with the Wayland/X11 compositor — zero screen tearing
- **Time-debt accumulator**: emulation is paced to the SMS native ~59.922Hz independently of monitor refresh rate (60Hz, 75Hz, 144Hz, etc.), so games always run at the correct speed

### 🕹️ Input
- **Keyboard**: Arrow Keys / WASD for movement, `Z` and `X` for buttons 1 and 2, `Enter` for Game Gear Start
- **Gamepad / Joystick**: native Linux support via `gilrs` — DPad, South/West for button 1, East/North for button 2, Start button
- **Light Gun (Phaser)**: mouse click triggers the light gun, with coordinates scaled to the emulated screen space

---

## Requirements

The emulator targets Linux and has been tested on **Arch Linux**. It requires GTK4 development libraries.

### Arch Linux
```bash
sudo pacman -S base-devel rustup gtk4 libadwaita
```

### Ubuntu / Debian
```bash
sudo apt install build-essential cargo rustc libgtk-4-dev libadwaita-1-dev
```

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
2. Click **"Open ROM"** in the header bar
3. Select a `.sms`, `.sg`, or `.gg` ROM file — the platform is detected automatically
4. The game boots immediately
5. Controls:

| Action | Keyboard | Gamepad |
|---|---|---|
| Move | Arrow Keys | D-Pad |
| Button 1 | `Z` | South / West (A/X) |
| Button 2 | `X` | East / North (B/Y) |
| Start (GG) | `Enter` | Start |
| Light Gun | Left Mouse Click | — |

---

## Architecture

```
vibe-sms/
├── src/
│   ├── main.rs         # Entry point
│   ├── core.rs         # Emulator loop (Z80 step, VDP, audio sync)
│   ├── bus.rs          # Z80 I/O bus: routes memory, VDP, audio, joypad
│   ├── mmu.rs          # Memory mapper (ROM paging, RAM)
│   ├── vdp.rs          # Video Display Processor
│   ├── joypad.rs       # Joypad + light gun state
│   ├── audio/
│   │   ├── psg.rs      # SN76489 PSG (tone + noise + GG stereo panning)
│   │   ├── ym2413.rs   # YM2413 FM synthesizer (OPLL)
│   │   └── mixer.rs    # Combines PSG + FM into stereo output
│   └── frontend/
│       └── mod.rs      # GTK4 UI, cpal audio stream, input handling
```

The backend (`audio/`, `mmu.rs`, `vdp.rs`, `core.rs`) is fully decoupled from the GTK frontend, making future ports to other platforms straightforward.
