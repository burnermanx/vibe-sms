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
            
            frame_cycles += cycles_run;
            self.cycles_accumulator += cycles_run as i32;

            sample_cycles_accumulator += cycles_run;
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
                    if self.vcounter == my {
                        let mx = self.cpu.io.bus.borrow().joypad.mouse_x;
                        
                        let pixel = self.cpu.io.bus.borrow().vdp.frame_buffer[(my as usize) * 256 + (mx as usize)];
                        let r = (pixel >> 16) & 0xFF;
                        let g = (pixel >> 8) & 0xFF;
                        let b = pixel & 0xFF;
                        
                        // Average brightness threshold (pure white flash is 765)
                        if (r + g + b) >= 750 {
                                let phaser_h_counter = 16 + (mx >> 1);
                            
                            self.cpu.io.bus.borrow_mut().vdp.h_counter = phaser_h_counter as u8;
                            self.cpu.io.bus.borrow_mut().vdp.latch_h_v_counters();
                            self.cpu.io.bus.borrow_mut().joypad.th_pin_low = true; // Stay low until CPU reads it or Vblank!
                        }
                    } else if self.vcounter > my + 8 || self.vcounter < my {
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
        let mut fb = self.cpu.io.bus.borrow().vdp.frame_buffer;
        // Strip the internal priority encoding bit before output
        for pixel in fb.iter_mut() {
            *pixel = (*pixel & 0x00FFFFFF) | 0xFF000000;
        }
        fb
    }

    // Proxy commands to joypad
    #[allow(clippy::too_many_arguments)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;

    /// A 48 KB ROM filled with 0x00 (NOP). The Z80 spins through NOPs indefinitely.
    fn nop_rom() -> Vec<u8> { vec![0u8; 0xC000] }

    fn make_emu() -> Emulator {
        Emulator::new(nop_rom(), Platform::MasterSystem, 44100.0)
    }

    /// Runs `f` on a thread with a 32 MB stack to avoid overflow in debug builds.
    /// `step_frame` triggers deep Z80 dispatch which exceeds the default 2 MB test stack.
    fn with_large_stack<F, R>(f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    // ── step_frame audio ──────────────────────────────────────────────────────

    #[test]
    fn step_frame_produces_stereo_audio_samples() {
        with_large_stack(|| {
            let mut emu = make_emu();
            let (_, audio) = emu.step_frame();
            assert_eq!(audio.len() % 2, 0, "audio buffer must contain stereo pairs");
            assert!(
                audio.len() >= 1400 && audio.len() <= 1600,
                "expected ~1470 samples (735 pairs), got {}",
                audio.len()
            );
        });
    }

    #[test]
    fn step_frame_audio_is_silent_with_no_active_channels() {
        with_large_stack(|| {
            let mut emu = make_emu();
            let (_, audio) = emu.step_frame();
            // All PSG volumes default to 15 (silent) and FM is disabled.
            assert!(audio.iter().all(|&s| s == 0.0), "all samples should be 0 with silent channels");
        });
    }

    #[test]
    fn consecutive_frames_each_produce_audio() {
        with_large_stack(|| {
            let mut emu = make_emu();
            for _ in 0..5 {
                let (_, audio) = emu.step_frame();
                assert!(!audio.is_empty());
            }
        });
    }

    // ── vcounter / vblank ─────────────────────────────────────────────────────

    #[test]
    fn vblank_flag_set_on_frame_ready() {
        with_large_stack(|| {
            let mut emu = make_emu();
            let (frame_ready, _) = emu.step_frame();
            assert!(frame_ready, "step_frame must return frame_ready=true at vblank");
        });
    }

    #[test]
    fn vcounter_resets_to_zero_each_frame() {
        with_large_stack(|| {
            let mut emu = make_emu();
            emu.step_frame();
            assert_eq!(emu.vcounter, 0);
        });
    }

    #[test]
    fn step_frame_advances_vcounter_to_262_lines() {
        with_large_stack(|| {
            let mut emu = make_emu();
            emu.step_frame();
            assert!(emu.vcounter <= 261);
        });
    }

    // ── set_input / NMI ───────────────────────────────────────────────────────

    #[test]
    fn set_input_does_not_panic() {
        with_large_stack(|| {
            let mut emu = make_emu();
            emu.set_input(true, false, true, false, true, false, false);
            emu.set_input(false, false, false, false, false, false, false);
        });
    }

    #[test]
    fn sms_start_button_rising_edge_triggers_nmi() {
        with_large_stack(|| {
            let mut emu = make_emu();
            emu.set_input(false, false, false, false, false, false, false);
            emu.set_input(false, false, false, false, false, false, true);
            assert_eq!(emu.cpu.nmi_pending, 1);
        });
    }

    #[test]
    fn gg_start_button_does_not_trigger_nmi() {
        with_large_stack(|| {
            let mut emu = Emulator::new(nop_rom(), Platform::GameGear, 44100.0);
            emu.set_input(false, false, false, false, false, false, true);
            assert_eq!(emu.cpu.nmi_pending, 0);
        });
    }

    // ── set_lightgun ──────────────────────────────────────────────────────────

    #[test]
    fn set_lightgun_updates_bus_state() {
        with_large_stack(|| {
            let mut emu = make_emu();
            emu.set_lightgun(true, 128, 96);
            let bus = emu.cpu.io.bus.borrow();
            assert!(bus.joypad.lightgun_active);
            assert_eq!(bus.joypad.mouse_x, 128);
            assert_eq!(bus.joypad.mouse_y, 96);
        });
    }

    // ── get_framebuffer ───────────────────────────────────────────────────────

    #[test]
    fn get_framebuffer_strips_priority_encoding_bit() {
        with_large_stack(|| {
            let emu = make_emu();
            emu.cpu.io.bus.borrow_mut().vdp.frame_buffer[0] = 0x01AABBCC;
            let fb = emu.get_framebuffer();
            assert_eq!(fb[0], 0xFFAABBCC, "priority bit must be stripped and alpha forced to 0xFF");
        });
    }

    #[test]
    fn get_framebuffer_has_correct_size() {
        with_large_stack(|| {
            let emu = make_emu();
            let fb = emu.get_framebuffer();
            assert_eq!(fb.len(), 256 * 192);
        });
    }

    // ── save_state / load_state roundtrip ────────────────────────────────────

    #[test]
    fn save_load_state_roundtrip_preserves_vcounter() {
        with_large_stack(|| {
            let mut emu = make_emu();
            emu.step_frame();
            emu.vcounter = 42;
            let state = emu.save_state();
            emu.vcounter = 0;
            emu.load_state(state);
            assert_eq!(emu.vcounter, 42);
        });
    }

    #[test]
    fn save_load_state_roundtrip_preserves_vram() {
        with_large_stack(|| {
            let mut emu = make_emu();
            emu.cpu.io.bus.borrow_mut().vdp.vram[0x100] = 0xAB;
            let state = emu.save_state();
            emu.cpu.io.bus.borrow_mut().vdp.vram[0x100] = 0x00;
            emu.load_state(state);
            assert_eq!(emu.cpu.io.bus.borrow().vdp.vram[0x100], 0xAB);
        });
    }

    #[test]
    fn save_load_state_roundtrip_preserves_cpu_pc() {
        with_large_stack(|| {
            let mut emu = make_emu();
            emu.cpu.pc = 0x1234;
            let state = emu.save_state();
            emu.cpu.pc = 0;
            emu.load_state(state);
            assert_eq!(emu.cpu.pc, 0x1234);
        });
    }
}
