const MAX_VOLUME: f32 = 0.25; // Adjusted mix volume to compete with FM chip

const PSG_VOLUME_TABLE: [f32; 16] = [
    1.000000, 0.794328, 0.630957, 0.501187,
    0.398107, 0.316228, 0.251189, 0.199526,
    0.158489, 0.125893, 0.100000, 0.079433,
    0.063096, 0.050119, 0.039811, 0.000000, // 15 is off
];
pub struct Psg {
    // 4 canais: 3 Tone, 1 Noise
    pub registers: [u16; 8], 
    pub phases: [f32; 4],
    latch: u8,
    // Noise shift register (LFSR)
    noise_lfsr: u16,
    
    // Stereo Panning (Game Gear only - Port 0x06)
    pub stereo: u8,
    pub is_gg: bool,
    pub sample_rate: f32,
}

impl Default for Psg {
    fn default() -> Self {
        Self::new(false, 44100.0)
    }
}

impl Psg {
    pub fn new(is_gg: bool, sample_rate: f32) -> Self {
        Self {
            registers: [
                0, 0x0F,
                0, 0x0F,
                0, 0x0F,
                0, 0x0F,
            ],
            phases: [0.0; 4],
            latch: 0,
            noise_lfsr: 0x8000,
            stereo: 0xFF,
            is_gg,
            sample_rate,
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

    pub fn write_stereo(&mut self, value: u8) {
        if self.is_gg {
            self.stereo = value;
        }
    }

    pub fn generate_sample(&mut self) -> (f32, f32) {
        let mut mixed_l = 0.0;
        let mut mixed_r = 0.0;
        let master_clock = 3579545.0;
        let sample_rate = self.sample_rate;
        
        // Tone channels 0 to 2
        for i in 0..3 {
            let mut reg_val = self.registers[i * 2] as f32;
            if reg_val == 0.0 {
                reg_val = 1024.0;
            }
            
            // Freq = Clock / (32 * Register)
            let freq = master_clock / (32.0 * reg_val);
            let phase_step = freq / sample_rate;
            
            self.phases[i] += phase_step;
            if self.phases[i] >= 1.0 {
                self.phases[i] -= 1.0;
            }
            
            let output = if self.phases[i] < 0.5 { 1.0 } else { -1.0 };
            let volume = PSG_VOLUME_TABLE[(self.registers[i * 2 + 1] & 0x0F) as usize];
            
            // Apply Stereo Panning for Game Gear; always true for SMS (panned centre)
            let pan_r = if self.is_gg { (self.stereo & (1 << i)) != 0 } else { true };
            let pan_l = if self.is_gg { (self.stereo & (1 << (i + 4))) != 0 } else { true };
            // Add to mix if freq is above a cutoff to avoid DC offset hum on low limits
            if freq > 10.0 {
                if pan_l { mixed_l += output * volume; }
                if pan_r { mixed_r += output * volume; }
            }
        }
        
        // Noise channel 3
        let noise_ctrl = self.registers[6];
        let shift_rate = noise_ctrl & 0x03;
        
        let noise_shift_freq = match shift_rate {
            0 => master_clock / 256.0,  // (16 * 16)
            1 => master_clock / 512.0,  // (32 * 16)
            2 => master_clock / 1024.0, // (64 * 16)
            3 => {
                let mut reg_val = self.registers[4] as f32; // Tone 3 register
                if reg_val == 0.0 { reg_val = 1024.0; }
                master_clock / (16.0 * reg_val) // Shifts at Tone 3 transition rate
            },
            _ => master_clock / 256.0,
        };
        
        let noise_phase_step = noise_shift_freq / sample_rate;
        self.phases[3] += noise_phase_step;
        
        while self.phases[3] >= 1.0 {
            self.phases[3] -= 1.0;
            
            let is_white_noise = (noise_ctrl & 0x04) != 0;
            let tapped_bit = if is_white_noise {
                // SMS PSGs tap bits 0 and 3
                (self.noise_lfsr & 0x01) ^ ((self.noise_lfsr >> 3) & 0x01)
            } else {
                // Periodic noise taps bit 0 only
                self.noise_lfsr & 0x01
            };
            
            self.noise_lfsr = (self.noise_lfsr >> 1) | (tapped_bit << 15);
        }
        
        let noise_output = if (self.noise_lfsr & 0x01) == 1 { 1.0 } else { -1.0 };
        let noise_vol = PSG_VOLUME_TABLE[(self.registers[7] & 0x0F) as usize];

        // Apply Stereo Panning for Noise; always true for SMS (panned centre)
        let pan_r = if self.is_gg { (self.stereo & (1 << 3)) != 0 } else { true };
        let pan_l = if self.is_gg { (self.stereo & (1 << 7)) != 0 } else { true };
        
        if pan_l { mixed_l += noise_output * noise_vol; }
        if pan_r { mixed_r += noise_output * noise_vol; }
        
        (mixed_l * MAX_VOLUME, mixed_r * MAX_VOLUME)
    }
}
