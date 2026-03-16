/// Emulação do EEPROM serial 93C46 (Microwire 3-wire, 1Kbit = 64 palavras × 16 bits).
///
/// Jogos Game Gear que usam este chip:
///   - Majors Pro Baseball          (CRC32: 0x36EBCD6D)
///   - Pro Yakyuu GG League         (CRC32: 0x2DA8E943)
///   - World Series Baseball [v0]   (CRC32: 0x3D8D0DD6)
///   - World Series Baseball [v1]   (CRC32: 0xBB38CFD7)
///   - World Series Baseball '95    (CRC32: 0x578A8A38)
///
/// Mapeamento de sinais (acesso serial via $8000):
///   Escrita → bit 0 = DI (Data In), bit 1 = CLK, bit 2 = CS
///   Leitura ← bit 3 = DO (Data Out)
///
/// Acesso direto (via $8008–$8087):
///   Leitura/escrita direta dos 128 bytes de armazenamento (endereçamento em bytes, LE).

#[derive(PartialEq)]
enum State {
    /// Aguardando o start bit (DI=1 na borda de subida do CLK)
    Start,
    /// Recebendo 8 bits: 2 opcode + 6 endereço
    Opcode,
    /// Enviando 16 bits de dados (READ)
    Reading,
    /// Recebendo 16 bits de dados (WRITE / WRAL)
    Writing,
}

pub struct Eeprom93C46 {
    /// 128 bytes = 64 palavras × 16 bits (armazenamento persistido no .eep)
    pub data: [u8; 128],
    /// true quando data foi modificado desde o último save
    pub dirty: bool,

    // Estado das linhas na última escrita
    cs_prev: bool,
    clk_prev: bool,

    // Proteção de escrita (desabilitada por padrão; EWEN a habilita)
    write_enabled: bool,

    // Máquina de estados
    state: State,
    position: u8,     // bits restantes na fase atual
    opcode_reg: u16,  // acumula bits durante a fase Opcode
    addr: u8,         // endereço de palavra (0-63) decodificado
    latch: u16,       // palavra sendo recebida durante Writing
    write_all: bool,  // WRAL: escrever em todos os endereços

    // Saída serial (DO)
    out_bit: bool,
    out_reg: u16,     // palavra sendo enviada durante Reading
}

impl Eeprom93C46 {
    pub fn new() -> Self {
        Self {
            data: [0xFF; 128], // EEPROM apagada = 0xFF
            dirty: false,
            cs_prev: false,
            clk_prev: false,
            write_enabled: false,
            state: State::Start,
            position: 0,
            opcode_reg: 0,
            addr: 0,
            latch: 0,
            write_all: false,
            out_bit: true, // DO=1 = pronto
            out_reg: 0,
        }
    }

    /// Processa uma escrita no registrador de controle serial ($8000).
    /// bits: 0=DI, 1=CLK, 2=CS
    pub fn write_control(&mut self, value: u8) {
        let cs  = (value & 0x04) != 0;
        let clk = (value & 0x02) != 0;
        let di  = (value & 0x01) != 0;

        // Borda descendente de CS (1→0): reseta a máquina de estados
        if self.cs_prev && !cs {
            self.reset_state();
        }

        // Borda de subida do CLK enquanto CS está ativo
        if cs && !self.clk_prev && clk {
            self.process_rising_clk(di);
        }

        self.cs_prev = cs;
        self.clk_prev = clk;
    }

    /// Lê o estado do DO ($8000, bit 3).
    pub fn read_control(&self) -> u8 {
        if self.out_bit { 0x08 } else { 0x00 }
    }

    /// Leitura direta de byte ($8008–$8087): offset = addr - $8008.
    pub fn direct_read(&self, offset: u8) -> u8 {
        self.data[offset as usize]
    }

    /// Escrita direta de byte ($8008–$8087): offset = addr - $8008.
    pub fn direct_write(&mut self, offset: u8, value: u8) {
        self.data[offset as usize] = value;
        self.dirty = true;
    }

    // ── Internos ──────────────────────────────────────────────────────────────

    fn reset_state(&mut self) {
        self.state = State::Start;
        self.position = 0;
        self.opcode_reg = 0;
        self.latch = 0;
        self.write_all = false;
        self.out_bit = true;
        // write_enabled e data persistem entre transações
    }

    fn process_rising_clk(&mut self, di: bool) {
        match self.state {
            State::Start => {
                // O start bit é sempre 1
                if di {
                    self.state = State::Opcode;
                    self.opcode_reg = 0;
                    self.position = 8; // 2 opcode + 6 endereço
                }
            }
            State::Opcode => {
                self.opcode_reg = (self.opcode_reg << 1) | (di as u16);
                self.position -= 1;
                if self.position == 0 {
                    self.decode_command();
                }
            }
            State::Reading => {
                // Atualiza DO antes de decrementar (o bit atual já foi lido pelo game)
                if self.position > 0 {
                    self.position -= 1;
                    self.out_bit = if self.position > 0 {
                        (self.out_reg >> (self.position - 1)) & 1 != 0
                    } else {
                        true // DO=1 após último bit
                    };
                }
                if self.position == 0 {
                    self.state = State::Start;
                }
            }
            State::Writing => {
                self.latch = (self.latch << 1) | (di as u16);
                self.position -= 1;
                if self.position == 0 {
                    if self.write_enabled {
                        if self.write_all {
                            for a in 0..64u8 {
                                self.write_word(a, self.latch);
                            }
                        } else {
                            self.write_word(self.addr, self.latch);
                        }
                        self.dirty = true;
                    }
                    self.write_all = false;
                    self.state = State::Start;
                    self.out_bit = true; // DO=1 = escrita concluída
                }
            }
        }
    }

    fn decode_command(&mut self) {
        let op   = (self.opcode_reg >> 6) & 0x03;
        let addr = (self.opcode_reg & 0x3F) as u8;

        match op {
            // READ — envia 16 bits (com dummy 0 no início)
            0b10 => {
                self.addr = addr;
                self.out_reg = self.read_word(addr);
                self.state = State::Reading;
                self.position = 16;
                self.out_bit = false; // dummy bit 0
            }
            // WRITE — recebe 16 bits
            0b01 => {
                self.addr = addr;
                self.state = State::Writing;
                self.position = 16;
                self.latch = 0;
                self.write_all = false;
            }
            // ERASE — apaga uma palavra (→ 0xFFFF)
            0b11 => {
                if self.write_enabled {
                    self.write_word(addr, 0xFFFF);
                    self.dirty = true;
                }
                self.state = State::Start;
                self.out_bit = true;
            }
            // Comandos especiais (decodificados pelos bits 5-4 do endereço)
            0b00 => {
                match addr >> 4 {
                    0b00 => {
                        // EWDS — desabilita escrita
                        self.write_enabled = false;
                        self.state = State::Start;
                        self.out_bit = true;
                    }
                    0b01 => {
                        // WRAL — escreve o mesmo valor em todas as 64 palavras
                        self.state = State::Writing;
                        self.position = 16;
                        self.latch = 0;
                        self.write_all = true;
                    }
                    0b10 => {
                        // ERAL — apaga tudo (→ 0xFFFF)
                        if self.write_enabled {
                            for a in 0..64u8 {
                                self.write_word(a, 0xFFFF);
                            }
                            self.dirty = true;
                        }
                        self.state = State::Start;
                        self.out_bit = true;
                    }
                    0b11 => {
                        // EWEN — habilita escrita
                        self.write_enabled = true;
                        self.state = State::Start;
                        self.out_bit = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn read_word(&self, addr: u8) -> u16 {
        let base = (addr as usize) * 2;
        u16::from_le_bytes([self.data[base], self.data[base + 1]])
    }

    fn write_word(&mut self, addr: u8, word: u16) {
        let base = (addr as usize) * 2;
        let bytes = word.to_le_bytes();
        self.data[base]     = bytes[0];
        self.data[base + 1] = bytes[1];
    }
}
