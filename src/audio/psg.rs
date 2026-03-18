const MAX_VOLUME: f32 = 0.25; // Per-channel headroom: 4 channels × 0.25 = 1.0 max

const PSG_VOLUME_TABLE: [f32; 16] = [
    1.000000, 0.794328, 0.630957, 0.501187,
    0.398107, 0.316228, 0.251189, 0.199526,
    0.158489, 0.125893, 0.100000, 0.079433,
    0.063096, 0.050119, 0.039811, 0.000000, // 15 is off
];

/// SN76489 PSG emulation using integer decrementing counters.
///
/// The real chip has a master clock (3579545 Hz NTSC) divided by 16 to get
/// the internal clock. Each channel has a 10-bit counter that decrements
/// every internal clock tick. When it reaches zero, it reloads from the
/// register and the output polarity toggles.
pub struct Psg {
    // 4 pairs: [tone0, vol0, tone1, vol1, tone2, vol2, noise_ctrl, vol3]
    pub registers: [u16; 8],
    latch: u8,

    // Integer counters (one per channel, count down from register value)
    counters: [u16; 4],
    // Output polarity: +1 or -1 (stored as i8: 1 or -1)
    polarity: [i8; 4],

    // Noise shift register (LFSR) — 16 bits for SMS/GG
    noise_lfsr: u16,

    // Fractional accumulator for downsampling from PSG internal clock to output sample rate
    // PSG internal clock = master_clock / 16
    clock_frac: f64,
    clock_step: f64, // = psg_internal_clock / sample_rate (how many PSG ticks per output sample)

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
        let master_clock: f64 = 3579545.0;
        let psg_clock = master_clock / 16.0;

        Self {
            registers: [
                0, 0x0F,
                0, 0x0F,
                0, 0x0F,
                0, 0x0F,
            ],
            latch: 0,
            counters: [0; 4],
            polarity: [1; 4],
            noise_lfsr: 0x8000,
            clock_frac: 0.0,
            clock_step: psg_clock / sample_rate as f64,
            stereo: 0xFF,
            is_gg,
            sample_rate,
        }
    }

    pub fn get_state(&self) -> crate::savestate::PsgState {
        crate::savestate::PsgState {
            registers:  self.registers,
            latch:      self.latch,
            counters:   self.counters,
            polarity:   self.polarity,
            noise_lfsr: self.noise_lfsr,
            clock_frac: self.clock_frac,
            stereo:     self.stereo,
        }
    }

    pub fn load_state(&mut self, s: &crate::savestate::PsgState) {
        self.registers  = s.registers;
        self.latch      = s.latch;
        self.counters   = s.counters;
        self.polarity   = s.polarity;
        self.noise_lfsr = s.noise_lfsr;
        self.clock_frac = s.clock_frac;
        self.stereo     = s.stereo;
    }

    pub fn write_data(&mut self, value: u8) {
        if value & 0x80 != 0 {
            // Latch/Data byte (1ccctdddd)
            self.latch = (value >> 4) & 0x07;
            let data = (value & 0x0F) as u16;

            let reg_index = self.latch as usize;
            if reg_index.is_multiple_of(2) {
                // Tone register (lower 4 bits)
                self.registers[reg_index] = (self.registers[reg_index] & 0x3F0) | data;
            } else {
                // Volume register
                self.registers[reg_index] = data;
            }
        } else {
            // Data byte (0_dddddd)
            let data = (value & 0x3F) as u16;
            let reg_index = self.latch as usize;

            if reg_index.is_multiple_of(2) {
                // Tone register (upper 6 bits)
                self.registers[reg_index] = (data << 4) | (self.registers[reg_index] & 0x0F);
            } else {
                // Volume (only low 4 bits matter)
                self.registers[reg_index] = data & 0x0F;
            }
        }

        // Writing to the noise control register resets the LFSR
        if self.latch == 6 {
            self.noise_lfsr = 0x8000;
        }
    }

    pub fn write_stereo(&mut self, value: u8) {
        if self.is_gg {
            self.stereo = value;
        }
    }

    /// Clock one tick of the PSG internal clock (master_clock / 16).
    /// Each channel's counter decrements. At zero, it reloads and output toggles.
    fn tick(&mut self) {
        // --- Tone channels 0, 1, 2 ---
        for ch in 0..3 {
            if self.counters[ch] > 0 {
                self.counters[ch] -= 1;
            }
            if self.counters[ch] == 0 {
                let reg_val = self.registers[ch * 2];
                // Reload counter. If register is 0, treat as 0x400 (1024) per hardware behavior
                self.counters[ch] = if reg_val == 0 { 0x400 } else { reg_val };
                // Toggle polarity
                self.polarity[ch] = -self.polarity[ch];
            }
        }

        // --- Noise channel 3 ---
        if self.counters[3] > 0 {
            self.counters[3] -= 1;
        }
        if self.counters[3] == 0 {
            // Reload based on shift rate
            let noise_ctrl = self.registers[6];
            let shift_rate = noise_ctrl & 0x03;
            let reload = match shift_rate {
                0 => 0x10,  // 16
                1 => 0x20,  // 32
                2 => 0x40,  // 64
                3 => {
                    // Use tone channel 2's register value
                    let t2 = self.registers[4];
                    if t2 == 0 { 0x400 } else { t2 }
                },
                _ => 0x10,
            };
            self.counters[3] = reload;

            // Toggle noise polarity
            let old_polarity = self.polarity[3];
            self.polarity[3] = -self.polarity[3];

            // LFSR shifts on rising edge only (transition from -1 to +1)
            if old_polarity < 0 && self.polarity[3] > 0 {
                let is_white = (noise_ctrl & 0x04) != 0;
                let tapped = if is_white {
                    // SMS/GG: tap bits 0 and 3 ($0009)
                    (self.noise_lfsr & 0x0001) ^ ((self.noise_lfsr >> 3) & 0x0001)
                } else {
                    // Periodic: tap bit 0 only
                    self.noise_lfsr & 0x0001
                };
                self.noise_lfsr = (self.noise_lfsr >> 1) | (tapped << 15);
            }
        }
    }

    pub fn generate_sample(&mut self) -> (f32, f32) {
        // Run PSG ticks to catch up to this output sample
        self.clock_frac += self.clock_step;
        let ticks = self.clock_frac as u32;
        self.clock_frac -= ticks as f64;

        for _ in 0..ticks {
            self.tick();
        }

        // --- Mix output ---
        let mut mixed_l: f32 = 0.0;
        let mut mixed_r: f32 = 0.0;

        // Tone channels 0-2
        for ch in 0..3 {
            let reg_val = self.registers[ch * 2];
            // Register value 0 or 1: constant +1 (used for PCM sample playback)
            let output: f32 = if reg_val <= 1 {
                1.0
            } else {
                self.polarity[ch] as f32
            };
            let volume = PSG_VOLUME_TABLE[(self.registers[ch * 2 + 1] & 0x0F) as usize];

            let pan_r = if self.is_gg { (self.stereo & (1 << ch)) != 0 } else { true };
            let pan_l = if self.is_gg { (self.stereo & (1 << (ch + 4))) != 0 } else { true };

            if pan_l { mixed_l += output * volume; }
            if pan_r { mixed_r += output * volume; }
        }

        // Noise channel 3 — output is the LFSR's bit 0
        let noise_output: f32 = if (self.noise_lfsr & 0x01) == 1 { 1.0 } else { -1.0 };
        let noise_vol = PSG_VOLUME_TABLE[(self.registers[7] & 0x0F) as usize];

        let pan_r = if self.is_gg { (self.stereo & (1 << 3)) != 0 } else { true };
        let pan_l = if self.is_gg { (self.stereo & (1 << 7)) != 0 } else { true };

        if pan_l { mixed_l += noise_output * noise_vol; }
        if pan_r { mixed_r += noise_output * noise_vol; }

        (mixed_l * MAX_VOLUME, mixed_r * MAX_VOLUME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latch_byte_selects_tone0_low_nibble() {
        let mut psg = Psg::new(false, 44100.0);
        // Latch byte: bit7=1, bits6-4=000 (reg 0 = tom 0), bits3-0=0b1010
        psg.write_data(0x80 | 0x0A);
        assert_eq!(psg.registers[0] & 0x0F, 0x0A, "nibble baixo do registro 0");
    }

    #[test]
    fn data_byte_updates_upper_6_bits_of_tone() {
        let mut psg = Psg::new(false, 44100.0);
        // Primeiro faz latch no reg 0 com nibble baixo = 0x05
        psg.write_data(0x85); // latch reg0, data=5
        // Data byte: bit7=0, bits5-0 = upper 6 bits
        psg.write_data(0x0F); // upper bits = 0x0F
        // registers[0] = (0x0F << 4) | 0x05 = 0xF5
        assert_eq!(psg.registers[0], 0xF5);
    }

    #[test]
    fn latch_byte_selects_volume_register() {
        let mut psg = Psg::new(false, 44100.0);
        // Reg 1 (volume do canal 0): latch byte = 1 ccc t dddd → ccc=000, t=1 → reg índice 1
        // byte = 0x80 | (0b001 << 4) | 0x07 = 0x97
        psg.write_data(0x97);
        assert_eq!(psg.registers[1], 0x07, "volume do canal 0 deve ser 7");
    }

    #[test]
    fn volume_15_produces_silence() {
        // Volume 15 = silêncio na tabela de volumes do PSG
        let mut psg = Psg::new(false, 44100.0);
        // Todos os canais já iniciam com volume 0x0F (silêncio)
        let (l, r) = psg.generate_sample();
        assert_eq!(l, 0.0, "canal silencioso deve produzir sample 0.0");
        assert_eq!(r, 0.0);
    }

    #[test]
    fn volume_0_produces_nonzero_output() {
        // Canal 0 com tom e volume = 0 (máximo)
        let mut psg = Psg::new(false, 44100.0);
        // Configura tom 0 = 0x001 (frequência bem alta para garantir saída não-zero)
        psg.write_data(0x81); // latch reg0, data=1 → lower nibble = 1
        psg.write_data(0x00); // data byte: upper 6 bits = 0 → reg[0] = 1
        // Volume 0 = amplitude máxima: reg1 = 0
        psg.write_data(0x90); // latch reg1, data=0
        // Gera alguns samples para avançar o estado
        let mut nonzero = false;
        for _ in 0..100 {
            let (l, _r) = psg.generate_sample();
            if l != 0.0 { nonzero = true; break; }
        }
        assert!(nonzero, "canal com volume máximo deve produzir saída não-zero");
    }

    #[test]
    fn noise_control_write_resets_lfsr() {
        let mut psg = Psg::new(false, 44100.0);
        // Avança o LFSR gerando alguns samples
        psg.write_data(0x90); // volume 0 no noise
        for _ in 0..50 { psg.generate_sample(); }
        // Escreve no registro de controle do noise (reg 6)
        psg.write_data(0x80 | (0b110 << 4) | 0x00); // latch reg6
        assert_eq!(psg.noise_lfsr, 0x8000, "escrever no noise control deve resetar o LFSR");
    }

    #[test]
    fn stereo_write_only_affects_gg() {
        let mut psg_sms = Psg::new(false, 44100.0);
        psg_sms.write_stereo(0x00); // não-GG ignora
        assert_eq!(psg_sms.stereo, 0xFF, "SMS ignora write_stereo");

        let mut psg_gg = Psg::new(true, 44100.0);
        psg_gg.write_stereo(0x0F);
        assert_eq!(psg_gg.stereo, 0x0F, "GG aplica write_stereo");
    }

    #[test]
    fn tone0_zero_treated_as_0x400() {
        // Quando registrador de tom = 0, o contador deve recarregar com 0x400
        // Isso é verificado indiretamente: a saída não deve travar (sample gerado sem panic)
        let mut psg = Psg::new(false, 44100.0);
        psg.write_data(0x80); // latch reg0, data=0
        psg.write_data(0x00); // upper 6 bits = 0 → reg[0] = 0
        psg.write_data(0x90); // volume 0
        for _ in 0..200 { psg.generate_sample(); } // não deve panicar
    }

    #[test]
    fn all_channels_start_silent() {
        let psg = Psg::new(false, 44100.0);
        assert_eq!(psg.registers[1], 0x0F, "vol canal 0 inicia em 15 (silêncio)");
        assert_eq!(psg.registers[3], 0x0F, "vol canal 1 inicia em 15");
        assert_eq!(psg.registers[5], 0x0F, "vol canal 2 inicia em 15");
        assert_eq!(psg.registers[7], 0x0F, "vol noise inicia em 15");
    }
}
