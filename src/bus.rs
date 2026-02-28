pub struct Bus {
    // Aqui vai a instância da Memory Management Unit (MMU)
    // E conectar as portas de E/S (I/O) ao VDP e Joypad
    pub mmu: crate::mmu::Mmu,
    pub vdp: crate::vdp::Vdp,
    pub joypad: crate::joypad::Joypad,
}

impl Bus {
    pub fn new(rom: Vec<u8>) -> Self {
        Self {
            mmu: crate::mmu::Mmu::new(rom),
            vdp: crate::vdp::Vdp::new(),
            joypad: crate::joypad::Joypad::new(),
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
            // Portas do VDP: $BE = Data, $BF = Control. $7E = VCounter, $7F = HCounter.
            0xBE | 0xBD => self.vdp.read_data(),
            0xBF => self.vdp.read_control(),
            0x7E => self.vdp.read_vcounter(),
            0x7F => self.vdp.read_hcounter(),
            // Portas do controle/joypad (I/O do systema): $DC, $DD
            0xDC => self.joypad.read_port_dc(),
            0xDD => self.joypad.read_port_dd(),
            // Portas não mapeadas ou padrões
            _ => {
                // println!("Unmapped I/O Read: port {:02X}", port);
                0xFF
            },
        }
    }

    // Escrita nas portas de I/O ($00 - $FF)
    pub fn write_io(&mut self, port: u8, value: u8) {
        match port {
            // VDP Data e Control Port ($BE e $BF, espelhados)
            0xBE | 0xBD => self.vdp.write_data(value),
            0xBF => self.vdp.write_control(value),
            // Controle de memória do Sistema ($3E)
            0x3E => {
                // Bit 3 (0x08) = Cartridge RAM enable
                // Bit 4 (0x10) = Cartridge ROM disable (BIOS enable)
                // Bit 5 (0x20) = I/O Chip disable
                // Bit 6 (0x40) = Work RAM disable
            },
            // Controle de I/O ($3F)
            0x3F => {
                // Nationalization, Port A/B control
            },
            // Portas não mapeadas
            _ => {
                // println!("Unmapped I/O Write: port {:02X}, val {:02X}", port, value);
            }, 
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
