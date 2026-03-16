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
/// Exportado para testes.
pub(crate)
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

// ── Testes ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Cria um ROM de teste com `num_banks` bancos de 16KB.
    /// Cada byte contém o número do banco em que está.
    fn make_rom(num_banks: usize) -> Vec<u8> {
        let mut rom = vec![0u8; num_banks * 0x4000];
        for bank in 0..num_banks {
            for byte in 0..0x4000usize {
                rom[bank * 0x4000 + byte] = bank as u8;
            }
        }
        rom
    }

    // ── Banco fixo ($0000–$03FF) ──────────────────────────────────────────────

    #[test]
    fn first_1kb_is_always_bank0() {
        let mut rom = make_rom(4);
        // Marca o primeiro KB do banco 0 com valor especial
        for i in 0..0x400 { rom[i] = 0xAA; }
        let mut mmu = Mmu::new(rom, false);
        // Troca o banco 0 para o banco 2
        mmu.write(0xFFFD, 2);
        // O primeiro KB ($0000–$03FF) continua sendo do banco 0
        assert_eq!(mmu.read(0x0000), 0xAA);
        assert_eq!(mmu.read(0x03FF), 0xAA);
    }

    // ── Bank switching ($FFFD / $FFFE / $FFFF) ────────────────────────────────

    #[test]
    fn bank0_switching_affects_0400_to_3fff() {
        let rom = make_rom(4);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xFFFD, 3); // banco 0 → banco 3
        // $0400 em diante deve retornar 3 (número do banco)
        assert_eq!(mmu.read(0x0400), 3);
        assert_eq!(mmu.read(0x3FFF), 3);
    }

    #[test]
    fn bank1_switching_affects_4000_to_7fff() {
        let rom = make_rom(4);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xFFFE, 2); // banco 1 → banco 2
        assert_eq!(mmu.read(0x4000), 2);
        assert_eq!(mmu.read(0x7FFF), 2);
    }

    #[test]
    fn bank2_switching_affects_8000_to_bfff() {
        let rom = make_rom(4);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xFFFF, 3); // banco 2 → banco 3
        assert_eq!(mmu.read(0x8000), 3);
        assert_eq!(mmu.read(0xBFFF), 3);
    }

    #[test]
    fn bank_wraps_modulo_num_banks() {
        // 4 bancos: banco 4 → wraps para banco 0
        let rom = make_rom(4);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xFFFE, 4); // banco 1 = 4 mod 4 = 0
        assert_eq!(mmu.read(0x4000), 0);
        mmu.write(0xFFFE, 5); // 5 mod 4 = 1
        assert_eq!(mmu.read(0x4000), 1);
    }

    // ── Work RAM e espelhamento ────────────────────────────────────────────────

    #[test]
    fn work_ram_read_write() {
        let rom = make_rom(3);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xC000, 0x42);
        assert_eq!(mmu.read(0xC000), 0x42);
        mmu.write(0xDFFF, 0x99);
        assert_eq!(mmu.read(0xDFFF), 0x99);
    }

    #[test]
    fn ram_mirror_e000_reads_from_c000() {
        let rom = make_rom(3);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xC000, 0x55);
        assert_eq!(mmu.read(0xE000), 0x55, "$E000 deve espelhar $C000");
    }

    #[test]
    fn ram_mirror_write_visible_at_c000() {
        let rom = make_rom(3);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xE010, 0x77);
        assert_eq!(mmu.read(0xC010), 0x77, "escrita em $E010 deve refletir em $C010");
    }

    // ── SRAM do Cartucho ──────────────────────────────────────────────────────

    #[test]
    fn cart_ram_disabled_by_default() {
        let rom = make_rom(3);
        let mut mmu = Mmu::new(rom, false);
        // Por padrão rom_bank_2=2, então $8000 lê da ROM banco 2 (valor=2 em make_rom)
        let rom_value = mmu.read(0x8000);
        mmu.write(0x8000, 0xBB); // ram_control bit 3 = 0 → escrita ignorada
        // Leitura deve continuar retornando o valor da ROM (inalterado)
        assert_eq!(mmu.read(0x8000), rom_value, "cart RAM desabilitada: ROM não deve ser alterada");
        assert!(!mmu.sram_dirty);
    }

    #[test]
    fn cart_ram_enabled_by_bit3_of_ram_control() {
        let rom = make_rom(3);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xFFFC, 0x08); // habilita cart RAM
        mmu.write(0x8000, 0xCC);
        assert_eq!(mmu.read(0x8000), 0xCC);
        assert!(mmu.sram_dirty);
    }

    #[test]
    fn cart_ram_write_protect_bit0() {
        let rom = make_rom(3);
        let mut mmu = Mmu::new(rom, false);
        mmu.write(0xFFFC, 0x09); // habilita (bit3) + write-protect (bit0)
        mmu.write(0x8000, 0xDD);
        // Escrita deve ser bloqueada
        assert_ne!(mmu.read(0x8000), 0xDD);
        assert!(!mmu.sram_dirty);
    }

    #[test]
    fn cart_ram_page_selection_bit2() {
        let rom = make_rom(3);
        let mut mmu = Mmu::new(rom, false);
        // Página 0 (bit2=0)
        mmu.write(0xFFFC, 0x08);
        mmu.write(0x8000, 0x11);
        // Página 1 (bit2=1)
        mmu.write(0xFFFC, 0x0C);
        mmu.write(0x8000, 0x22);
        // Volta para página 0 — deve ler 0x11
        mmu.write(0xFFFC, 0x08);
        assert_eq!(mmu.read(0x8000), 0x11);
        // Página 1 — deve ler 0x22
        mmu.write(0xFFFC, 0x0C);
        assert_eq!(mmu.read(0x8000), 0x22);
    }

    // ── ROM padding ───────────────────────────────────────────────────────────

    #[test]
    fn rom_smaller_than_3_banks_is_padded() {
        let rom = vec![0xAA; 0x1000]; // 4KB — muito pequeno
        let mmu = Mmu::new(rom, false);
        assert!(mmu.rom.len() >= 0xC000, "ROM deve ser padded para pelo menos 48KB");
    }

    // ── CRC32 ─────────────────────────────────────────────────────────────────

    #[test]
    fn crc32_known_vector() {
        // CRC32 de "123456789" (ISO 3309 / IEEE 802.3) = 0xCBF43926
        let data = b"123456789";
        assert_eq!(crc32(data), 0xCBF43926);
    }

    #[test]
    fn crc32_empty_input() {
        // CRC32 de slice vazio = 0x00000000
        assert_eq!(crc32(&[]), 0x00000000);
    }

    // ── Detecção de EEPROM ────────────────────────────────────────────────────

    #[test]
    fn non_eeprom_rom_has_no_eeprom() {
        let rom = make_rom(3); // CRC aleatório → não está na lista
        let mmu = Mmu::new(rom, true); // is_gg = true
        assert!(mmu.eeprom.is_none(), "ROM desconhecida não deve ativar EEPROM");
    }

    #[test]
    fn sms_rom_never_has_eeprom() {
        // Mesmo que o CRC bata por acaso, is_gg=false impede EEPROM
        let rom = make_rom(3);
        let mmu = Mmu::new(rom, false);
        assert!(mmu.eeprom.is_none());
    }
}
