# vibe-sms

**vibe-sms** is a highly accurate and portable emulator for the Sega Master System (and equivalent Mark III hardware), built entirely from scratch in **Rust**. It features a modern, native graphical user interface for Linux developed using **GTK4** and **Libadwaita**, ensuring seamless integration, high performance, and a sleek design.

This project was initiated from scratch using **Google Antigravity** powered by **Gemini 3.1 Pro**.

## Main Features

* **CPU & Core Components**: Native `Z80` core emulation with a custom Memory Management Unit (MMU) and accurate Master System data bus mapping.
* **Video (VDP)**: Graphics processor that renders backgrounds and sprites, generating precise Line and VBlank interrupts.
* **Continuous Audio**:
  * Emulation of the classic Western **PSG (SN76489)** sound chip using mathematically accurate continuous phase accumulators derived from the main SMS clock.
  * Base module for the **FM Synthesizer (YM2413)** built-in for Japanese Master Systems and Mark III.
  * Native audio backend written over `cpal` to guarantee extremely low latency synchronization.
* **Input**: Full simultaneous support for:
  * Keyboard (Numpad / Arrow keys) with action buttons 1 and 2 mapped to the Z and X keys.
  * Gamepads / Joysticks natively supported on Linux through `gilrs`.
* **Modern GUI (GTK)**: An isolated `frontend/` architecture built on Libadwaita standards, featuring native `FileChooserNative` window dialogs for a modern desktop experience.

## Requirements and Dependencies on Linux

The code integrates with the native GTK4 graphical library, which requires specific components to be present on your operating system. The emulator has been actively tested and validated on **Arch Linux**.

To install the build dependencies on Arch-based systems:

```bash
sudo pacman -S base-devel rustup gtk4 libadwaita
```

*(Make sure your C build toolchain and Cargo are correctly installed, typically using a version manager like `rustup`).*

For Ubuntu/Debian-based distributions, the equivalent command would be:

```bash
sudo apt install build-essential cargo rustc libgtk-4-dev libadwaita-1-dev
```

## Building and Running

Clone the repository to your local machine, and then compile it directly using Cargo, the Rust package manager:

```bash
# Build and Run in debug/development mode
cargo run

# Build for Release (highly recommended for maximum gameplay performance)
cargo build --release
./target/release/vibe-sms
```

Alternatively, you can just execute `cargo run --release`.

## Usage (Playing Games)

1. After compiling, the emulator will launch with its modern GTK/Libadwaita base window.
2. Don't worry if the screen starts blank or black; it is simply awaiting commands.
3. Click the **"Open ROM"** button situated in the Header Bar.
4. Select standard `.sms` format files (direct ROM dumps of Sega Master System cartridges).
5. The game will boot immediately once loaded!
6. Use the **Arrow Keys** on your Keyboard or your **Gamepad's DPad** to move around. On modern controllers, the standard A/B buttons (or Cross/Circle on PlayStation controllers) will work seamlessly to simulate the original Sega *1* and *2* action buttons.

## Development Notes & Architecture

The project was refactored by decoupling the window/interface logic (`minifb`/`gtk`) to allow for a highly portable structure. In the future, Windows or macOS builds can consume the backend modules (`audio/`, `cpu/`, `mmu/`, and `vdp/`) without the overhead of the Linux GTK renderer.

Audio synchronization logic via `cpal` is dialed into a conservative buffer threshold (~20-90ms) to guarantee zero "clicks" or buffer underruns, maintaining smooth playback even during Wayland/X11 window compositor fluctuations.
