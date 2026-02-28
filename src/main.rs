mod bus;
mod joypad;
mod mmu;
mod vdp;

use bus::{Bus, System};
use z80::Z80;
use minifb::{Window, WindowOptions, Key, Scale};

fn main() {
    println!("Iniciando vibe-sms emulator...");
    
    // Obter o caminho da ROM via argumentos da linha de comando
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Uso: {} <caminho_para_a_rom.sms>", args[0]);
        std::process::exit(1);
    }
    
    let rom_path = &args[1];
    println!("Carregando ROM: {}", rom_path);
    
    // Ler o arquivo binário da ROM
    let rom = match std::fs::read(rom_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Erro ao ler a ROM {}: {}", rom_path, e);
            std::process::exit(1);
        }
    };
    
    let bus = Bus::new(rom);
    let system = System::new(bus);
    let mut cpu = Z80::new(system);
    
    cpu.init(); // Inicializa registradores
    
    // Configurando Janela
    let mut window = Window::new(
        "vibe-sms - Sega Master System Emulator",
        256,
        192,
        WindowOptions {
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    ).unwrap();

    // 60 FPS
    window.set_target_fps(60);
    
    let mut vcounter = 0;
    let cycles_per_line = 228;
    let lines_per_frame = 262;
    let mut cycles_accumulator = 0;
    
    // Line Interrupt Counter (Reg 10 do VDP)
    let mut line_interrupt_counter: u8 = 0;

    println!("Iniciando loop principal...");
    
    while window.is_open() && !window.is_key_down(Key::Escape) {
        
        // Emulamos 1 quadro (Frame)
        let total_frame_cycles = cycles_per_line * lines_per_frame;
        let mut frame_cycles = 0;
        
        while frame_cycles < total_frame_cycles {
            let mut cycles_run = cpu.step();
            // Evitar trava do emulador quando a CPU Z80 entra em modo HALT
            if cycles_run == 0 {
                cycles_run = 4; // NOP
            }
            
            frame_cycles += cycles_run;
            cycles_accumulator += cycles_run;
            
            // Checar scanlines
            if cycles_accumulator >= cycles_per_line {
                cycles_accumulator -= cycles_per_line;
                
                // Line Interrupt Logic
                let vdp_reg_0 = cpu.io.bus.borrow().vdp.registers[0];
                let vdp_reg_1 = cpu.io.bus.borrow().vdp.registers[1];
                let vdp_reg_10 = cpu.io.bus.borrow().vdp.registers[10];
                
                if vcounter <= 192 {
                    if line_interrupt_counter == 0 {
                        line_interrupt_counter = vdp_reg_10; // Reload
                        cpu.io.bus.borrow_mut().vdp.line_interrupt_flag = true;
                    } else {
                        line_interrupt_counter -= 1;
                    }
                } else {
                    line_interrupt_counter = vdp_reg_10; // Fora do frame, reloading constante
                }
                
                vcounter += 1;
                
                if vcounter >= lines_per_frame {
                    vcounter = 0;
                }
                
                // Atualizar V-Counter no VDP com pulos do padrão NTSC
                let hw_vcounter = if vcounter <= 218 {
                    vcounter
                } else {
                    vcounter - 6
                };
                cpu.io.bus.borrow_mut().vdp.v_counter = hw_vcounter as u8;
                cpu.io.bus.borrow_mut().vdp.h_counter = 0x80; // Aprox. fixo no retorno do loop
                
                // VBlank (início da área não visível, linha 192)
                if vcounter == 192 {
                    cpu.io.bus.borrow_mut().vdp.vblank_flag = true;
                    
                    // Gravar log no arquivo
                    static mut FRAME_COUNT: u32 = 0;
                    unsafe {
                        FRAME_COUNT += 1;
                        if FRAME_COUNT <= 10 || FRAME_COUNT % 60 == 0 {
                            use std::io::Write;
                            if let Ok(mut file) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open("trace.log")
                            {
                                writeln!(file, "PC travado em: {:04X}, Reg1: {:02X}, Reg0: {:02X}, VBlank: {}, LInt: {}, HCount: {:02X}", 
                                         cpu.pc, cpu.io.bus.borrow().vdp.registers[1], 
                                         cpu.io.bus.borrow().vdp.registers[0],
                                         cpu.io.bus.borrow().vdp.vblank_flag,
                                         cpu.io.bus.borrow().vdp.line_interrupt_flag,
                                         cpu.io.bus.borrow().vdp.h_counter).unwrap();
                            }
                        }
                    }
                    
                    // Fim da área visível, renderizar tela localmente
                    cpu.io.bus.borrow_mut().vdp.render_frame();
                    
                    // Enviar para a janela
                    let buffer = cpu.io.bus.borrow().vdp.frame_buffer.clone();
                    window.update_with_buffer(&buffer, 256, 192).unwrap();
                }
                
                // Checar interrupções do VDP pendentes
                let trigger_irq = {
                    let vdp_core = &cpu.io.bus.borrow().vdp;
                    let vblank_irq_enabled = (vdp_reg_1 & 0x20) != 0;
                    let line_irq_enabled = (vdp_reg_0 & 0x10) != 0;
                    
                    (vdp_core.vblank_flag && vblank_irq_enabled) || (vdp_core.line_interrupt_flag && line_irq_enabled)
                };
                
                if trigger_irq {
                    cpu.assert_irq(0xFF);
                }
            }
        }
        
        // window update é feito no VBlank ou aqui se quiser ignorar taxa de atualização da emulação
        // porém chamando o `update_with_buffer` acima é mais preciso.
        // Como o limit_update_rate já segura a thread, isso ajuda a rodar nos ~60 FPS reais
    }
}
