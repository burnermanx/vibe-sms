use crate::audio::ym2413::Ym2413;

pub struct Fm {
    ym2413: Ym2413,
    fm_enable: bool,
    /// When true, the FM chip is hidden from the game:
    /// port 0xF2 reads return 0 (FM absent) and writes are ignored.
    /// The game falls back to PSG automatically.
    pub user_disabled: bool,
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
            user_disabled: false,
        }
    }

    pub fn write_data(&mut self, port: u8, value: u8) {
        match port {
            0xF0 => self.ym2413.write_address(value),
            0xF1 => self.ym2413.write_data(value),
            0xF2 => {
                // When user has disabled FM, ignore the game's attempt to enable it.
                // This makes the game believe FM hardware is absent and use PSG instead.
                if !self.user_disabled {
                    self.fm_enable = (value & 0x01) != 0;
                }
            }
            _ => {}
        }
    }

    pub fn read_data(&mut self, port: u8) -> u8 {
        match port {
            // Return 0 when user-disabled so the game thinks FM chip is not present
            0xF2 => if !self.user_disabled && self.fm_enable { 1 } else { 0 },
            _ => 0xFF,
        }
    }

    pub fn generate_sample(&mut self) -> f32 {
        if self.user_disabled || !self.fm_enable {
            return 0.0;
        }
        self.ym2413.generate_sample()
    }
}
