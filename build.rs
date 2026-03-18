#[cfg(windows)]
extern crate winres;

fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().unwrap();

        // Increase the main thread stack to 32 MB (default 1 MB overflows the emulator).
        let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        if target_env == "msvc" {
            println!("cargo:rustc-link-arg=/STACK:33554432");
        } else {
            // mingw / gnu
            println!("cargo:rustc-link-arg=-Wl,--stack,33554432");
        }
    }

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "macos" {
        // Increase the main thread stack to 32 MB (default 8 MB can overflow deep emulation).
        println!("cargo:rustc-link-arg=-Wl,-stack_size,0x2000000");
    }
}
