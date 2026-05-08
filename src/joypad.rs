pub(crate) struct Joypad {
    pub(crate) p1_up: bool,
    pub(crate) p1_down: bool,
    pub(crate) p1_left: bool,
    pub(crate) p1_right: bool,
    pub(crate) p1_b1: bool,
    pub(crate) p1_b2: bool,
    pub(crate) gg_start: bool,

    // Light Phaser
    pub(crate) lightgun_active: bool,
    pub(crate) mouse_x: u16,
    pub(crate) mouse_y: u16,
    pub(crate) th_pin_low: bool,

    // I/O Control
    pub(crate) port_3f: u8,
}

impl Joypad {
    pub(crate) fn new() -> Self {
        Self {
            p1_up: false, p1_down: false, p1_left: false, p1_right: false,
            p1_b1: false, p1_b2: false, gg_start: false,
            lightgun_active: false,
            mouse_x: 0,
            mouse_y: 0,
            th_pin_low: false,
            port_3f: 0xFF,
        }
    }

    // Porta DC ($DC) - Entradas de Controle do Jogador 1 (e parte do Jogador 2)
    pub(crate) fn read_port_dc(&self) -> u8 {
        let mut port = 0xFF; // Invertido, apertar botão altera para 0
        if self.p1_up { port &= !0x01; }
        if self.p1_down { port &= !0x02; }
        if self.p1_left { port &= !0x04; }
        if self.p1_right { port &= !0x08; }
        
        // The Light Phaser TRIGGER is physically wired to 'Button 1' (Bit 4)!
        if self.p1_b1 { port &= !0x10; }
        
        if self.p1_b2 { port &= !0x20; }
        
        port
    }

    // Porta DD ($DD) - Jogador 2 e outras funções
    // Port $3F I/O Control bits:
    //   Bit 1: Port A (P1) TH direction (1=output, 0=input)
    //   Bit 5: Port A (P1) TH output level
    //   Bit 3: Port B (P2) TH direction (1=output, 0=input)
    //   Bit 7: Port B (P2) TH output level
    pub(crate) fn read_port_dd(&self) -> u8 {
        let mut port = 0xFF;

        // Bit 6: Player 1 TH pin — always reflects physical pin state.
        // When Light Phaser pulls TH low, this reads 0 regardless of
        // port $3F direction setting (the pin is physically pulled low).
        if self.th_pin_low {
            port &= !0x40;
        }

        // Bit 7: Player 2 TH pin
        if (self.port_3f & 0x08) != 0 && (self.port_3f & 0x80) == 0 {
            port &= !0x80;
        }

        port
    }

    pub(crate) fn write_port_3f(&mut self, value: u8) {
        self.port_3f = value;
    }

    pub(crate) fn read_port_00(&self) -> u8 {
        let mut port = 0x7F; // Bit 7 is Start Button (Active low)
        if !self.gg_start {
            port |= 0x80;
        }
        port
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_buttons_all_high() {
        let j = Joypad::new();
        assert_eq!(j.read_port_dc() & 0x3F, 0x3F, "todos os botões soltos → bits 0-5 em 1");
    }

    #[test]
    fn up_clears_bit0() {
        let mut j = Joypad::new();
        j.p1_up = true;
        assert_eq!(j.read_port_dc() & 0x01, 0, "up pressed → bit 0 = 0 (active-low)");
        assert_ne!(j.read_port_dc() & 0x3E, 0, "outros bits devem continuar em 1");
    }

    #[test]
    fn down_clears_bit1() {
        let mut j = Joypad::new();
        j.p1_down = true;
        assert_eq!(j.read_port_dc() & 0x02, 0);
    }

    #[test]
    fn left_clears_bit2() {
        let mut j = Joypad::new();
        j.p1_left = true;
        assert_eq!(j.read_port_dc() & 0x04, 0);
    }

    #[test]
    fn right_clears_bit3() {
        let mut j = Joypad::new();
        j.p1_right = true;
        assert_eq!(j.read_port_dc() & 0x08, 0);
    }

    #[test]
    fn b1_clears_bit4() {
        let mut j = Joypad::new();
        j.p1_b1 = true;
        assert_eq!(j.read_port_dc() & 0x10, 0);
    }

    #[test]
    fn b2_clears_bit5() {
        let mut j = Joypad::new();
        j.p1_b2 = true;
        assert_eq!(j.read_port_dc() & 0x20, 0);
    }

    #[test]
    fn multiple_buttons_simultaneously() {
        let mut j = Joypad::new();
        j.p1_up = true;
        j.p1_b1 = true;
        let dc = j.read_port_dc();
        assert_eq!(dc & 0x01, 0, "up deve estar ativo");
        assert_eq!(dc & 0x10, 0, "b1 deve estar ativo");
        assert_eq!(dc & 0x22, 0x22, "down e b2 devem estar soltos");
    }

    #[test]
    fn th_pin_low_clears_bit6_of_port_dd() {
        let mut j = Joypad::new();
        j.th_pin_low = true;
        assert_eq!(j.read_port_dd() & 0x40, 0, "TH low → bit 6 = 0");
    }

    #[test]
    fn th_pin_high_keeps_bit6_of_port_dd() {
        let j = Joypad::new();
        assert_ne!(j.read_port_dd() & 0x40, 0, "TH high → bit 6 = 1");
    }

    #[test]
    fn gg_start_not_pressed_returns_0xff() {
        let j = Joypad::new();
        assert_eq!(j.read_port_00(), 0xFF, "start solto → bit 7 = 1 → 0xFF");
    }

    #[test]
    fn gg_start_pressed_clears_bit7() {
        let mut j = Joypad::new();
        j.gg_start = true;
        assert_eq!(j.read_port_00(), 0x7F, "start pressionado → bit 7 = 0 → 0x7F");
    }

    #[test]
    fn write_port_3f_stores_value() {
        let mut j = Joypad::new();
        j.write_port_3f(0xAB);
        assert_eq!(j.port_3f, 0xAB);
    }
}
