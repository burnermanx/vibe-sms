use crate::platform::Platform;

pub struct Bus {
    pub mmu:    crate::mmu::Mmu,
    pub vdp:    crate::vdp::Vdp,
    pub joypad: crate::joypad::Joypad,
    pub mixer:  crate::audio::mixer::AudioMixer,
    pub platform: Platform,
}

impl Bus {
    pub fn new(rom: Vec<u8>, platform: Platform, sample_rate: f32) -> Self {
        Self {
            mmu:    crate::mmu::Mmu::new(rom, platform),
            vdp:    crate::vdp::Vdp::new(platform),
            joypad: crate::joypad::Joypad::new(platform.is_gg()),
            mixer:  crate::audio::mixer::AudioMixer::new(platform.is_gg(), sample_rate),
            platform,
        }
    }

    /// Read a byte from the memory bus.
    pub fn read(&mut self, addr: u16) -> u8 {
        self.mmu.read(addr)
    }

    /// Write a byte to the memory bus.
    pub fn write(&mut self, addr: u16, value: u8) {
        self.mmu.write(addr, value);
    }

    /// Read from an I/O port ($00–$FF).
    pub fn read_io(&mut self, port: u8) -> u8 {
        match port {
            // VDP data/control ports: 0x80–0xBF
            0x80..=0xBF => {
                if port.is_multiple_of(2) {
                    self.vdp.read_data()
                } else {
                    self.vdp.read_control()
                }
            },
            // VDP V/H counters: 0x40–0x7F
            0x40..=0x7F => {
                if port.is_multiple_of(2) {
                    self.vdp.read_vcounter()
                } else {
                    self.vdp.read_hcounter()
                }
            },
            // Game Gear Start button and I/O ports
            0x00 => if self.platform.is_gg() { self.joypad.read_port_00() } else { 0xFF },
            // FM audio detection port ($F0–$F2) — checked before the 0xC0-0xFF joypad mirror
            0xF0..=0xF2 => self.mixer.fm.read_data(port),
            // Joypad ports: 0xC0–0xFF (mirrored throughout this range)
            // Even ports = Port A ($DC equivalent), Odd ports = Port B ($DD equivalent)
            0xC0..=0xFF => {
                if port.is_multiple_of(2) {
                    self.joypad.read_port_dc()
                } else {
                    self.joypad.read_port_dd()
                }
            },
            _ => 0xFF,
        }
    }

    /// Write to an I/O port ($00–$FF).
    pub fn write_io(&mut self, port: u8, value: u8) {
        match port {
            // VDP data/control ports: 0x80–0xBF
            0x80..=0xBF => {
                if port.is_multiple_of(2) {
                    self.vdp.write_data(value)
                } else {
                    self.vdp.write_control(value)
                }
            },
            // Game Gear stereo panning (port 0x06)
            0x06 => {
                if self.platform.is_gg() {
                    self.mixer.psg.write_stereo(value);
                }
            },
            // PSG audio ports: 0x40–0x7F
            0x40..=0x7F => self.mixer.psg.write_data(value),
            // System memory control ($3E/$3F, mirrored 0x00–0x3F)
            0x00..=0x3F => {
                if port.is_multiple_of(2) {
                    // Bit 3 (0x08) = Cartridge RAM enable (Memory Control)
                } else {
                    // Nationalization / Port A/B control (I/O Control)
                    self.joypad.write_port_3f(value);
                }
            },
            // FM audio ports: 0xF0–0xF2
            0xF0..=0xF2 => self.mixer.fm.write_data(port, value),
            _ => {},
        }
    }
}

pub struct System {
    pub bus: std::cell::RefCell<Bus>,
    pub platform: Platform,
}

impl System {
    pub fn new(bus: Bus, platform: Platform) -> Self {
        Self {
            bus: std::cell::RefCell::new(bus),
            platform,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;

    fn make_sms_bus() -> Bus {
        Bus::new(vec![0u8; 0xC000], Platform::MasterSystem, 44100.0)
    }

    // ── V/H counter reads (ports 0x40–0x7F) ──────────────────────────────────

    #[test]
    fn read_vcounter_on_even_ports_40_7f() {
        let mut bus = make_sms_bus();
        bus.vdp.v_counter = 0xAA;
        assert_eq!(bus.read_io(0x40), 0xAA);
        assert_eq!(bus.read_io(0x7E), 0xAA);
    }

    #[test]
    fn read_hcounter_on_odd_ports_40_7f() {
        let mut bus = make_sms_bus();
        bus.vdp.h_counter = 0x55;
        assert_eq!(bus.read_io(0x41), 0x55);
        assert_eq!(bus.read_io(0x7F), 0x55);
    }

    // ── PSG writes (ports 0x40–0x7F) ─────────────────────────────────────────

    #[test]
    fn write_psg_latch_updates_tone_register() {
        let mut bus = make_sms_bus();
        // Latch byte: ch0 tone, lower nibble = 1
        bus.write_io(0x7E, 0x81);
        assert_eq!(bus.mixer.psg.registers[0] & 0x0F, 1);
    }

    #[test]
    fn write_psg_volume_register() {
        let mut bus = make_sms_bus();
        // Latch byte: ch0 volume = 5  (bit7=1, bits6-5=ch0, bit4=vol, bits3-0=5)
        bus.write_io(0x40, 0x95);
        assert_eq!(bus.mixer.psg.registers[1], 5);
    }

    // ── Joypad reads (ports 0xC0–0xFF) ───────────────────────────────────────

    #[test]
    fn read_joypad_port_a_on_even_ports_c0_ff() {
        let mut bus = make_sms_bus();
        assert_eq!(bus.read_io(0xC0), 0xFF); // no buttons
        assert_eq!(bus.read_io(0xDC), 0xFF);
        assert_eq!(bus.read_io(0xFE), 0xFF);
    }

    #[test]
    fn read_joypad_port_b_on_odd_ports_c0_ff() {
        let mut bus = make_sms_bus();
        assert_eq!(bus.read_io(0xC1), 0xFF);
        assert_eq!(bus.read_io(0xDD), 0xFF);
        assert_eq!(bus.read_io(0xFF), 0xFF);
    }

    // ── Port 0x00 (GG Start) ──────────────────────────────────────────────────

    #[test]
    fn read_port_00_on_sms_returns_0xff() {
        let mut bus = make_sms_bus();
        assert_eq!(bus.read_io(0x00), 0xFF);
    }

    #[test]
    fn read_port_00_on_gg_returns_joypad_byte() {
        let mut bus = Bus::new(vec![0u8; 0xC000], Platform::GameGear, 44100.0);
        // Start not pressed → bit 7 high
        assert_eq!(bus.read_io(0x00) & 0x80, 0x80);
    }

    // ── VDP reads/writes (ports 0x80–0xBF) ───────────────────────────────────

    #[test]
    fn write_two_vdp_control_bytes_sets_vram_write_mode() {
        let mut bus = make_sms_bus();
        // Two consecutive writes: first byte (low addr), second byte (command 01 = VramWrite)
        bus.write_io(0x81, 0x05); // addr low = 0x05
        bus.write_io(0x81, 0x40); // command 01, addr high = 0x00 → addr = 0x0005, VramWrite
        // Verify by writing data and checking VRAM
        bus.write_io(0x80, 0xAB);
        assert_eq!(bus.vdp.vram[0x05], 0xAB);
    }

    #[test]
    fn write_vdp_data_on_even_ports_writes_vram() {
        let mut bus = make_sms_bus();
        // Set VDP to VRAM write mode (address 0, command 01xx_xxxx)
        bus.vdp.write_control(0x00);
        bus.vdp.write_control(0x40);
        bus.write_io(0x80, 0x42);
        assert_eq!(bus.vdp.vram[0], 0x42);
    }

    // ── GG stereo port 0x06 ───────────────────────────────────────────────────

    #[test]
    fn write_port_06_sets_gg_stereo() {
        let mut bus = Bus::new(vec![0u8; 0xC000], Platform::GameGear, 44100.0);
        bus.write_io(0x06, 0xFF);
        assert_eq!(bus.mixer.psg.stereo, 0xFF);
    }

    #[test]
    fn write_port_06_ignored_on_sms() {
        let mut bus = make_sms_bus();
        let before = bus.mixer.psg.stereo;
        bus.write_io(0x06, 0xFF);
        assert_eq!(bus.mixer.psg.stereo, before);
    }

    // ── FM ports 0xF0–0xF2 ───────────────────────────────────────────────────

    #[test]
    fn write_f2_enables_fm() {
        let mut bus = make_sms_bus();
        bus.write_io(0xF2, 0x01);
        assert_eq!(bus.read_io(0xF2), 0x01);
    }

    #[test]
    fn write_f2_disables_fm() {
        let mut bus = make_sms_bus();
        bus.write_io(0xF2, 0x01);
        bus.write_io(0xF2, 0x00);
        assert_eq!(bus.read_io(0xF2), 0x00);
    }

    #[test]
    fn user_disabled_fm_blocks_f2_enable() {
        let mut bus = make_sms_bus();
        bus.mixer.fm.user_disabled = true;
        bus.write_io(0xF2, 0x01);
        assert_eq!(bus.read_io(0xF2), 0x00);
    }

    // ── Unmapped ports ────────────────────────────────────────────────────────

    #[test]
    fn read_unmapped_ports_return_0xff() {
        let mut bus = make_sms_bus();
        assert_eq!(bus.read_io(0x20), 0xFF);
        assert_eq!(bus.read_io(0x38), 0xFF);
    }
}

impl z80::Z80_io for System {
    fn read_byte(&self, addr: u16) -> u8 {
        self.bus.borrow_mut().read(addr)
    }
    fn write_byte(&mut self, addr: u16, value: u8) {
        self.bus.borrow_mut().write(addr, value);
    }
    fn port_in(&self, addr: u16) -> u8 {
        self.bus.borrow_mut().read_io((addr & 0xFF) as u8)
    }
    fn port_out(&mut self, addr: u16, value: u8) {
        self.bus.borrow_mut().write_io((addr & 0xFF) as u8, value);
    }
}
