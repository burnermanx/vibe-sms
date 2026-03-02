pub struct Fm {
    pub registers: [u8; 0x40],
    address_latch: u8,
    // Emulação completa do YM2413 (OPLL) é bastante complexa com 9 canais
    // e moduladores de fase/envelope. 
    // Para simplificar num primeiro passo, manteremos os registradores
    // e geraremos uma aproximação de onda seno ou silêncio, para garantir
    // a correta leitura/escrita pelo bus.
    
    // Contadores simplificados
    counters: [u32; 9],
    frequencies: [f32; 9],
    volumes: [f32; 9],
    key_on: [bool; 9],
    instr: [u8; 9],
}

impl Default for Fm {
    fn default() -> Self {
        Self::new()
    }
}

impl Fm {
    pub fn new() -> Self {
        Self {
            registers: [0; 0x40],
            address_latch: 0,
            counters: [0; 9],
            frequencies: [0.0; 9],
            volumes: [0.0; 9],
            key_on: [false; 9],
            instr: [0; 9],
        }
    }

    pub fn write_data(&mut self, port: u8, value: u8) {
        match port {
            0xF0 => self.address_latch = value & 0x3F,
            0xF1 | 0xF2 => {
                let addr = self.address_latch as usize;
                self.registers[addr] = value;
                
                // Decode registers
                if addr >= 0x10 && addr <= 0x18 {
                    // F-Number LSB
                    self.update_channel(addr - 0x10);
                } else if addr >= 0x20 && addr <= 0x28 {
                    // Block / F-Number MSB / Key-On / Sustain
                    self.update_channel(addr - 0x20);
                } else if addr >= 0x30 && addr <= 0x38 {
                    // Instrument / Volume
                    let ch = addr - 0x30;
                    self.instr[ch] = (value >> 4) & 0x0F;
                    let vol = (value & 0x0F) as f32;
                    self.volumes[ch] = (15.0 - vol) / 15.0; // 0 is loudest, 15 is silent
                }
            },
            _ => {}
        }
    }

    fn update_channel(&mut self, ch: usize) {
        let f_num_lsb = self.registers[0x10 + ch] as u16;
        let reg_20 = self.registers[0x20 + ch];
        
        let f_num_msb = (reg_20 & 0x01) as u16;
        let block = (reg_20 >> 1) & 0x07;
        self.key_on[ch] = (reg_20 & 0x10) != 0;
        
        let f_num = (f_num_msb << 8) | f_num_lsb;
        
        // Freq aproximada baseada no clock do OPLL (3.58MHz / 72)
        // Freq = (49716 * f_num) / (2^19 / 2^block)
        let f = (49716.0 * f_num as f32) / (524288.0 / (1 << block) as f32);
        self.frequencies[ch] = f;
    }

    pub fn generate_sample(&mut self) -> f32 {
        let mut mixed = 0.0;
        let master_vol = 0.2;
        let sample_rate = 44100.0;
        
        for ch in 0..9 {
            if self.key_on[ch] && self.volumes[ch] > 0.0 {
                // Muito grude wave approximation (sine)
                let phase_step = (self.frequencies[ch] * 2.0 * std::f32::consts::PI) / sample_rate;
                self.counters[ch] = self.counters[ch].wrapping_add(1);
                
                let phase = (self.counters[ch] as f32) * phase_step;
                let sample = phase.sin();
                
                // Apply very basic volume
                mixed += sample * self.volumes[ch];
            }
        }
        
        (mixed / 9.0) * master_vol
    }
}
