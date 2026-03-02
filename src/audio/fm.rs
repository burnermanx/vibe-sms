#[repr(C)]
pub struct OPLL {
    _private: [u8; 0],
}

#[link(name = "emu2413")]
unsafe extern "C" {
    pub fn OPLL_new(clk: u32, rate: u32) -> *mut OPLL;
    pub fn OPLL_delete(opll: *mut OPLL);
    pub fn OPLL_reset(opll: *mut OPLL);
    pub fn OPLL_setRate(opll: *mut OPLL, rate: u32);
    pub fn OPLL_setQuality(opll: *mut OPLL, q: u8);
    pub fn OPLL_writeIO(opll: *mut OPLL, reg: u32, val: u8);
    pub fn OPLL_writeReg(opll: *mut OPLL, reg: u32, val: u8);
    pub fn OPLL_calc(opll: *mut OPLL) -> i16;
}

pub struct Fm {
    opll: *mut OPLL,
    address_latch: u8,
    fm_enable: bool,
}

impl Default for Fm {
    fn default() -> Self {
        Self::new()
    }
}

impl Fm {
    pub fn new() -> Self {
        unsafe {
            // Master System OPLL clock is typically 3.579545 MHz
            // Sample rate we use is 44100
            let opll = OPLL_new(3579545, 44100);
            OPLL_reset(opll);
            OPLL_setQuality(opll, 1); // 1 = good quality synthesis
            
            Self {
                opll,
                address_latch: 0,
                fm_enable: false, // Games that support FM will write 1 to port $F2
            }
        }
    }

    pub fn write_data(&mut self, port: u8, value: u8) {
        match port {
            0xF0 => {
                self.address_latch = value & 0x3F;
            },
            0xF1 => {
                unsafe {
                    OPLL_writeReg(self.opll, self.address_latch as u32, value);
                }
            },
            0xF2 => {
                self.fm_enable = (value & 0x01) != 0;
            },
            _ => {}
        }
    }

    pub fn read_data(&self, port: u8) -> u8 {
        match port {
            0xF2 => self.fm_enable as u8,
            // $F0 and $F1 typically return open bus ($FF) or internal status,
            // but for simplicity and safety against hardware detection logic, we return 0xFF.
            _ => 0xFF,
        }
    }

    pub fn generate_sample(&mut self) -> f32 {
        if !self.fm_enable {
            return 0.0;
        }

        unsafe {
            let sample = OPLL_calc(self.opll);
            // Convert i16 to f32 (-1.0 to 1.0)
            (sample as f32) / 32768.0
        }
    }
}

// Since we hold a raw pointer, we need to implement Drop to prevent memory leaks
impl Drop for Fm {
    fn drop(&mut self) {
        unsafe {
            if !self.opll.is_null() {
                OPLL_delete(self.opll);
            }
        }
    }
}

// Ensure it can be moved across threads if necessary (assuming OPLL is thread sound)
unsafe impl Send for Fm {}
