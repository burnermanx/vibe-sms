use crate::bus::{Bus, System};
use crate::platform::Platform;
use z80::Z80;

pub struct Emulator {
    pub cpu: Z80<System>,
    pub frame_cycles: u32,
    pub vcounter: u16,
    pub cycles_accumulator: i32,
    pub line_interrupt_counter: u8,
    pub platform: Platform,
}

impl Emulator {
    pub fn new(rom_data: Vec<u8>, platform: Platform, sample_rate: f32) -> Self {
        let bus = Bus::new(rom_data, platform, sample_rate);
        let system = System::new(bus, platform);
        let mut cpu = Z80::new(system);
        cpu.init();

        Self {
            cpu,
            frame_cycles: 0,
            vcounter: 0,
            cycles_accumulator: 0,
            line_interrupt_counter: 0,
            platform,
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
                let (sample_l, sample_r) = self.cpu.io.bus.borrow_mut().mixer.generate_sample();
                // O cpal espera interleaved stereo: Left, Right
                audio_buffer.push(sample_l);
                audio_buffer.push(sample_r);
            }
            
            if self.cycles_accumulator >= cycles_per_line as i32 {
                self.cycles_accumulator -= cycles_per_line as i32;
                
                let _vdp_reg_0 = self.cpu.io.bus.borrow().vdp.registers[0];
                let _vdp_reg_1 = self.cpu.io.bus.borrow().vdp.registers[1];
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
                if self.vcounter >= 262 {
                    self.vcounter = 0;
                    self.cpu.io.bus.borrow_mut().joypad.th_pin_low = false;
                    self.cpu.io.bus.borrow_mut().vdp.h_latched = false;
                }
                
                let hw_vcounter = if self.vcounter <= 218 {
                    self.vcounter
                } else {
                    self.vcounter - 6
                };
                self.cpu.io.bus.borrow_mut().vdp.v_counter = hw_vcounter as u8;

                // Update H counter based on cycle position within the scanline.
                // The VDP H counter maps pixel positions (0-341) to counter values:
                //   Pixels 0-293: H counter = pixel / 2 (values 0x00 to 0x93)  
                //   Pixels 294-341: H counter = (pixel - 294) / 2 + 0xED - 23 (values 0xED down through jump to 0x93)
                // Since Z80 cycles × 3/2 ≈ pixel position, and we reset cycles_accumulator each line:
                let cycle_in_line = (self.cycles_accumulator.max(0) as u32).min(227);
                let pixel_pos = (cycle_in_line * 3) / 2; // 0-341 range
                let h_counter = if pixel_pos < 0xED {
                    pixel_pos as u8
                } else {
                    // Jump: after 0xED (237) the counter wraps through 0x93-0xFF hblank region
                    (pixel_pos - 0xED + 0x93) as u8
                };
                self.cpu.io.bus.borrow_mut().vdp.h_counter = h_counter;
                
                if self.vcounter < 192 {
                    self.cpu.io.bus.borrow_mut().vdp.render_scanline(self.vcounter as usize);
                    
                    // Light Phaser Detection Simulation
                    // The photodiode is ALWAYS active, independently of the trigger.
                    let my = self.cpu.io.bus.borrow().joypad.mouse_y;
                    
                    // We check the exact row we just rendered!
                    if self.vcounter as u16 == my {
                        let mx = self.cpu.io.bus.borrow().joypad.mouse_x;
                        
                        let pixel = self.cpu.io.bus.borrow().vdp.frame_buffer[(my as usize) * 256 + (mx as usize)];
                        let r = (pixel >> 16) & 0xFF;
                        let g = (pixel >> 8) & 0xFF;
                        let b = pixel & 0xFF;
                        
                        // Average brightness threshold (pure white flash is 765)
                        if (r + g + b) >= 750 {
                            if self.cpu.io.bus.borrow().joypad.lightgun_active {
                                println!("LIGHT GUN HIT DETECTED! mx: {}, my: {}, color: {}", mx, my, r+g+b);
                            }
                            
                            let phaser_h_counter = 16 + (mx >> 1);
                            
                            self.cpu.io.bus.borrow_mut().vdp.h_counter = phaser_h_counter as u8;
                            self.cpu.io.bus.borrow_mut().vdp.latch_h_v_counters();
                            self.cpu.io.bus.borrow_mut().joypad.th_pin_low = true; // Stay low until CPU reads it or Vblank!
                        }
                    } else if self.vcounter as u16 > my + 8 || self.vcounter < my {
                        // Automatically release the TH switch right after 8 scanlines (creating a realistic physical sensor pulse).
                        self.cpu.io.bus.borrow_mut().joypad.th_pin_low = false;
                    }
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
        let mut fb = self.cpu.io.bus.borrow().vdp.frame_buffer.clone();
        // Strip the internal priority encoding bit before output
        for pixel in fb.iter_mut() {
            *pixel = (*pixel & 0x00FFFFFF) | 0xFF000000;
        }
        fb
    }

    // Proxy commands to joypad
    pub fn set_input(&mut self, up: bool, down: bool, left: bool, right: bool, b1: bool, b2: bool, start: bool) {
        let mut bus = self.cpu.io.bus.borrow_mut();
        // Detect rising edge of Start/Pause button
        // SMS: Pause button triggers NMI
        // Game Gear: Start button is read from I/O port 0x00, DOES NOT trigger NMI
        let trigger_nmi = !self.platform.is_gg() && start && !bus.joypad.gg_start;
        
        bus.joypad.gg_start = start;
        bus.joypad.p1_up = up;
        bus.joypad.p1_down = down;
        bus.joypad.p1_left = left;
        bus.joypad.p1_right = right;
        bus.joypad.p1_b1 = b1;
        bus.joypad.p1_b2 = b2;
        
        drop(bus);
        
        if trigger_nmi {
            self.cpu.pulse_nmi();
        }
    }

    pub fn set_lightgun(&mut self, active: bool, x: u16, y: u16) {
        let mut bus = self.cpu.io.bus.borrow_mut();
        bus.joypad.lightgun_active = active;
        bus.joypad.mouse_x = x;
        bus.joypad.mouse_y = y;
    }

    // ── Save state ────────────────────────────────────────────────────────────

    pub fn save_state(&self) -> crate::savestate::SaveState {
        use crate::savestate::*;
        let bus = self.cpu.io.bus.borrow();

        let cpu = CpuState {
            af: self.cpu.get_af(), bc: self.cpu.get_bc(),
            de: self.cpu.get_de(), hl: self.cpu.get_hl(),
            af_alt: self.cpu.get_af_alt(), bc_alt: self.cpu.get_bc_alt(),
            de_alt: self.cpu.get_de_alt(), hl_alt: self.cpu.get_hl_alt(),
            pc: self.cpu.pc, sp: self.cpu.sp,
            ix: self.cpu.ix, iy: self.cpu.iy, mem_ptr: self.cpu.mem_ptr,
            i: self.cpu.i, r: self.cpu.r,
            iff1: self.cpu.iff1, iff2: self.cpu.iff2, halted: self.cpu.halted,
            interrupt_mode: self.cpu.interrupt_mode, iff_delay: self.cpu.iff_delay,
            irq_pending: self.cpu.irq_pending, nmi_pending: self.cpu.nmi_pending,
            irq_data: self.cpu.irq_data,
        };

        let mmu = MmuState {
            ram:        bus.mmu.ram,
            cart_ram:   bus.mmu.cart_ram,
            ram_control: bus.mmu.ram_control,
            rom_bank_0: bus.mmu.rom_bank_0,
            rom_bank_1: bus.mmu.rom_bank_1,
            rom_bank_2: bus.mmu.rom_bank_2,
        };

        let vdp = bus.vdp.get_state();
        let psg = bus.mixer.psg.get_state();

        let timing = EmuTimingState {
            vcounter: self.vcounter,
            cycles_accumulator: self.cycles_accumulator,
            line_interrupt_counter: self.line_interrupt_counter,
            frame_cycles: self.frame_cycles,
        };

        SaveState { cpu, mmu, vdp, psg, timing }
    }

    pub fn load_state(&mut self, state: crate::savestate::SaveState) {
        // CPU
        let c = &state.cpu;
        self.cpu.set_af(c.af); self.cpu.set_bc(c.bc);
        self.cpu.set_de(c.de); self.cpu.set_hl(c.hl);
        self.cpu.set_af_alt(c.af_alt); self.cpu.set_bc_alt(c.bc_alt);
        self.cpu.set_de_alt(c.de_alt); self.cpu.set_hl_alt(c.hl_alt);
        self.cpu.pc = c.pc; self.cpu.sp = c.sp;
        self.cpu.ix = c.ix; self.cpu.iy = c.iy; self.cpu.mem_ptr = c.mem_ptr;
        self.cpu.i = c.i; self.cpu.r = c.r;
        self.cpu.iff1 = c.iff1; self.cpu.iff2 = c.iff2; self.cpu.halted = c.halted;
        self.cpu.interrupt_mode = c.interrupt_mode; self.cpu.iff_delay = c.iff_delay;
        self.cpu.irq_pending = c.irq_pending; self.cpu.nmi_pending = c.nmi_pending;
        self.cpu.irq_data = c.irq_data;

        // Bus
        let mut bus = self.cpu.io.bus.borrow_mut();
        let m = &state.mmu;
        bus.mmu.ram        = m.ram;
        bus.mmu.cart_ram   = m.cart_ram;
        bus.mmu.ram_control = m.ram_control;
        bus.mmu.rom_bank_0 = m.rom_bank_0;
        bus.mmu.rom_bank_1 = m.rom_bank_1;
        bus.mmu.rom_bank_2 = m.rom_bank_2;

        bus.vdp.load_state(&state.vdp);
        bus.mixer.psg.load_state(&state.psg);

        // Timing
        let t = &state.timing;
        self.vcounter              = t.vcounter;
        self.cycles_accumulator    = t.cycles_accumulator;
        self.line_interrupt_counter = t.line_interrupt_counter;
        self.frame_cycles          = t.frame_cycles;
    }

    // ── EEPROM persistence ────────────────────────────────────────────────────

    pub fn has_eeprom(&self) -> bool {
        self.cpu.io.bus.borrow().mmu.eeprom.is_some()
    }

    pub fn is_eeprom_dirty(&self) -> bool {
        self.cpu.io.bus.borrow().mmu.eeprom.as_ref().map(|e| e.dirty).unwrap_or(false)
    }

    pub fn clear_eeprom_dirty(&self) {
        if let Some(ref mut eeprom) = self.cpu.io.bus.borrow_mut().mmu.eeprom {
            eeprom.dirty = false;
        }
    }

    pub fn get_eeprom_data(&self) -> Option<Vec<u8>> {
        self.cpu.io.bus.borrow().mmu.eeprom.as_ref().map(|e| e.data.to_vec())
    }

    pub fn load_eeprom_data(&self, data: &[u8]) {
        if let Some(ref mut eeprom) = self.cpu.io.bus.borrow_mut().mmu.eeprom {
            let len = data.len().min(eeprom.data.len());
            eeprom.data[..len].copy_from_slice(&data[..len]);
            eeprom.dirty = false;
        }
    }

    // ── SRAM persistence ──────────────────────────────────────────────────────

    pub fn is_sram_dirty(&self) -> bool {
        self.cpu.io.bus.borrow().mmu.sram_dirty
    }

    pub fn clear_sram_dirty(&self) {
        self.cpu.io.bus.borrow_mut().mmu.sram_dirty = false;
    }

    /// Returns a copy of the 16KB cart RAM.
    pub fn get_cart_ram(&self) -> Vec<u8> {
        self.cpu.io.bus.borrow().mmu.cart_ram.to_vec()
    }

    /// Overwrites cart RAM with the given data (used when loading a .sav file).
    pub fn load_cart_ram(&self, data: &[u8]) {
        let mut bus = self.cpu.io.bus.borrow_mut();
        let len = data.len().min(bus.mmu.cart_ram.len());
        bus.mmu.cart_ram[..len].copy_from_slice(&data[..len]);
        bus.mmu.sram_dirty = false;
    }
}
