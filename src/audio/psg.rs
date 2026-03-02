const MAX_VOLUME: f32 = 0.2; // Keep overall volume in check

pub struct Psg {
    // 4 canais: 3 Tone, 1 Noise
    pub registers: [u16; 8], 
    pub counters: [u16; 4],
    pub outputs: [i32; 4],
    latch: u8,
    
    // Noise shift register (LFSR)
    noise_lfsr: u16,
}

impl Default for Psg {
    fn default() -> Self {
        Self::new()
    }
}

impl Psg {
    pub fn new() -> Self {
        Self {
            registers: [
                0, 0x0F, // Ch 1 Tone/Vol (0x0F = Silence)
                0, 0x0F, // Ch 2 Tone/Vol
                0, 0x0F, // Ch 3 Tone/Vol
                0, 0x0F, // Ch 4 Noise/Vol
            ],
            counters: [0; 4],
            outputs: [1, 1, 1, 1], // Inicia em High
            latch: 0,
            noise_lfsr: 0x8000,
        }
    }

    pub fn write_data(&mut self, value: u8) {
        if value & 0x80 != 0 {
            // Latch/Data byte (1ccctdddd)
            self.latch = (value >> 4) & 0x07;
            let data = (value & 0x0F) as u16;
            
            let reg_index = self.latch as usize;
            if reg_index % 2 == 0 {
                // Register de tom (inferior 4-bits)
                self.registers[reg_index] = (self.registers[reg_index] & 0x3F0) | data;
            } else {
                // Volume
                self.registers[reg_index] = data;
            }
        } else {
            // Data byte (0_dddddd)
            let data = (value & 0x3F) as u16;
            let reg_index = self.latch as usize;
            
            if reg_index % 2 == 0 {
                // Register de tom (superior 6-bits)
                self.registers[reg_index] = (data << 4) | (self.registers[reg_index] & 0x0F);
            } else {
                // Volume (apesar da documentação dizer que apenas muda se for tom, 
                // há casos em que altera volume também, porém só os ultimos 4 bits)
                self.registers[reg_index] = data & 0x0F;
            }
        }
        
        // Se escreveu no registro de controle do Noise, reseta o LFSR
        if self.latch == 6 {
            self.noise_lfsr = 0x8000;
        }
    }

    pub fn generate_sample(&mut self) -> f32 {
        let mut mixed = 0.0;
        
        // At 3.58MHz, downsampling to 44100Hz implies jumping by ~81 cycles per sample
        // Para simplificar no contexto do mixer cpal, esta função atuará por tick ou batch
        // Aqui assumimos que chamamos isso 44100 vezes por seg, e fazemos os clocks internos 
        // aproximaidamente baseados no clock master do SMS
        
        // A geração de tom inverte a saída do canal quando o contador decrementa para 0.
        for i in 0..3 {
            self.counters[i] = self.counters[i].saturating_sub(1);
            if self.counters[i] == 0 {
                self.counters[i] = self.registers[i * 2]; // Reload
                // Freq 0 equals to 0x400 behavior-wise usually
                if self.counters[i] == 0 {
                    self.counters[i] = 0x400; 
                }
                self.outputs[i] *= -1;
            }
            
            let volume = (15 - self.registers[i * 2 + 1]) as f32 / 15.0; // 0x0F é silencioso (0)
            mixed += (self.outputs[i] as f32) * volume;
        }
        
        // Canal de Noise
        self.counters[3] = self.counters[3].saturating_sub(1);
        if self.counters[3] == 0 {
            let noise_ctrl = self.registers[6];
            let shift_rate = noise_ctrl & 0x03;
            
            self.counters[3] = match shift_rate {
                0 => 0x10,
                1 => 0x20,
                2 => 0x40,
                3 => self.registers[4], // Baseado no Tone 3
                _ => 0x10,
            };
            
            // Noise flip
            self.outputs[3] *= -1;
            
            // Shift register operation (only on positive edge)
            if self.outputs[3] == 1 {
                let is_white_noise = (noise_ctrl & 0x04) != 0;
                let tapped_bit = if is_white_noise {
                    // Tap bits 0 and 3 para o SMS (SN76489)
                    (self.noise_lfsr & 0x01) ^ ((self.noise_lfsr >> 3) & 0x01)
                } else {
                    self.noise_lfsr & 0x01
                };
                
                self.noise_lfsr = (self.noise_lfsr >> 1) | (tapped_bit << 15);
            }
        }
        
        let noise_vol = (15 - self.registers[7]) as f32 / 15.0;
        let lsb = (self.noise_lfsr & 0x01) as i32;
        let noise_out = if lsb == 1 { 1.0 } else { -1.0 };
        mixed += noise_out * noise_vol;
        
        // Scale appropriately
        (mixed / 4.0) * MAX_VOLUME
    }
}
