#![cfg_attr(windows, windows_subsystem = "windows")]

mod audio;
mod bus;
mod platform;
mod core;
mod eeprom;
mod frontend;
mod savestate;
mod joypad;
mod mmu;
mod vdp;

use frontend::launch_frontend;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rom_path = args.iter().skip(1).find(|a| !a.starts_with('-')).cloned();

    #[cfg(windows)]
    {
        let debug = args.iter().any(|a| a == "--debug" || a == "-d");
        if debug {
            enable_debug_console();
        }
    }

    launch_frontend(rom_path);
}

/// Allocates a console window for debug output.
/// Only compiled on Windows; called only when --debug / -d is passed.
/// AllocConsole sets the Win32 standard handles automatically; Rust's
/// println! uses GetStdHandle under the hood, so no freopen is needed.
#[cfg(windows)]
fn enable_debug_console() {
    extern "system" {
        fn AllocConsole() -> i32;
    }
    unsafe { AllocConsole(); }
}
