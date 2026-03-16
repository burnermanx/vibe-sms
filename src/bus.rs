use crate::platform::Platform;

pub struct Bus {
    pub mmu:    crate::mmu::Mmu,
    pub vdp:    crate::vdp::Vdp,
    pub joypad: crate::joypad::Joypad,
    pub mixer:  crate::audio::mixer::AudioMixer,
    pub platform: Platform,
}

impl Bus {
    pub fn new(rom: Vec<u8>, platform: Platform, sample_rate: f32) -> Self {
        Self {
            mmu:    crate::mmu::Mmu::new(rom, platform),
            vdp:    crate::vdp::Vdp::new(platform),
            joypad: crate::joypad::Joypad::new(platform.is_gg()),
            mixer:  crate::audio::mixer::AudioMixer::new(platform.is_gg(), sample_rate),
            platform,
        }
    }

    // Acesso à memória usando o barramento
    pub fn read(&mut self, addr: u16) -> u8 {
        self.mmu.read(addr)
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        self.mmu.write(addr, value);
    }

    // Leitura das portas de I/O ($00 - $FF)
    pub fn read_io(&mut self, port: u8) -> u8 {
        match port {
            // Portas do VDP: 0x80 a 0xBF
            0x80..=0xBF => {
                if port % 2 == 0 {
                    self.vdp.read_data()
                } else {
                    self.vdp.read_control()
                }
            },
            // Contadores do VDP: 0x40 a 0x7F
            0x40..=0x7F => {
                if port % 2 == 0 {
                    self.vdp.read_vcounter()
                } else {
                    self.vdp.read_hcounter()
                }
            },
            // Game Gear Start button and I/O ports
            0x00 => if self.platform.is_gg() { self.joypad.read_port_00() } else { 0xFF },
            // FM Audio Detection port ($F0 - $F2) — checked before the 0xC0-0xFF joypad mirror
            0xF0..=0xF2 => self.mixer.fm.read_data(port),
            // I/O ports: 0xC0-0xFF → Joypad (mirrored throughout this range)
            // Even ports = Port A ($DC equivalent), Odd ports = Port B ($DD equivalent)
            0xC0..=0xFF => {
                if port % 2 == 0 {
                    self.joypad.read_port_dc()
                } else {
                    self.joypad.read_port_dd()
                }
            },
            // Portas não mapeadas ou padrões
            _ => 0xFF,
        }
    }

    // Escrita nas portas de I/O ($00 - $FF)
    pub fn write_io(&mut self, port: u8, value: u8) {
        match port {
            // VDP Data e Control Port (espelhados 0x80 a 0xBF)
            0x80..=0xBF => {
                if port % 2 == 0 {
                    self.vdp.write_data(value)
                } else {
                    self.vdp.write_control(value)
                }
            },
            // Game Gear Stereo Panning (Port 0x06)
            0x06 => {
                if self.platform.is_gg() {
                    self.mixer.psg.write_stereo(value);
                }
            },
            // Audio PSG ($40 a $7F)
            0x40..=0x7F => self.mixer.psg.write_data(value),
            // Controle de memória do Sistema ($3E e $3F espelhados de 0x00..0x3F)
            0x00..=0x3F => {
                if port % 2 == 0 {
                    // Bit 3 (0x08) = Cartridge RAM enable, etc (Memory Control)
                } else {
                    // Nationalization, Port A/B control (I/O Control)
                    self.joypad.write_port_3f(value);
                }
            },
            // Audio FM ($F0 - $F2)
            0xF0..=0xF2 => self.mixer.fm.write_data(port, value),
            // Portas não mapeadas
            _ => {}, 
        }
    }
}

pub struct System {
    pub bus: std::cell::RefCell<Bus>,
    pub platform: Platform,
}

impl System {
    pub fn new(bus: Bus, platform: Platform) -> Self {
        Self {
            bus: std::cell::RefCell::new(bus),
            platform,
        }
    }
}

impl z80::Z80_io for System {
    fn read_byte(&self, addr: u16) -> u8 {
        self.bus.borrow_mut().read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.bus.borrow_mut().write(addr, value);
    }
    fn port_in(&self, addr: u16) -> u8 {
        self.bus.borrow_mut().read_io((addr & 0xFF) as u8)
    }
    fn port_out(&mut self, addr: u16, value: u8) {
        self.bus.borrow_mut().write_io((addr & 0xFF) as u8, value);
    }
}
