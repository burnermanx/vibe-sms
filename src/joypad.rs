pub struct Joypad {
    pub p1_up: bool,
    pub p1_down: bool,
    pub p1_left: bool,
    pub p1_right: bool,
    pub p1_b1: bool,
    pub p1_b2: bool,
    pub gg_start: bool,
    
    // Light Phaser
    pub lightgun_active: bool,
    pub mouse_x: u16,
    pub mouse_y: u16,
    pub th_pin_low: bool,
    
    // I/O Control
    pub port_3f: u8,
    pub is_gg: bool,
}

impl Joypad {
    pub fn new(is_gg: bool) -> Self {
        Self {
            p1_up: false, p1_down: false, p1_left: false, p1_right: false,
            p1_b1: false, p1_b2: false, gg_start: false,
            lightgun_active: false,
            mouse_x: 0,
            mouse_y: 0,
            th_pin_low: false,
            port_3f: 0xFF,
            is_gg,
        }
    }

    // Porta DC ($DC) - Entradas de Controle do Jogador 1 (e parte do Jogador 2)
    pub fn read_port_dc(&self) -> u8 {
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
    pub fn read_port_dd(&self) -> u8 {
        let mut port = 0xFF; // Simplificado para jogar só com controle 1 inicialmente
        if self.th_pin_low {
            // Meka uses port &= !0x40; (Bit 6) for Player 1 TH.
            port &= !0x40;
        }
        port
    }

    pub fn write_port_3f(&mut self, value: u8) {
        self.port_3f = value;
    }

    pub fn read_port_00(&self) -> u8 {
        let mut port = 0x7F; // Bit 7 is Start Button (Active low)
        if !self.gg_start {
            port |= 0x80;
        }
        port
    }
}
