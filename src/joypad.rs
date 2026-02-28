pub struct Joypad {
    pub p1_up: bool,
    pub p1_down: bool,
    pub p1_left: bool,
    pub p1_right: bool,
    pub p1_b1: bool,
    pub p1_b2: bool,
}

impl Joypad {
    pub fn new() -> Self {
        Self {
            p1_up: false, p1_down: false, p1_left: false, p1_right: false,
            p1_b1: false, p1_b2: false,
        }
    }

    // Porta DC ($DC) - Entradas de Controle do Jogador 1 (e parte do Jogador 2)
    pub fn read_port_dc(&self) -> u8 {
        let mut port = 0xFF; // Invertido, apertar botão altera para 0
        if self.p1_up { port &= !0x01; }
        if self.p1_down { port &= !0x02; }
        if self.p1_left { port &= !0x04; }
        if self.p1_right { port &= !0x08; }
        if self.p1_b1 { port &= !0x10; }
        if self.p1_b2 { port &= !0x20; }
        
        port
    }

    // Porta DD ($DD) - Jogador 2 e outras funções
    pub fn read_port_dd(&self) -> u8 {
        0xFF // Simplificado para jogar só com controle 1 inicialmente
    }
}
