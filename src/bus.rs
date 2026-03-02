pub struct Bus {
    // Aqui vai a instância da Memory Management Unit (MMU)
    // E conectar as portas de E/S (I/O) ao VDP e Joypad
    pub mmu: crate::mmu::Mmu,
    pub vdp: crate::vdp::Vdp,
    pub joypad: crate::joypad::Joypad,
    pub mixer: crate::audio::mixer::AudioMixer,
}

impl Bus {
    pub fn new(rom: Vec<u8>) -> Self {
        Self {
            mmu: crate::mmu::Mmu::new(rom),
            vdp: crate::vdp::Vdp::new(),
            joypad: crate::joypad::Joypad::new(),
            mixer: crate::audio::mixer::AudioMixer::new(),
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
            // Portas do controle/joypad (I/O do systema): $DC, $DD
            0xDC => self.joypad.read_port_dc(),
            0xDD => self.joypad.read_port_dd(),
            // FM Audio Control and Detection port ($F0 - $F2)
            0xF0..=0xF2 => self.mixer.fm.read_data(port),
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
            // Audio PSG ($40 a $7F)
            0x40..=0x7F => self.mixer.psg.write_data(value),
            // Controle de memória do Sistema ($3E e $3F espelhados de 0x00..0x3F)
            0x00..=0x3F => {
                if port % 2 == 0 {
                    // Bit 3 (0x08) = Cartridge RAM enable, etc (Memory Control)
                } else {
                    // Nationalization, Port A/B control (I/O Control)
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
}

impl System {
    pub fn new(bus: Bus) -> Self {
        Self {
            bus: std::cell::RefCell::new(bus),
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
