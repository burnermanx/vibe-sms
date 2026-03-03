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
    pub sprite_collision: bool,
    pub sprite_overflow: bool,
    pub v_counter: u8,
    pub h_counter: u8,
    pub h_latched: bool,
    pub latched_h_counter: u8,
    pub latched_v_counter: u8,
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
            sprite_collision: false,
            sprite_overflow: false,
            v_counter: 0,
            h_counter: 0,
            h_latched: false,
            latched_h_counter: 0,
            latched_v_counter: 0,
        }
    }

    pub fn latch_h_v_counters(&mut self) {
        // The real hardware always updates the latch when TH drops!
        self.latched_h_counter = self.h_counter;
        self.latched_v_counter = self.v_counter;
        self.h_latched = true;
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

    pub fn render_scanline(&mut self, screen_y: usize) {
        let display_enabled = (self.registers[1] & 0x40) != 0;
        let backdrop_color = self.get_color(16 + (self.registers[7] & 0x0F) as usize);

        if !display_enabled {
            for screen_x in 0..256 {
                self.frame_buffer[screen_y * 256 + screen_x] = backdrop_color;
            }
            return;
        }

        // Obter endereço da base do Name Table
        // No SMS1, o registrador 2 usa apenas os bits 1, 2 e 3 (0x0E). O bit 0 é ignorado.
        let name_table_base = ((self.registers[2] & 0x0E) as usize) << 10;
        
        let scroll_x = self.registers[8] as usize; // Até 255
        let scroll_y = self.registers[9] as usize; // Até 223/255
        
        // Registrador 0 contem flags que inibem scrolling e mascaram a primeira coluna
        let mask_col0 = (self.registers[0] & 0x20) != 0;
        let inhibit_hscroll = (self.registers[0] & 0x40) != 0;
        let inhibit_vscroll = (self.registers[0] & 0x80) != 0;

        let active_hscroll = if inhibit_hscroll && screen_y < 16 { 0 } else { scroll_x };
        
        for screen_x in 0..256 {
                // Mascarar os primeiros 8 pixels esconde sujeira de scroll do Master System
                if mask_col0 && screen_x < 8 {
                    self.frame_buffer[screen_y * 256 + screen_x] = backdrop_color;
                    continue;
                }
                
                // Emulação do scroll Vertical
                let active_vscroll = if inhibit_vscroll && screen_x >= 192 { 0 } else { scroll_y };
                let mut bg_y = screen_y + active_vscroll;
                if active_vscroll < 224 {
                    // Standard 28-row wrap
                    bg_y %= 224;
                } else {
                    // Values 224-255 wrap at 32-rows (256)
                    bg_y %= 256;
                }
                
                let row = bg_y / 8;
                let tile_y = bg_y % 8;
                
                // Emulação do scroll Horizontal
                let effective_x = (screen_x + (256 - active_hscroll)) % 256;
                let col = (effective_x / 8) % 32;
                let tile_x = effective_x % 8;
                
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
        
        // Renderização de Sprites (Hardware de Mobilidade)
        // Sprite Attribute Table (Register 5). No SMS, o bit 0 é ignorado (0x7E)
        let sat_base = ((self.registers[5] & 0x7E) as usize) << 7;
        
        // Sprite Pattern Generator Base (Register 6). No SMS, apenas o bit 2 interessa (0x04)
        let sprite_tile_base = ((self.registers[6] & 0x04) as usize) << 11;
        let is_8x16 = (self.registers[1] & 0x02) != 0;
        let sprite_height = if is_8x16 { 16 } else { 8 };
        
        // Master System suporta Sprite Shift left by 8 pixels (Early Clock)
        let sprite_shift = (self.registers[0] & 0x08) != 0;
        
        // Coleta de sprites válidos para este frame (até encontrar terminador 0xD0)
        let mut valid_sprites = Vec::new();
        for i in 0..64 {
            let y_pos = self.vram[sat_base + i];
            if y_pos == 208 { // 0xD0 termina a lista em mode 192 linhas
                break;
            }
            let actual_y = (y_pos as usize + 1) % 256;
            
            let raw_x = self.vram[sat_base + 0x80 + (i * 2)] as i32;
            let mut x_pos = raw_x;
            if sprite_shift {
                x_pos -= 8;
            }
            
            let mut tile_index = self.vram[sat_base + 0x80 + (i * 2) + 1] as usize;
            if is_8x16 {
                tile_index &= 0xFE; // IGNORA O LSB se for 8x16
            }
            
            valid_sprites.push((actual_y, x_pos, tile_index));
        }

        // Desenhar sprites na scanline atual mapeando colisões e overflows (8 por linha max)
        let mut line_sprite_buffer = [false; 256];
        let mut sprites_on_this_line = 0;
        
        for (actual_y, x_pos, tile_index) in &valid_sprites {
                // Checar se este sprite intercepta esta linha atual
                if screen_y >= *actual_y && screen_y < *actual_y + sprite_height {
                    sprites_on_this_line += 1;
                    
                    // Master System só desenha os primeiros 8 sprites que encontrar na linha!
                    if sprites_on_this_line > 8 {
                        self.sprite_overflow = true;
                        continue; 
                    }
                    
                    let y_in_sprite = screen_y - *actual_y;
                    let current_tile = tile_index + (y_in_sprite / 8);
                    let line_in_tile = y_in_sprite % 8;
                    
                    let tile_addr = sprite_tile_base + (current_tile * 32);
                    let plane0 = self.vram[tile_addr + line_in_tile * 4];
                    let plane1 = self.vram[tile_addr + line_in_tile * 4 + 1];
                    let plane2 = self.vram[tile_addr + line_in_tile * 4 + 2];
                    let plane3 = self.vram[tile_addr + line_in_tile * 4 + 3];
                    
                    for x in 0..8 {
                        let draw_x = x_pos + x as i32;
                        if draw_x >= 0 && draw_x < 256 {
                            let draw_x_u = draw_x as usize;
                            let bit_offset = 7 - x; 
                            let mask = 1 << bit_offset;
                            
                            let mut color_index = 0;
                            if (plane0 & mask) != 0 { color_index |= 1; }
                            if (plane1 & mask) != 0 { color_index |= 2; }
                            if (plane2 & mask) != 0 { color_index |= 4; }
                            if (plane3 & mask) != 0 { color_index |= 8; }
                            
                            if color_index != 0 {
                                // Se um pixel opaco de sprite já foi desenhado aqui, há colisão
                                if line_sprite_buffer[draw_x_u] {
                                    self.sprite_collision = true;
                                } else {
                                    // Se não há colisão prévia, esse sprite tem a maior prioridade e desenhamos
                                    let argb = self.get_color(16 + color_index);
                                    self.frame_buffer[screen_y * 256 + draw_x_u] = argb;
                                    line_sprite_buffer[draw_x_u] = true;
                                }
                            }
                        }
                    }
                }
        }
    }

    pub fn read_vcounter(&mut self) -> u8 {
        self.v_counter
    }

    pub fn read_hcounter(&mut self) -> u8 {
        if self.h_latched {
            self.latched_h_counter
        } else {
            self.h_counter
        }
    }
    
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
        
        if self.sprite_overflow {
            status |= 0x40;
            self.sprite_overflow = false; // Flag reseta apos leitura
        }
        
        if self.sprite_collision {
            status |= 0x20;
            self.sprite_collision = false; // Flag reseta apos leitura
        }
        
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
