pub struct Mmu {
    pub ram: [u8; 8192], // 8KB Work RAM ($C000 - $DFFF)
    pub rom: Vec<u8>,    // O Cartucho de Jogo
    pub cart_ram: [u8; 16384], // Até 16KB de RAM no Cartucho
    
    // Registradores do Sega Mapper
    pub ram_control: u8, // $FFFC
    pub rom_bank_0: usize,  // $FFFD (apenas a partir de $0400, $0000-$03FF é fixo)
    pub rom_bank_1: usize,  // $FFFE
    pub rom_bank_2: usize,  // $FFFF
}

impl Mmu {
    pub fn new(mut rom: Vec<u8>) -> Self {
        // Garantir no mínimo 3 bancos (48KB) para evitar bounds check panic
        if rom.len() < 0xC000 {
            rom.resize(0xC000, 0);
        }
        
        Self {
            ram: [0; 8192],
            rom,
            cart_ram: [0; 16384],
            ram_control: 0,
            rom_bank_0: 0,
            rom_bank_1: 1, // Padrão: banco 1
            rom_bank_2: 2, // Padrão: banco 2
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x03FF => {
                // Primeiros 1KB são FIXOS no Banco 0
                self.rom[addr as usize]
            },
            0x0400..=0x3FFF => {
                // Restante do Slot 0 ($0400-$3FFF) mapeado pelo rom_bank_0
                let offset = (self.rom_bank_0 * 0x4000) + (addr as usize);
                if offset < self.rom.len() { self.rom[offset] } else { 0xFF }
            },
            0x4000..=0x7FFF => {
                // ROM Slot 1 ($4000-$7FFF) mapeado pelo rom_bank_1
                let offset = (self.rom_bank_1 * 0x4000) + (addr as usize - 0x4000);
                if offset < self.rom.len() { self.rom[offset] } else { 0xFF }
            },
            0x8000..=0xBFFF => {
                // ROM Slot 2 ou RAM do Cartucho
                if (self.ram_control & 0x08) != 0 {
                    // RAM Habilitada neste slot
                    // Bit 2 ($04) define página da RAM
                    let ram_page = if (self.ram_control & 0x04) != 0 { 1 } else { 0 };
                    let offset = (ram_page * 0x2000) + (addr as usize - 0x8000);
                    self.cart_ram[offset]
                } else {
                    // ROM no Slot 2 ($8000-$BFFF)
                    let offset = (self.rom_bank_2 * 0x4000) + (addr as usize - 0x8000);
                    if offset < self.rom.len() { self.rom[offset] } else { 0xFF }
                }
            },
            0xC000..=0xDFFF => {
                // RAM interna
                self.ram[(addr - 0xC000) as usize]
            },
            0xE000..=0xFFFF => {
                // RAM Mirror e Registradores do Mapper
                self.ram[(addr - 0xE000) as usize]
            }
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0xBFFF => {
                // Escrita na RAM do Cartucho (se habilitada)
                if (self.ram_control & 0x08) != 0 {
                    let ram_page = if (self.ram_control & 0x04) != 0 { 1 } else { 0 };
                    let offset = (ram_page * 0x2000) + (addr as usize - 0x8000);
                    self.cart_ram[offset] = value;
                }
            },
            0xC000..=0xDFFF => {
                // RAM interna
                self.ram[(addr - 0xC000) as usize] = value;
            },
            0xE000..=0xFFFF => {
                // RAM Mirror
                self.ram[(addr - 0xE000) as usize] = value;
                
                // Mappers
                match addr {
                    0xFFFC => self.ram_control = value,
                    0xFFFD => self.rom_bank_0 = value as usize,
                    0xFFFE => self.rom_bank_1 = value as usize,
                    0xFFFF => self.rom_bank_2 = value as usize,
                    _ => {}
                }
            },
            _ => {
                // Descartar outras escritas na ROM
            }
        }
    }
}
