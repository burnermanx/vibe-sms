fn main() {
    println!("cargo:rerun-if-changed=src/audio/emu2413/emu2413.c");
    println!("cargo:rerun-if-changed=src/audio/emu2413/emu2413.h");

    cc::Build::new()
        .file("src/audio/emu2413/emu2413.c")
        .compile("emu2413");
}
