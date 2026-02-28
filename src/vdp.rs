#[derive(PartialEq)]
enum VdpMode {
    VramRead,
    VramWrite,
    CramWrite,
}

pub struct Vdp {
    pub vram: [u8; 16384], // 16KB VRAM
    pub cram: [u8; 32],    // 32 Bytes CRAM (Paleta de cores)
    pub registers: [u8; 16], // Registradores do VDP (0-10 no SMS)
    
    pub frame_buffer: [u32; 256 * 192], // Pixels finais a serem desenhados na janela
    
    // Máquina de estados da porta de controle
    control_word: u16,
    first_byte_received: bool,
    mode: VdpMode,
    address_register: u16,
    read_buffer: u8, // Buffer de leitura atrasada da VRAM
    pub vblank_flag: bool,
    pub line_interrupt_flag: bool,
    pub v_counter: u8,
    pub h_counter: u8,
}

impl Vdp {
    pub fn new() -> Self {
        Self {
            vram: [0; 16384],
            cram: [0; 32],
            registers: [0; 16],
            frame_buffer: [0xFF000000; 256 * 192],
            control_word: 0,
            first_byte_received: false,
            mode: VdpMode::VramRead,
            address_register: 0,
            read_buffer: 0,
            vblank_flag: false,
            line_interrupt_flag: false,
            v_counter: 0,
            h_counter: 0,
        }
    }

    // Traduz dados da CRAM (formato SMS 6 bits bbggrr) para XRGB (32 bits)
    fn get_color(&self, cram_address: usize) -> u32 {
// ... mantido o código anterior para espaço visual ...
        let color_byte = self.cram[cram_address & 0x1F];
        let r = (color_byte & 0x03) * 85;
        let g = ((color_byte >> 2) & 0x03) * 85;
        let b = ((color_byte >> 4) & 0x03) * 85;
        0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }

    pub fn render_frame(&mut self) {
        // Obter endereço da base do Name Table (Register 2 garante qual é a tabela 0x3800)
        let name_table_base = ((self.registers[2] & 0x0E) as usize) << 10;
        
        let scroll_x = self.registers[8] as usize; // Até 255
        let scroll_y = self.registers[9] as usize; // Até 223/255
        // Registrador 0 contem flags que inibem scrolling em certas áreas (não vamos simular por simplicidade nesta V1)
        
        for screen_y in 0..192 {
            // Emulação do scroll Vertical, Master System tem colunas de 28 tiles (224 linhas virtuais) no name table
            let bg_y = (screen_y + scroll_y) % 224;
            let row = bg_y / 8;
            let tile_y = bg_y % 8;
            
            for screen_x in 0..256 {
                // Emulação do scroll Horizontal
                let bg_x = (256 - scroll_x + screen_x) % 256; 
                let col = bg_x / 8;
                let tile_x = bg_x % 8;
                
                // Endereço do tile word na Name Table (Linha tem 32 colunas, 2 bytes/column)
                let nt_addr = name_table_base + (row * 32 + col) * 2;
                
                let tile_data_lo = self.vram[nt_addr] as u16;
                let tile_data_hi = self.vram[nt_addr + 1] as u16;
                let tile_word = tile_data_lo | (tile_data_hi << 8);
                
                let tile_index = (tile_word & 0x01FF) as usize;
                let h_flip = (tile_word & 0x0200) != 0;
                let v_flip = (tile_word & 0x0400) != 0;
                let palette_bank = if (tile_word & 0x0800) != 0 { 16 } else { 0 };
                
                let tile_base_addr = tile_index * 32;
                
                // Tratar flip Vertical
                let y_offset = if v_flip { 7 - tile_y } else { tile_y };
                let plane0 = self.vram[tile_base_addr + y_offset * 4];
                let plane1 = self.vram[tile_base_addr + y_offset * 4 + 1];
                let plane2 = self.vram[tile_base_addr + y_offset * 4 + 2];
                let plane3 = self.vram[tile_base_addr + y_offset * 4 + 3];
                
                // Tratar flip Horizontal
                let bit_offset = if h_flip { tile_x } else { 7 - tile_x };
                let mask = 1 << bit_offset;
                
                let mut color_index = 0;
                if (plane0 & mask) != 0 { color_index |= 1; }
                if (plane1 & mask) != 0 { color_index |= 2; }
                if (plane2 & mask) != 0 { color_index |= 4; }
                if (plane3 & mask) != 0 { color_index |= 8; }
                
                let final_color_addr = palette_bank + color_index;
                let argb = self.get_color(final_color_addr);
                
                self.frame_buffer[screen_y * 256 + screen_x] = argb;
            }
        }
        
        // Renderização de Sprites (Hardware de Mobilidade)
        let sat_base = ((self.registers[5] & 0x7E) as usize) << 7;
        let sprite_tile_base = ((self.registers[6] & 0x04) as usize) << 11;
        let is_8x16 = (self.registers[1] & 0x02) != 0;
        let sprite_height = if is_8x16 { 16 } else { 8 };
        
        // O Master System suporta até 64 sprites
        for i in 0..64 {
            let y_pos = self.vram[sat_base + i];
            
            // Y = 208 (0xD0) em modo de 192 linhas termina a lista de sprites
            if y_pos == 208 {
                break;
            }
            
            // Coordenada Y física (SMS subtrai 1 ou adiciona 1 da posição nominal)
            let actual_y = (y_pos as usize + 1) % 256;
            
            let x_pos = self.vram[sat_base + 0x80 + (i * 2)] as usize;
            let mut tile_index = self.vram[sat_base + 0x80 + (i * 2) + 1] as usize;
            
            if is_8x16 {
                tile_index &= 0xFE; // IGNORA O LSB se for 8x16
            }
            
            let mut draw_y = actual_y;
            
            for y in 0..sprite_height {
                if draw_y >= 192 {
                    draw_y += 1;
                    continue; // Fora da tela visível
                }
                
                // Qual banco do tile estamos desenhando (metade de cima ou baixo)?
                let current_tile = tile_index + (y / 8); 
                let line_in_tile = y % 8;
                
                let tile_addr = sprite_tile_base + (current_tile * 32);
                let plane0 = self.vram[tile_addr + line_in_tile * 4];
                let plane1 = self.vram[tile_addr + line_in_tile * 4 + 1];
                let plane2 = self.vram[tile_addr + line_in_tile * 4 + 2];
                let plane3 = self.vram[tile_addr + line_in_tile * 4 + 3];
                
                let mut draw_x = x_pos;
                for x in 0..8 {
                    if draw_x >= 256 {
                        draw_x += 1;
                        continue;
                    }
                    
                    let bit_offset = 7 - x; // Sprites não tem flip nativo no VDP do SMS1
                    let mask = 1 << bit_offset;
                    
                    let mut color_index = 0;
                    if (plane0 & mask) != 0 { color_index |= 1; }
                    if (plane1 & mask) != 0 { color_index |= 2; }
                    if (plane2 & mask) != 0 { color_index |= 4; }
                    if (plane3 & mask) != 0 { color_index |= 8; }
                    
                    // Em Sprites, Cor 0 é sempre transparente
                    if color_index != 0 {
                        // Sprites usam a segunda metade da Paleta de Cores (CRAM índices 16 a 31)
                        let argb = self.get_color(16 + color_index);
                        self.frame_buffer[draw_y * 256 + draw_x] = argb;
                    }
                    draw_x += 1;
                }
                draw_y += 1;
            }
        }
    }

    pub fn read_vcounter(&self) -> u8 { self.v_counter }
    pub fn read_hcounter(&self) -> u8 { self.h_counter }
    
    // Leitura na porta DATA ($BE)
    pub fn read_data(&mut self) -> u8 {
        self.first_byte_received = false; // Ler dados limpa o latch
        let data = self.read_buffer;
        // Na SMS, a leitura carrega o buffer com a VRAM atual, 
        // mas também retorna o que estava no buffer antes
        self.read_buffer = self.vram[self.address_register as usize];
        self.address_register = (self.address_register + 1) & 0x3FFF;
        data
    }
    
    // Escrita na porta DATA ($BE)
    pub fn write_data(&mut self, value: u8) {
        self.first_byte_received = false; // Latch é limpo ao ler ou escrever dados
        match self.mode {
            VdpMode::VramWrite | VdpMode::VramRead => {
                self.vram[self.address_register as usize] = value;
                self.read_buffer = value; // Atualiza o read buffer (SMS behavior)
            },
            VdpMode::CramWrite => {
                let cram_addr = (self.address_register & 0x1F) as usize;
                self.cram[cram_addr] = value;
            }
        }
        self.address_register = (self.address_register + 1) & 0x3FFF; // Auto-increment
    }
    
    // Leitura na porta CONTROL ($BF)
    pub fn read_control(&mut self) -> u8 {
        self.first_byte_received = false; // Limpa o latch
        
        let mut status = 0x00;
        if self.vblank_flag {
            status |= 0x80;
            self.vblank_flag = false; // Ler a porta de controle limpa a VBlank Interrupt Flag!
        }
        
        // As interrupções de linha não geram um bit de status no registro lido, mas a leitura da porta as limpa!
        self.line_interrupt_flag = false;
        
        // TODO: Sprite Overfow (bit 6), Sprite Collision (bit 5)
        
        status
    }
    
    // Escrita na porta CONTROL ($BF)
    pub fn write_control(&mut self, value: u8) {
        if !self.first_byte_received {
            // Primeiro byte: lower byte do word
            self.control_word = (self.control_word & 0xFF00) | (value as u16);
            self.first_byte_received = true;
        } else {
            // Segundo byte: upper byte e commando
            self.control_word = (self.control_word & 0x00FF) | ((value as u16) << 8);
            self.first_byte_received = false;
            
            let command = value >> 6;
            match command {
                0 => { // LER VRAM (00)
                    self.address_register = self.control_word & 0x3FFF;
                    self.mode = VdpMode::VramRead;
                    // Ao mandar commando de leitura, read buffer é populado imediatamente e o endereço avança
                    self.read_buffer = self.vram[self.address_register as usize];
                    self.address_register = (self.address_register + 1) & 0x3FFF;
                },
                1 => { // ESCREVER VRAM (01)
                    self.address_register = self.control_word & 0x3FFF;
                    self.mode = VdpMode::VramWrite;
                },
                2 => { // REGISTRADOR DO VDP (10)
                    let reg_index = value & 0x0F;
                    let reg_data = (self.control_word & 0x00FF) as u8;
                    if reg_index <= 10 {
                        self.registers[reg_index as usize] = reg_data;
                        // TODO: Tratar side-effects (ex: reg 1 habilitar vídeo mode/vblank)
                    }
                    self.mode = VdpMode::VramRead; // Comandos param modo write
                },
                3 => { // ESCREVER CRAM (11)
                    self.address_register = self.control_word & 0x3FFF;
                    self.mode = VdpMode::CramWrite;
                },
                _ => unreachable!()
            }
        }
    }
}
