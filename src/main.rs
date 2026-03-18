#![cfg_attr(windows, windows_subsystem = "windows")]

mod audio;
mod bus;
pub mod platform;
pub mod core;
mod eeprom;
pub mod frontend;
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

/// Allocates a console window and redirects stdout/stderr to it.
/// Only compiled on Windows; called only when --debug / -d is passed.
#[cfg(windows)]
fn enable_debug_console() {
    extern "system" {
        fn AllocConsole() -> i32;
    }
    unsafe {
        AllocConsole();
        // Redirect the C runtime's stdout/stderr handles to the new console.
        libc::freopen(
            b"CONOUT$\0".as_ptr().cast(),
            b"w\0".as_ptr().cast(),
            libc::stdout(),
        );
        libc::freopen(
            b"CONOUT$\0".as_ptr().cast(),
            b"w\0".as_ptr().cast(),
            libc::stderr(),
        );
    }
}
