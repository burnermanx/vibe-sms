use crate::eeprom::Eeprom93C46;

/// Jogos GG que usam EEPROM 93C46 em vez de SRAM (identificados por CRC32 do ROM).
/// Fonte: Gearsystem game_db.h
const EEPROM_CRCS: &[u32] = &[
    0x36EBCD6D, // Majors Pro Baseball
    0x2DA8E943, // Pro Yakyuu GG League
    0x3D8D0DD6, // World Series Baseball [v0]
    0xBB38CFD7, // World Series Baseball [v1]
    0x578A8A38, // World Series Baseball '95
];

/// Calcula o CRC32 (IEEE 802.3 / standard) dos dados do ROM.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

pub struct Mmu {
    pub ram: [u8; 8192],       // 8KB Work RAM ($C000–$DFFF)
    pub rom: Vec<u8>,          // O Cartucho de Jogo
    pub cart_ram: [u8; 16384], // Até 16KB de RAM no Cartucho (SRAM)
    pub sram_dirty: bool,      // true quando cart_ram foi modificada desde o último save

    // EEPROM 93C46 (apenas para jogos GG que a utilizam)
    pub eeprom: Option<Eeprom93C46>,

    // Registradores do Sega Mapper
    pub ram_control: u8,    // $FFFC
    pub rom_bank_0: usize,  // $FFFD
    pub rom_bank_1: usize,  // $FFFE
    pub rom_bank_2: usize,  // $FFFF
}

impl Mmu {
    pub fn new(mut rom: Vec<u8>, is_gg: bool) -> Self {
        // Detecta EEPROM antes de fazer padding (CRC32 do ROM original)
        let eeprom = if is_gg {
            let rom_crc = crc32(&rom);
            if EEPROM_CRCS.contains(&rom_crc) {
                println!("EEPROM 93C46 detectada (CRC32: {:#010X})", rom_crc);
                Some(Eeprom93C46::new())
            } else {
                None
            }
        } else {
            None
        };

        // Garantir no mínimo 3 bancos (48KB) para evitar bounds check panic
        if rom.len() < 0xC000 {
            rom.resize(0xC000, 0);
        }

        Self {
            ram: [0; 8192],
            rom,
            cart_ram: [0; 16384],
            sram_dirty: false,
            eeprom,
            ram_control: 0,
            rom_bank_0: 0,
            rom_bank_1: 1,
            rom_bank_2: 2,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x03FF => {
                // Primeiros 1KB são FIXOS no Banco 0
                self.rom[addr as usize]
            }
            0x0400..=0x3FFF => {
                let rom_banks = (self.rom.len() / 0x4000).max(1);
                let actual_bank = self.rom_bank_0 % rom_banks;
                let offset = (actual_bank * 0x4000) + (addr as usize);
                if offset < self.rom.len() { self.rom[offset] } else { 0xFF }
            }
            0x4000..=0x7FFF => {
                let rom_banks = (self.rom.len() / 0x4000).max(1);
                let actual_bank = self.rom_bank_1 % rom_banks;
                let offset = (actual_bank * 0x4000) + (addr as usize - 0x4000);
                if offset < self.rom.len() { self.rom[offset] } else { 0xFF }
            }
            0x8000..=0xBFFF => {
                // EEPROM 93C46 (acesso serial e direto)
                if let Some(ref eeprom) = self.eeprom {
                    return match addr {
                        0x8000 => eeprom.read_control(),
                        0x8008..=0x8087 => eeprom.direct_read((addr - 0x8008) as u8),
                        _ => 0xFF,
                    };
                }

                // SRAM do Cartucho (Sega Mapper padrão)
                if (self.ram_control & 0x08) != 0 {
                    let ram_page = if (self.ram_control & 0x04) != 0 { 1 } else { 0 };
                    let offset = (ram_page * 0x2000) + (addr as usize - 0x8000);
                    return self.cart_ram[offset];
                }

                // ROM no Slot 2
                let rom_banks = (self.rom.len() / 0x4000).max(1);
                let actual_bank = self.rom_bank_2 % rom_banks;
                let offset = (actual_bank * 0x4000) + (addr as usize - 0x8000);
                if offset < self.rom.len() { self.rom[offset] } else { 0xFF }
            }
            0xC000..=0xDFFF => {
                self.ram[(addr - 0xC000) as usize]
            }
            0xE000..=0xFFFF => {
                self.ram[(addr - 0xE000) as usize]
            }
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0xBFFF => {
                // EEPROM 93C46 (acesso serial e direto)
                if let Some(ref mut eeprom) = self.eeprom {
                    match addr {
                        0x8000 => eeprom.write_control(value),
                        0x8008..=0x8087 => eeprom.direct_write((addr - 0x8008) as u8, value),
                        _ => {}
                    }
                    return;
                }

                // SRAM do Cartucho (Sega Mapper padrão)
                // Bit 3 ($08): habilita cart RAM; Bit 0 ($01): write-protect (1 = protegido)
                if (self.ram_control & 0x08) != 0 && (self.ram_control & 0x01) == 0 {
                    let ram_page = if (self.ram_control & 0x04) != 0 { 1 } else { 0 };
                    let offset = (ram_page * 0x2000) + (addr as usize - 0x8000);
                    self.cart_ram[offset] = value;
                    self.sram_dirty = true;
                }
            }
            0xC000..=0xDFFF => {
                self.ram[(addr - 0xC000) as usize] = value;
            }
            0xE000..=0xFFFF => {
                // RAM Mirror
                self.ram[(addr - 0xE000) as usize] = value;

                // Mappers só recebem escritas, independentes da RAM física espelhada
                match addr {
                    0xFFFC => self.ram_control = value,
                    0xFFFD => self.rom_bank_0 = value as usize,
                    0xFFFE => self.rom_bank_1 = value as usize,
                    0xFFFF => self.rom_bank_2 = value as usize,
                    _ => {}
                }
            }
            _ => {
                // Descartar outras escritas na ROM
            }
        }
    }
}
