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

pub(crate) struct Eeprom93C46 {
    /// 128 bytes = 64 palavras × 16 bits (armazenamento persistido no .eep)
    pub(crate) data: [u8; 128],
    /// true quando data foi modificado desde o último save
    pub(crate) dirty: bool,

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
    pub(crate) fn new() -> Self {
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
    pub(crate) fn write_control(&mut self, value: u8) {
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
    pub(crate) fn read_control(&self) -> u8 {
        if self.out_bit { 0x08 } else { 0x00 }
    }

    /// Leitura direta de byte ($8008–$8087): offset = addr - $8008.
    pub(crate) fn direct_read(&self, offset: u8) -> u8 {
        self.data[offset as usize]
    }

    /// Escrita direta de byte ($8008–$8087): offset = addr - $8008.
    pub(crate) fn direct_write(&mut self, offset: u8, value: u8) {
        self.data[offset as usize] = value;
        self.dirty = true;
    }

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
                if self.position > 0 {
                    // Calcula o bit ANTES de decrementar: position=16 → bit15 (MSB), ..., position=1 → bit0
                    self.out_bit = (self.out_reg >> (self.position - 1)) & 1 != 0;
                    self.position -= 1;
                    if self.position == 0 {
                        self.state = State::Start;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // Microwire helpers

    /// Ativa CS (começa transação).
    fn begin_tx(e: &mut Eeprom93C46) {
        e.write_control(0x04); // CS=1, CLK=0, DI=0
    }

    /// Desativa CS (encerra transação e reseta a máquina de estados).
    fn end_tx(e: &mut Eeprom93C46) {
        e.write_control(0x00); // CS=0 → borda descendente → reset
    }

    /// Envia um bit (DI=di) via borda de subida do CLK. Retorna DO após a borda.
    fn clock_bit(e: &mut Eeprom93C46, di: bool) -> bool {
        let d = if di { 1u8 } else { 0u8 };
        e.write_control(0x04 | d);        // CS=1, CLK=0
        e.write_control(0x04 | 0x02 | d); // CS=1, CLK=1 → borda de subida
        (e.read_control() & 0x08) != 0    // retorna DO (bit 3)
    }

    /// Envia start bit (1) + 8 bits de comando (opcode MSB primeiro).
    /// cmd = (opcode << 6) | addr, enviado do bit 7 ao bit 0.
    fn send_cmd(e: &mut Eeprom93C46, opcode: u8, addr: u8) {
        clock_bit(e, true); // start bit sempre 1
        let cmd = (opcode << 6) | (addr & 0x3F);
        for i in (0..8).rev() {
            clock_bit(e, (cmd >> i) & 1 != 0);
        }
    }

    /// Transação completa: EWEN (habilita escrita).
    fn ewen(e: &mut Eeprom93C46) {
        begin_tx(e);
        send_cmd(e, 0b00, 0b11_0000); // op=00, addr bits 5-4=11 → EWEN
        end_tx(e);
    }

    /// Transação completa: EWDS (desabilita escrita).
    fn ewds(e: &mut Eeprom93C46) {
        begin_tx(e);
        send_cmd(e, 0b00, 0b00_0000); // op=00, addr bits 5-4=00 → EWDS
        end_tx(e);
    }

    /// Transação completa: WRITE — escreve word em addr.
    fn write_word_cmd(e: &mut Eeprom93C46, addr: u8, data: u16) {
        begin_tx(e);
        send_cmd(e, 0b01, addr);
        for i in (0..16).rev() {
            clock_bit(e, (data >> i) & 1 != 0);
        }
        end_tx(e);
    }

    /// Transação completa: READ — lê e retorna a word em addr (16 bits MSB-first).
    fn read_word_cmd(e: &mut Eeprom93C46, addr: u8) -> u16 {
        begin_tx(e);
        send_cmd(e, 0b10, addr);
        // Primeiro bit disponível em DO antes do clock é o dummy (0); ignoramos.
        // 16 clocks → 16 bits de dados, MSB primeiro.
        let mut word = 0u16;
        for _ in 0..16 {
            let bit = clock_bit(e, false);
            word = (word << 1) | (bit as u16);
        }
        end_tx(e);
        word
    }

    #[test]
    fn initial_data_is_erased() {
        let e = Eeprom93C46::new();
        assert_eq!(e.data, [0xFF; 128], "EEPROM nova deve estar apagada (0xFF)");
        assert!(!e.dirty);
    }

    #[test]
    fn write_without_ewen_is_ignored() {
        let mut e = Eeprom93C46::new();
        write_word_cmd(&mut e, 0, 0x1234);
        assert_eq!(e.direct_read(0), 0xFF, "write sem EWEN não deve alterar dados");
        assert_eq!(e.direct_read(1), 0xFF);
        assert!(!e.dirty);
    }

    #[test]
    fn ewen_enables_write() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        write_word_cmd(&mut e, 5, 0xABCD);
        assert!(e.dirty);
        // Verifica via acesso direto (LE: low byte em [10], high byte em [11])
        assert_eq!(e.direct_read(10), 0xCD);
        assert_eq!(e.direct_read(11), 0xAB);
    }

    #[test]
    fn read_returns_written_word() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        write_word_cmd(&mut e, 5, 0xABCD);
        assert_eq!(read_word_cmd(&mut e, 5), 0xABCD);
    }

    #[test]
    fn read_msb_first() {
        // Verifica que o protocolo serial envia os bits do MSB para o LSB
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        write_word_cmd(&mut e, 0, 0x8001); // bit15=1, bit0=1, demais=0
        assert_eq!(read_word_cmd(&mut e, 0), 0x8001);
    }

    #[test]
    fn ewds_prevents_write_after_ewen() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        ewds(&mut e);
        write_word_cmd(&mut e, 3, 0x5678);
        assert_eq!(read_word_cmd(&mut e, 3), 0xFFFF, "EWDS deve bloquear escrita");
    }

    #[test]
    fn erase_word_sets_0xffff() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        write_word_cmd(&mut e, 0, 0x1234);
        begin_tx(&mut e);
        send_cmd(&mut e, 0b11, 0); // ERASE addr=0
        end_tx(&mut e);
        assert_eq!(read_word_cmd(&mut e, 0), 0xFFFF);
    }

    #[test]
    fn erase_without_ewen_is_ignored() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        write_word_cmd(&mut e, 0, 0x1234);
        ewds(&mut e);
        begin_tx(&mut e);
        send_cmd(&mut e, 0b11, 0); // ERASE sem write_enabled
        end_tx(&mut e);
        assert_eq!(read_word_cmd(&mut e, 0), 0x1234, "ERASE sem EWEN não deve apagar");
    }

    #[test]
    fn eral_erases_all_words() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        write_word_cmd(&mut e, 0,  0x1111);
        write_word_cmd(&mut e, 10, 0x2222);
        write_word_cmd(&mut e, 63, 0x3333);
        begin_tx(&mut e);
        send_cmd(&mut e, 0b00, 0b10_0000); // ERAL
        end_tx(&mut e);
        for addr in 0..64u8 {
            assert_eq!(read_word_cmd(&mut e, addr), 0xFFFF, "ERAL deve apagar addr {}", addr);
        }
    }

    #[test]
    fn wral_writes_all_words() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        begin_tx(&mut e);
        send_cmd(&mut e, 0b00, 0b01_0000); // WRAL
        for i in (0..16).rev() {
            clock_bit(&mut e, (0xBEEFu16 >> i) & 1 != 0);
        }
        end_tx(&mut e);
        for addr in 0..64u8 {
            assert_eq!(read_word_cmd(&mut e, addr), 0xBEEF, "WRAL deve preencher addr {}", addr);
        }
    }

    #[test]
    fn wral_without_ewen_is_ignored() {
        let mut e = Eeprom93C46::new();
        begin_tx(&mut e);
        send_cmd(&mut e, 0b00, 0b01_0000); // WRAL sem EWEN
        for i in (0..16).rev() {
            clock_bit(&mut e, (0xBEEFu16 >> i) & 1 != 0);
        }
        end_tx(&mut e);
        assert_eq!(read_word_cmd(&mut e, 0), 0xFFFF);
    }

    #[test]
    fn cs_falling_edge_resets_mid_command() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        // Começa um WRITE mas abandona no meio
        begin_tx(&mut e);
        clock_bit(&mut e, true);  // start bit
        clock_bit(&mut e, false); // 1º bit do opcode
        end_tx(&mut e); // CS cai → reset
        // Transação completa após o reset deve funcionar normalmente
        write_word_cmd(&mut e, 7, 0xDEAD);
        assert_eq!(read_word_cmd(&mut e, 7), 0xDEAD);
    }

    #[test]
    fn direct_read_write() {
        let mut e = Eeprom93C46::new();
        e.direct_write(0, 0x42);
        e.direct_write(1, 0x13);
        assert_eq!(e.direct_read(0), 0x42);
        assert_eq!(e.direct_read(1), 0x13);
        assert!(e.dirty);
    }

    #[test]
    fn direct_and_serial_access_share_storage() {
        // word 2 está nos bytes [4] (low) e [5] (high) em LE
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        write_word_cmd(&mut e, 2, 0x1234);
        assert_eq!(e.direct_read(4), 0x34, "low byte de word[2]");
        assert_eq!(e.direct_read(5), 0x12, "high byte de word[2]");
    }

    #[test]
    fn write_all_64_words_and_read_back() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        for addr in 0..64u8 {
            let val = 0x0100u16 * addr as u16 + addr as u16;
            write_word_cmd(&mut e, addr, val);
        }
        for addr in 0..64u8 {
            let expected = 0x0100u16 * addr as u16 + addr as u16;
            assert_eq!(read_word_cmd(&mut e, addr), expected, "addr {}", addr);
        }
    }

    #[test]
    fn dirty_cleared_externally() {
        let mut e = Eeprom93C46::new();
        ewen(&mut e);
        write_word_cmd(&mut e, 0, 0x1111);
        assert!(e.dirty);
        e.dirty = false;
        assert!(!e.dirty);
    }
}
