use crate::platform::Platform;

#[derive(PartialEq)]
enum VdpMode {
    VramRead,
    VramWrite,
    CramWrite,
}

/// Fixed 16-colour hardware palette of the TMS9918A.
/// Indexed by colour code 0–15. Colour 0 is "Transparent" (rendered as backdrop).
const TMS_PALETTE: [u32; 16] = [
    0xFF000000, // 0  Transparent
    0xFF000000, // 1  Black
    0xFF21C842, // 2  Medium Green
    0xFF5EDC78, // 3  Light Green
    0xFF5455ED, // 4  Dark Blue
    0xFF7D76FC, // 5  Light Blue
    0xFFD4524D, // 6  Dark Red
    0xFF42EBF5, // 7  Cyan
    0xFFFC5554, // 8  Medium Red
    0xFFFF7978, // 9  Light Red
    0xFFD4C154, // 10 Dark Yellow
    0xFFE6CE80, // 11 Light Yellow
    0xFF21B03B, // 12 Dark Green
    0xFFC95BB4, // 13 Magenta
    0xFFCCCCCC, // 14 Gray
    0xFFFFFFFF, // 15 White
];

pub struct Vdp {
    pub vram: [u8; 16384],
    pub cram: [u8; 64],
    pub registers: [u8; 16],
    pub frame_buffer: [u32; 256 * 192],

    control_word: u16,
    first_byte_received: bool,
    mode: VdpMode,
    address_register: u16,
    read_buffer: u8,
    pub vblank_flag: bool,
    pub line_interrupt_flag: bool,
    pub sprite_collision: bool,
    pub sprite_overflow: bool,
    pub v_counter: u8,
    pub h_counter: u8,
    pub h_latched: bool,
    pub latched_h_counter: u8,
    pub latched_v_counter: u8,
    pub platform: Platform,
    pub cram_latch: u8,
}

impl Vdp {
    pub fn new(platform: Platform) -> Self {
        Self {
            vram: [0; 16384],
            cram: [0; 64],
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
            platform,
            cram_latch: 0,
        }
    }

    // ── Save-state helpers ────────────────────────────────────────────────────

    pub fn get_state(&self) -> crate::savestate::VdpState {
        crate::savestate::VdpState {
            vram:                self.vram,
            cram:                self.cram,
            registers:           self.registers,
            control_word:        self.control_word,
            first_byte_received: self.first_byte_received,
            mode: match self.mode {
                VdpMode::VramRead  => 0,
                VdpMode::VramWrite => 1,
                VdpMode::CramWrite => 2,
            },
            address_register:    self.address_register,
            read_buffer:         self.read_buffer,
            vblank_flag:         self.vblank_flag,
            line_interrupt_flag: self.line_interrupt_flag,
            sprite_collision:    self.sprite_collision,
            sprite_overflow:     self.sprite_overflow,
            v_counter:           self.v_counter,
            h_counter:           self.h_counter,
            h_latched:           self.h_latched,
            latched_h_counter:   self.latched_h_counter,
            latched_v_counter:   self.latched_v_counter,
            cram_latch:          self.cram_latch,
        }
    }

    pub fn load_state(&mut self, s: &crate::savestate::VdpState) {
        self.vram                = s.vram;
        self.cram                = s.cram;
        self.registers           = s.registers;
        self.control_word        = s.control_word;
        self.first_byte_received = s.first_byte_received;
        self.mode = match s.mode {
            1 => VdpMode::VramWrite,
            2 => VdpMode::CramWrite,
            _ => VdpMode::VramRead,
        };
        self.address_register    = s.address_register;
        self.read_buffer         = s.read_buffer;
        self.vblank_flag         = s.vblank_flag;
        self.line_interrupt_flag = s.line_interrupt_flag;
        self.sprite_collision    = s.sprite_collision;
        self.sprite_overflow     = s.sprite_overflow;
        self.v_counter           = s.v_counter;
        self.h_counter           = s.h_counter;
        self.h_latched           = s.h_latched;
        self.latched_h_counter   = s.latched_h_counter;
        self.latched_v_counter   = s.latched_v_counter;
        self.cram_latch          = s.cram_latch;
    }

    pub fn latch_h_v_counters(&mut self) {
        // The real hardware always updates the latch when TH drops!
        self.latched_h_counter = self.h_counter;
        self.latched_v_counter = self.v_counter;
        self.h_latched = true;
    }

    fn get_color(&self, cram_address: usize) -> u32 {
        if self.platform.is_gg() {
            // Game Gear Palette: 12-bit xxxxbbbbggggrrrr (Words at even addresses)
            let base_addr = (cram_address & 0x1F) * 2;
            let lo = self.cram[base_addr] as u16;
            let hi = self.cram[base_addr + 1] as u16;
            let color = lo | (hi << 8);
            
            let r = (color & 0x0F) * 17;
            let g = ((color >> 4) & 0x0F) * 17;
            let b = ((color >> 8) & 0x0F) * 17;
            0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
        } else {
            // Master System Palette: 6-bit ..bbggrr
            let color_byte = self.cram[cram_address & 0x1F];
            let r = (color_byte & 0x03) * 85;
            let g = ((color_byte >> 2) & 0x03) * 85;
            let b = ((color_byte >> 4) & 0x03) * 85;
            0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
        }
    }

    // ── TMS9918A rendering (SG-1000 / SC-3000) ───────────────────────────────

    /// Determine TMS9918A rendering mode from register bits M1, M2, M3, M4.
    fn tms_mode(&self) -> u8 {
        let m4 = (self.registers[0] >> 2) & 1; // SMS extension — if set, use Mode 4
        if m4 != 0 { return 4; }
        let m1 = (self.registers[1] >> 4) & 1; // Text
        let m3 = (self.registers[1] >> 3) & 1; // Multicolor (SMS calls this M2)
        let m2 = (self.registers[0] >> 1) & 1; // Graphics II (SMS calls this M3)
        match (m1, m2, m3) {
            (0, 0, 0) => 0, // Graphics I
            (1, 0, 0) => 1, // Text
            (0, 1, 0) => 2, // Graphics II
            (0, 0, 1) => 3, // Multicolor
            _         => 0, // Undefined — fall back to Graphics I
        }
    }

    /// TMS9918A Mode 0 — Graphics I (most common in SG-1000 games).
    fn render_tms_mode0(&mut self, screen_y: usize) {
        let display_enabled = (self.registers[1] & 0x40) != 0;
        let backdrop = TMS_PALETTE[(self.registers[7] & 0x0F) as usize];

        if !display_enabled {
            for x in 0..256 { self.frame_buffer[screen_y * 256 + x] = backdrop; }
            return;
        }

        let name_base    = (self.registers[2] as usize & 0x0F) << 10;
        let color_base   = (self.registers[3] as usize) << 6;
        let pattern_base = (self.registers[4] as usize & 0x07) << 11;

        let row    = screen_y / 8;
        let tile_y = screen_y % 8;

        for col in 0..32usize {
            let tile_index   = self.vram[name_base + row * 32 + col] as usize;
            let color_byte   = self.vram[(color_base + tile_index / 8) & 0x3FFF];
            let pattern_byte = self.vram[(pattern_base + tile_index * 8 + tile_y) & 0x3FFF];

            let fg_idx = (color_byte >> 4) as usize;
            let bg_idx = (color_byte & 0x0F) as usize;
            let fg = if fg_idx == 0 { backdrop } else { TMS_PALETTE[fg_idx] };
            let bg = if bg_idx == 0 { backdrop } else { TMS_PALETTE[bg_idx] };

            for bit in 0..8usize {
                let pixel_set = (pattern_byte >> (7 - bit)) & 1 != 0;
                self.frame_buffer[screen_y * 256 + col * 8 + bit] = if pixel_set { fg } else { bg };
            }
        }
        self.render_tms_sprites(screen_y);
    }

    /// TMS9918A Mode 1 — Text (40×24, 6-pixel-wide chars, no sprites).
    fn render_tms_mode1(&mut self, screen_y: usize) {
        let display_enabled = (self.registers[1] & 0x40) != 0;
        let backdrop = TMS_PALETTE[(self.registers[7] & 0x0F) as usize];

        if !display_enabled {
            for x in 0..256 { self.frame_buffer[screen_y * 256 + x] = backdrop; }
            return;
        }

        let name_base    = (self.registers[2] as usize & 0x0F) << 10;
        let pattern_base = (self.registers[4] as usize & 0x07) << 11;
        let fg = TMS_PALETTE[(self.registers[7] >> 4) as usize];

        let row    = screen_y / 8;
        let tile_y = screen_y % 8;

        // 8-pixel borders
        for x in 0..8usize   { self.frame_buffer[screen_y * 256 + x] = backdrop; }
        for x in 248..256usize { self.frame_buffer[screen_y * 256 + x] = backdrop; }

        for col in 0..40usize {
            let tile_index   = self.vram[(name_base + row * 40 + col) & 0x3FFF] as usize;
            let pattern_byte = self.vram[(pattern_base + tile_index * 8 + tile_y) & 0x3FFF];
            for bit in 0..6usize {
                let pixel_set = (pattern_byte >> (7 - bit)) & 1 != 0;
                self.frame_buffer[screen_y * 256 + 8 + col * 6 + bit] =
                    if pixel_set { fg } else { backdrop };
            }
        }
    }

    /// TMS9918A Mode 2 — Graphics II (3 screen zones, each with own 2KB pattern+color tables).
    fn render_tms_mode2(&mut self, screen_y: usize) {
        let display_enabled = (self.registers[1] & 0x40) != 0;
        let backdrop = TMS_PALETTE[(self.registers[7] & 0x0F) as usize];

        if !display_enabled {
            for x in 0..256 { self.frame_buffer[screen_y * 256 + x] = backdrop; }
            return;
        }

        let name_base    = (self.registers[2] as usize & 0x0F) << 10;
        // Pattern table: bit 2 of R4 selects 0x0000 or 0x2000
        let pattern_base = if (self.registers[4] & 0x04) != 0 { 0x2000usize } else { 0 };
        // Color table: bit 7 of R3 selects 0x2000; otherwise R3<<6
        let color_base   = if (self.registers[3] & 0x80) != 0 { 0x2000usize }
                           else { (self.registers[3] as usize) << 6 };

        let row    = screen_y / 8;
        let tile_y = screen_y % 8;
        let zone   = row / 8; // 0, 1, or 2 (each zone = 8 rows × 32 cols)

        for col in 0..32usize {
            let tile_index   = self.vram[(name_base + row * 32 + col) & 0x3FFF] as usize;
            let tile_offset  = zone * 256 + tile_index;
            let pattern_byte = self.vram[(pattern_base + tile_offset * 8 + tile_y) & 0x3FFF];
            let color_byte   = self.vram[(color_base   + tile_offset * 8 + tile_y) & 0x3FFF];

            let fg_idx = (color_byte >> 4) as usize;
            let bg_idx = (color_byte & 0x0F) as usize;
            let fg = if fg_idx == 0 { backdrop } else { TMS_PALETTE[fg_idx] };
            let bg = if bg_idx == 0 { backdrop } else { TMS_PALETTE[bg_idx] };

            for bit in 0..8usize {
                let pixel_set = (pattern_byte >> (7 - bit)) & 1 != 0;
                self.frame_buffer[screen_y * 256 + col * 8 + bit] = if pixel_set { fg } else { bg };
            }
        }
        self.render_tms_sprites(screen_y);
    }

    /// TMS9918A Mode 3 — Multicolor (4×4 pixel color blocks).
    fn render_tms_mode3(&mut self, screen_y: usize) {
        let display_enabled = (self.registers[1] & 0x40) != 0;
        let backdrop = TMS_PALETTE[(self.registers[7] & 0x0F) as usize];

        if !display_enabled {
            for x in 0..256 { self.frame_buffer[screen_y * 256 + x] = backdrop; }
            return;
        }

        let name_base    = (self.registers[2] as usize & 0x0F) << 10;
        let pattern_base = (self.registers[4] as usize & 0x07) << 11;

        let row        = screen_y / 8;
        let tile_y     = screen_y % 8;
        let color_line = tile_y / 4; // 0 = top half, 1 = bottom half

        for col in 0..32usize {
            let tile_index   = self.vram[(name_base + row * 32 + col) & 0x3FFF] as usize;
            let pattern_byte = self.vram[(pattern_base + tile_index * 8 + color_line * 4) & 0x3FFF];

            let left_idx  = (pattern_byte >> 4) as usize;
            let right_idx = (pattern_byte & 0x0F) as usize;
            let left  = if left_idx  == 0 { backdrop } else { TMS_PALETTE[left_idx] };
            let right = if right_idx == 0 { backdrop } else { TMS_PALETTE[right_idx] };

            for bit in 0..8usize {
                self.frame_buffer[screen_y * 256 + col * 8 + bit] =
                    if bit < 4 { left } else { right };
            }
        }
        self.render_tms_sprites(screen_y);
    }

    /// TMS9918A sprite renderer — shared by modes 0, 2, 3.
    ///
    /// SAT format: 4 bytes per sprite [Y, X, Name, Attr], 32 sprites max, 4/line limit.
    /// Y is the row above the sprite top (actual_y = Y + 1).  Y = 0xD0 terminates list.
    /// Attr bit 7 = early clock (shift left 32 px); bits 3:0 = colour (0 = transparent).
    fn render_tms_sprites(&mut self, screen_y: usize) {
        let sat_base  = (self.registers[5] as usize & 0x7F) << 7;
        let pat_base  = (self.registers[6] as usize & 0x07) << 11;
        let is_16x16  = (self.registers[1] & 0x02) != 0;
        let magnified = (self.registers[1] & 0x01) != 0;

        let pat_size  = if is_16x16 { 16usize } else { 8 };
        let draw_size = if magnified { pat_size * 2 } else { pat_size };

        let mut sprites_on_line = 0u32;
        let mut occupied = [false; 256];

        for i in 0..32usize {
            let y_byte = self.vram[(sat_base + i * 4) & 0x3FFF];
            if y_byte == 0xD0 { break; }

            let actual_y = y_byte.wrapping_add(1) as usize;
            // Determine if this sprite intersects screen_y (with possible wrap at 256)
            let y_in_sprite = if screen_y >= actual_y {
                let d = screen_y - actual_y;
                if d >= draw_size { continue; }
                d
            } else {
                // Sprite wraps past line 255
                let overflow = actual_y + draw_size;
                if overflow <= 256 { continue; }
                256 - actual_y + screen_y
            };

            sprites_on_line += 1;
            if sprites_on_line > 4 {
                self.sprite_overflow = true;
                break;
            }

            let x_byte    = self.vram[(sat_base + i * 4 + 1) & 0x3FFF];
            let name      = self.vram[(sat_base + i * 4 + 2) & 0x3FFF] as usize;
            let attr      = self.vram[(sat_base + i * 4 + 3) & 0x3FFF];
            let color_idx = (attr & 0x0F) as usize;
            if color_idx == 0 { continue; } // transparent

            let early_clock = (attr & 0x80) != 0;
            let x_origin = x_byte as i32 - if early_clock { 32 } else { 0 };
            let color = TMS_PALETTE[color_idx];

            // Row within the pattern (undo magnification)
            let pat_row = if magnified { y_in_sprite / 2 } else { y_in_sprite };

            // For 16×16: four 8×8 quadrant tiles — N, N+1, N+2, N+3 (name & 0xFC aligned)
            let tile_cols = if is_16x16 { 2usize } else { 1 };

            for tc in 0..tile_cols {
                let tile_index = if is_16x16 {
                    (name & 0xFC) + tc + if pat_row >= 8 { 2 } else { 0 }
                } else {
                    name
                };
                let tile_row_in_pat = if is_16x16 { pat_row % 8 } else { pat_row };
                let pat_byte = self.vram[(pat_base + tile_index * 8 + tile_row_in_pat) & 0x3FFF];

                for bit in 0..8usize {
                    if (pat_byte >> (7 - bit)) & 1 == 0 { continue; }

                    let pixel_count = if magnified { 2 } else { 1 };
                    for m in 0..pixel_count {
                        let draw_x = x_origin
                            + (tc * if magnified { 16 } else { 8 }) as i32
                            + (bit * pixel_count + m) as i32;
                        if draw_x < 0 || draw_x >= 256 { continue; }
                        let dx = draw_x as usize;
                        if occupied[dx] {
                            self.sprite_collision = true;
                        } else {
                            self.frame_buffer[screen_y * 256 + dx] = color;
                            occupied[dx] = true;
                        }
                    }
                }
            }
        }
    }

    pub fn render_scanline(&mut self, screen_y: usize) {
        // SG-1000 / SC-3000 use TMS9918A modes (not SMS Mode 4)
        if self.platform.is_sg_family() {
            match self.tms_mode() {
                1 => self.render_tms_mode1(screen_y),
                2 => self.render_tms_mode2(screen_y),
                3 => self.render_tms_mode3(screen_y),
                _ => self.render_tms_mode0(screen_y),
            }
            return;
        }

        // ── SMS / Game Gear — Mode 4 ──────────────────────────────────────────
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
                let bg_priority = (tile_word & 0x1000) != 0;
                
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
                
                // Store priority flag in bit 24 of frame_buffer for sprite compositing.
                // We use the alpha channel's bit 0 to encode tile priority.
                let priority_encoded = if bg_priority && color_index != 0 {
                    argb | 0x01000000  // Mark as high-priority BG pixel
                } else {
                    argb & 0xFE000000 | (argb & 0x00FFFFFF)  // Normal pixel
                };
                
                self.frame_buffer[screen_y * 256 + screen_x] = priority_encoded;
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
                                // Check BG priority: if the bg pixel has priority and is non-transparent,
                                // the sprite pixel is hidden behind it.
                                let bg_pixel = self.frame_buffer[screen_y * 256 + draw_x_u];
                                let bg_has_priority = (bg_pixel & 0x01000000) != 0;
                                
                                if bg_has_priority {
                                    // BG wins — don't draw sprite, but still track collision
                                    if line_sprite_buffer[draw_x_u] {
                                        self.sprite_collision = true;
                                    }
                                    line_sprite_buffer[draw_x_u] = true;
                                } else if line_sprite_buffer[draw_x_u] {
                                    self.sprite_collision = true;
                                } else {
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
                let addr = (self.address_register & 0x3F) as usize;
                if self.platform.is_gg() {
                    if addr % 2 == 0 {
                        self.cram_latch = value;
                    } else {
                        self.cram[addr - 1] = self.cram_latch;
                        self.cram[addr] = value;
                    }
                } else {
                    let addr_sms = (self.address_register & 0x1F) as usize;
                    self.cram[addr_sms] = value;
                }
                self.read_buffer = value; // SMS spec: write_data always updates read buffer
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
