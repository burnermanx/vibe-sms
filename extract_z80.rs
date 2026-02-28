fn main() {
    let glob_path = format!("{}/.cargo/registry/src/index.crates.io-*/z80-1.0.2/src/*", std::env::var("HOME").unwrap());
    let mut command = std::process::Command::new("sh");
    command.arg("-c");
    command.arg(format!("ls -la {}", glob_path));
    
    let output = command.output().unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
