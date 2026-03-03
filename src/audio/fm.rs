use crate::audio::ym2413::Ym2413;

pub struct Fm {
    ym2413: Ym2413,
    fm_enable: bool,
}

impl Default for Fm {
    fn default() -> Self {
        Self::new()
    }
}

impl Fm {
    pub fn new() -> Self {
        Self {
            ym2413: Ym2413::new(3579545, 44100),
            fm_enable: false,
        }
    }

    pub fn write_data(&mut self, port: u8, value: u8) {
        match port {
            0xF0 => self.ym2413.write_address(value),
            0xF1 => self.ym2413.write_data(value),
            0xF2 => self.fm_enable = (value & 0x01) != 0,
            _ => {}
        }
    }

    pub fn read_data(&mut self, port: u8) -> u8 {
        match port {
            0xF2 => if self.fm_enable { 1 } else { 0 },
            _ => 0xFF,
        }
    }

    pub fn generate_sample(&mut self) -> f32 {
        if !self.fm_enable {
            return 0.0;
        }
        self.ym2413.generate_sample()
    }
}
