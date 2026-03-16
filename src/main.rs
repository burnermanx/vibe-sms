mod audio;
mod bus;
pub mod core;
mod eeprom;
pub mod frontend;
mod joypad;
mod mmu;
mod vdp;

use frontend::launch_frontend;

fn main() {
    println!("Iniciando vibe-sms emulator...");
    
    // Obter o caminho da ROM via argumentos da linha de comando
    let args: Vec<String> = std::env::args().collect();
    let rom_path = if args.len() >= 2 {
        Some(args[1].clone())
    } else {
        None
    };
    
    // Opcionalmente, pode-se iniciar o loop do emulador no frontend 
    // ou passar o rom_path para que o frontend o carregue inicialmente
    launch_frontend(rom_path);
}
