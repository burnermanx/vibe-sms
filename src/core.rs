use crate::bus::{Bus, System};
use z80::Z80;

pub struct Emulator {
    pub cpu: Z80<System>,
    pub frame_cycles: u32,
    pub vcounter: u16,
    pub cycles_accumulator: i32,
    pub line_interrupt_counter: u8,
}

impl Emulator {
    pub fn new(rom_data: Vec<u8>) -> Self {
        let bus = Bus::new(rom_data);
        let system = System::new(bus);
        let mut cpu = Z80::new(system);
        cpu.init();
        
        Self {
            cpu,
            frame_cycles: 0,
            vcounter: 0,
            cycles_accumulator: 0,
            line_interrupt_counter: 0,
        }
    }

    pub fn step_frame(&mut self) -> (bool, Vec<f32>) {
        let cycles_per_line = 228;
        let lines_per_frame = 262;
        let total_frame_cycles = cycles_per_line * lines_per_frame;
        let mut frame_cycles = 0;
        
        let mut audio_buffer = Vec::new();
        // At 60Hz and 44100Hz audio, there are 735 samples per frame
        let samples_per_frame = 735;
        let cycles_per_sample = total_frame_cycles / samples_per_frame;
        let mut sample_cycles_accumulator = 0;
        
        // Retorna true se um frame (vblank) for emitido
        let mut frame_ready = false;

        while frame_cycles < total_frame_cycles {
            let mut cycles_run = self.cpu.step();
            if cycles_run == 0 {
                cycles_run = 4; // NOP (Halt state)
            }
            
            frame_cycles += cycles_run as u32;
            self.cycles_accumulator += cycles_run as i32;
            
            sample_cycles_accumulator += cycles_run as u32;
            while sample_cycles_accumulator >= cycles_per_sample {
                sample_cycles_accumulator -= cycles_per_sample;
                let sample = self.cpu.io.bus.borrow_mut().mixer.generate_sample();
                // O cpal espera interleaved stereo ou mono. Aqui usaremos mono por agora
                audio_buffer.push(sample);
            }
            
            if self.cycles_accumulator >= cycles_per_line as i32 {
                self.cycles_accumulator -= cycles_per_line as i32;
                
                let vdp_reg_0 = self.cpu.io.bus.borrow().vdp.registers[0];
                let vdp_reg_1 = self.cpu.io.bus.borrow().vdp.registers[1];
                let vdp_reg_10 = self.cpu.io.bus.borrow().vdp.registers[10];
                
                if self.vcounter <= 192 {
                    if self.line_interrupt_counter == 0 {
                        self.line_interrupt_counter = vdp_reg_10; 
                        self.cpu.io.bus.borrow_mut().vdp.line_interrupt_flag = true;
                    } else {
                        self.line_interrupt_counter -= 1;
                    }
                } else {
                    self.line_interrupt_counter = vdp_reg_10; 
                }
                
                self.vcounter += 1;
                if (self.vcounter as i32) >= (lines_per_frame as i32) {
                    self.vcounter = 0;
                }
                
                let hw_vcounter = if self.vcounter <= 218 {
                    self.vcounter
                } else {
                    self.vcounter - 6
                };
                self.cpu.io.bus.borrow_mut().vdp.v_counter = hw_vcounter as u8;
                self.cpu.io.bus.borrow_mut().vdp.h_counter = 0x80; 
                
                if self.vcounter < 192 {
                    self.cpu.io.bus.borrow_mut().vdp.render_scanline(self.vcounter as usize);
                }
                
                if self.vcounter == 192 {
                    self.cpu.io.bus.borrow_mut().vdp.vblank_flag = true;
                    frame_ready = true;
                }
            } // Fim do if cycles_accumulator
            
            // Re-avalia as interrupções do VDP a cada instrução do CPU
            // Isso evita que o Z80 reentre na rotina de interrupção se o VDP já teve a flag limpa!
            let trigger_irq = {
                let vdp_core = &self.cpu.io.bus.borrow().vdp;
                let vblank_irq_enabled = (vdp_core.registers[1] & 0x20) != 0;
                let line_irq_enabled = (vdp_core.registers[0] & 0x10) != 0;
                
                (vdp_core.vblank_flag && vblank_irq_enabled) || (vdp_core.line_interrupt_flag && line_irq_enabled)
            };
            
            if trigger_irq {
                self.cpu.assert_irq(0xFF); 
            } else {
                self.cpu.clr_irq();
            }
        }
        (frame_ready, audio_buffer)
    }

    pub fn get_framebuffer(&self) -> [u32; 256 * 192] {
        self.cpu.io.bus.borrow().vdp.frame_buffer.clone()
    }

    // Proxy commands to joypad
    pub fn set_input(&mut self, up: bool, down: bool, left: bool, right: bool, b1: bool, b2: bool) {
        let mut bus = self.cpu.io.bus.borrow_mut();
        bus.joypad.p1_up = up;
        bus.joypad.p1_down = down;
        bus.joypad.p1_left = left;
        bus.joypad.p1_right = right;
        bus.joypad.p1_b1 = b1;
        bus.joypad.p1_b2 = b2;
    }
}
