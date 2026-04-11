/// Save-state binary format  (magic "VSMS", version 1)
///
/// All integers are little-endian. booleans are 1 byte (0/1).
/// f64 is stored as its IEEE-754 bit pattern (u64 LE).
///
/// The file is written by `SaveState::serialize()` and read by
/// `SaveState::deserialize()`.  Both functions fail fast on any mismatch
/// so a truncated or wrong-version file is simply ignored.
const MAGIC: &[u8; 4] = b"VSMS";
const VERSION: u8 = 2;

pub(crate) struct CpuState {
    pub(crate) af: u16, pub bc: u16, pub de: u16, pub hl: u16,
    pub(crate) af_alt: u16, pub bc_alt: u16, pub de_alt: u16, pub hl_alt: u16,
    pub(crate) pc: u16, pub sp: u16, pub ix: u16, pub iy: u16, pub mem_ptr: u16,
    pub(crate) i: u8, pub r: u8,
    pub(crate) iff1: bool, pub iff2: bool, pub halted: bool,
    pub(crate) interrupt_mode: u8, pub iff_delay: u8,
    pub(crate) irq_pending: u8, pub nmi_pending: u8, pub irq_data: u8,
}

pub(crate) struct MmuState {
    pub(crate) ram: [u8; 8192],
    pub(crate) cart_ram: [u8; 16384],
    pub(crate) ram_control: u8,
    pub(crate) rom_bank_0: usize,
    pub(crate) rom_bank_1: usize,
    pub(crate) rom_bank_2: usize,
}

pub(crate) struct VdpState {
    pub(crate) vram: [u8; 16384],
    pub(crate) cram: [u8; 64],
    pub(crate) registers: [u8; 16],
    pub(crate) control_word: u16,
    pub(crate) first_byte_received: bool,
    pub(crate) mode: u8,          // 0=VramRead, 1=VramWrite, 2=CramWrite
    pub(crate) address_register: u16,
    pub(crate) read_buffer: u8,
    pub(crate) vblank_flag: bool,
    pub(crate) line_interrupt_flag: bool,
    pub(crate) sprite_collision: bool,
    pub(crate) sprite_overflow: bool,
    pub(crate) v_counter: u8,
    pub(crate) h_counter: u8,
    pub(crate) h_latched: bool,
    pub(crate) latched_h_counter: u8,
    pub(crate) latched_v_counter: u8,
    pub(crate) cram_latch: u8,
}

pub(crate) struct PsgState {
    pub(crate) registers: [u16; 8],
    pub(crate) latch: u8,
    pub(crate) counters: [u16; 4],
    pub(crate) polarity: [i8; 4],
    pub(crate) noise_lfsr: u16,
    pub(crate) clock_frac: f64,
    pub(crate) stereo: u8,
}

pub(crate) struct EmuTimingState {
    pub(crate) vcounter: u16,
    pub(crate) cycles_accumulator: i32,
    pub(crate) line_interrupt_counter: u8,
    pub(crate) frame_cycles: u32,
}

pub(crate) struct SaveState {
    pub(crate) cpu:    CpuState,
    pub(crate) mmu:    MmuState,
    pub(crate) vdp:    VdpState,
    pub(crate) psg:    PsgState,
    pub(crate) timing: EmuTimingState,
}

struct Ser(Vec<u8>);

impl Ser {
    fn new() -> Self { Ser(Vec::with_capacity(64 * 1024)) }
    fn u8(&mut self, v: u8)   { self.0.push(v); }
    fn u16(&mut self, v: u16) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn u32(&mut self, v: u32) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn i8(&mut self, v: i8)   { self.0.push(v as u8); }
    fn i32(&mut self, v: i32) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn f64(&mut self, v: f64) { self.0.extend_from_slice(&v.to_bits().to_le_bytes()); }
    fn bool(&mut self, v: bool) { self.0.push(v as u8); }
    fn bytes(&mut self, v: &[u8]) { self.0.extend_from_slice(v); }
}

struct De<'a> { data: &'a [u8], pos: usize }

impl<'a> De<'a> {
    fn new(data: &'a [u8]) -> Self { De { data, pos: 0 } }

    fn u8(&mut self) -> Option<u8> {
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        let b = self.data.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        let b = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i8(&mut self) -> Option<i8>  { Some(self.u8()? as i8) }
    fn i32(&mut self) -> Option<i32> {
        let b = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f64(&mut self) -> Option<f64> {
        let b = self.data.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        let bits = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        Some(f64::from_bits(bits))
    }
    fn bool(&mut self) -> Option<bool> { Some(self.u8()? != 0) }
    fn bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let slice = self.data.get(self.pos..self.pos + N)?;
        self.pos += N;
        let mut arr = [0u8; N];
        arr.copy_from_slice(slice);
        Some(arr)
    }
}

impl SaveState {
    pub(crate) fn serialize(&self) -> Vec<u8> {
        let mut s = Ser::new();

        s.bytes(MAGIC);
        s.u8(VERSION);

        // CPU
        let c = &self.cpu;
        s.u16(c.af); s.u16(c.bc); s.u16(c.de); s.u16(c.hl);
        s.u16(c.af_alt); s.u16(c.bc_alt); s.u16(c.de_alt); s.u16(c.hl_alt);
        s.u16(c.pc); s.u16(c.sp); s.u16(c.ix); s.u16(c.iy); s.u16(c.mem_ptr);
        s.u8(c.i); s.u8(c.r);
        s.bool(c.iff1); s.bool(c.iff2); s.bool(c.halted);
        s.u8(c.interrupt_mode); s.u8(c.iff_delay);
        s.u8(c.irq_pending); s.u8(c.nmi_pending); s.u8(c.irq_data);

        // MMU
        let m = &self.mmu;
        s.bytes(&m.ram);
        s.bytes(&m.cart_ram);
        s.u8(m.ram_control);
        s.u32(m.rom_bank_0 as u32);
        s.u32(m.rom_bank_1 as u32);
        s.u32(m.rom_bank_2 as u32);

        // VDP
        let v = &self.vdp;
        s.bytes(&v.vram);
        s.bytes(&v.cram);
        s.bytes(&v.registers);
        s.u16(v.control_word);
        s.bool(v.first_byte_received);
        s.u8(v.mode);
        s.u16(v.address_register);
        s.u8(v.read_buffer);
        s.bool(v.vblank_flag); s.bool(v.line_interrupt_flag);
        s.bool(v.sprite_collision); s.bool(v.sprite_overflow);
        s.u8(v.v_counter); s.u8(v.h_counter);
        s.bool(v.h_latched);
        s.u8(v.latched_h_counter); s.u8(v.latched_v_counter);
        s.u8(v.cram_latch);

        // PSG
        let p = &self.psg;
        for r in &p.registers { s.u16(*r); }
        s.u8(p.latch);
        for c in &p.counters { s.u16(*c); }
        for pol in &p.polarity { s.i8(*pol); }
        s.u16(p.noise_lfsr);
        s.f64(p.clock_frac);
        s.u8(p.stereo);

        // Timing
        let t = &self.timing;
        s.u16(t.vcounter);
        s.i32(t.cycles_accumulator);
        s.u8(t.line_interrupt_counter);
        s.u32(t.frame_cycles);

        s.0
    }

    pub(crate) fn deserialize(data: &[u8]) -> Option<Self> {
        let mut d = De::new(data);

        // Header
        let magic = d.bytes::<4>()?;
        if &magic != MAGIC { return None; }
        if d.u8()? != VERSION { return None; }

        // CPU
        let cpu = CpuState {
            af: d.u16()?, bc: d.u16()?, de: d.u16()?, hl: d.u16()?,
            af_alt: d.u16()?, bc_alt: d.u16()?, de_alt: d.u16()?, hl_alt: d.u16()?,
            pc: d.u16()?, sp: d.u16()?, ix: d.u16()?, iy: d.u16()?, mem_ptr: d.u16()?,
            i: d.u8()?, r: d.u8()?,
            iff1: d.bool()?, iff2: d.bool()?, halted: d.bool()?,
            interrupt_mode: d.u8()?, iff_delay: d.u8()?,
            irq_pending: d.u8()?, nmi_pending: d.u8()?, irq_data: d.u8()?,
        };

        // MMU
        let mmu = MmuState {
            ram:       d.bytes::<8192>()?,
            cart_ram:  d.bytes::<16384>()?,
            ram_control: d.u8()?,
            rom_bank_0: d.u32()? as usize,
            rom_bank_1: d.u32()? as usize,
            rom_bank_2: d.u32()? as usize,
        };

        // VDP
        let vdp = VdpState {
            vram:                d.bytes::<16384>()?,
            cram:                d.bytes::<64>()?,
            registers:           d.bytes::<16>()?,
            control_word:        d.u16()?,
            first_byte_received: d.bool()?,
            mode:                d.u8()?,
            address_register:    d.u16()?,
            read_buffer:         d.u8()?,
            vblank_flag:         d.bool()?,
            line_interrupt_flag: d.bool()?,
            sprite_collision:    d.bool()?,
            sprite_overflow:     d.bool()?,
            v_counter:           d.u8()?,
            h_counter:           d.u8()?,
            h_latched:           d.bool()?,
            latched_h_counter:   d.u8()?,
            latched_v_counter:   d.u8()?,
            cram_latch:          d.u8()?,
        };

        // PSG
        let mut psg_registers = [0u16; 8];
        for r in &mut psg_registers { *r = d.u16()?; }
        let psg_latch = d.u8()?;
        let mut psg_counters = [0u16; 4];
        for c in &mut psg_counters { *c = d.u16()?; }
        let mut psg_polarity = [0i8; 4];
        for p in &mut psg_polarity { *p = d.i8()?; }
        let psg = PsgState {
            registers: psg_registers,
            latch: psg_latch,
            counters: psg_counters,
            polarity: psg_polarity,
            noise_lfsr: d.u16()?,
            clock_frac: d.f64()?,
            stereo: d.u8()?,
        };

        // Timing
        let timing = EmuTimingState {
            vcounter:              d.u16()?,
            cycles_accumulator:    d.i32()?,
            line_interrupt_counter: d.u8()?,
            frame_cycles:          d.u32()?,
        };

        Some(SaveState { cpu, mmu, vdp, psg, timing })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> SaveState {
        SaveState {
            cpu: CpuState {
                af: 0x1234, bc: 0x5678, de: 0x9ABC, hl: 0xDEF0,
                af_alt: 0xAAAA, bc_alt: 0xBBBB, de_alt: 0xCCCC, hl_alt: 0xDDDD,
                pc: 0x0100, sp: 0xFFFE, ix: 0x1111, iy: 0x2222, mem_ptr: 0x3333,
                i: 0x42, r: 0x55,
                iff1: true, iff2: false, halted: false,
                interrupt_mode: 1, iff_delay: 0,
                irq_pending: 0, nmi_pending: 0, irq_data: 0xFF,
            },
            mmu: MmuState {
                ram: [0xAB; 8192],
                cart_ram: [0xCD; 16384],
                ram_control: 0x08,
                rom_bank_0: 0, rom_bank_1: 1, rom_bank_2: 2,
            },
            vdp: VdpState {
                vram: {
                    let mut v = [0u8; 16384];
                    v[0] = 0xDE; v[16383] = 0xAD;
                    v
                },
                cram: [0x3F; 64],
                registers: [0xAA; 16],
                control_word: 0x4001,
                first_byte_received: true,
                mode: 1,
                address_register: 0x1234,
                read_buffer: 0x77,
                vblank_flag: true,
                line_interrupt_flag: false,
                sprite_collision: true,
                sprite_overflow: false,
                v_counter: 0xC0,
                h_counter: 0x55,
                h_latched: true,
                latched_h_counter: 0x42,
                latched_v_counter: 0xBB,
                cram_latch: 0x0F,
            },
            psg: PsgState {
                registers: [100, 200, 300, 400, 0, 1, 2, 3],
                latch: 0x90,
                counters: [10, 20, 30, 40],
                polarity: [1, -1, 1, -1],
                noise_lfsr: 0x8000,
                clock_frac: std::f64::consts::PI,
                stereo: 0xFF,
            },
            timing: EmuTimingState {
                vcounter: 192,
                cycles_accumulator: -10,
                line_interrupt_counter: 7,
                frame_cycles: 59736,
            },
        }
    }

    #[test]
    fn roundtrip_preserves_cpu_fields() {
        let state = sample_state();
        let bytes = state.serialize();
        let r = SaveState::deserialize(&bytes).unwrap();
        assert_eq!(r.cpu.af, 0x1234);
        assert_eq!(r.cpu.bc, 0x5678);
        assert_eq!(r.cpu.de, 0x9ABC);
        assert_eq!(r.cpu.hl, 0xDEF0);
        assert_eq!(r.cpu.pc, 0x0100);
        assert_eq!(r.cpu.sp, 0xFFFE);
        assert_eq!(r.cpu.ix, 0x1111);
        assert_eq!(r.cpu.iy, 0x2222);
        assert_eq!(r.cpu.i, 0x42);
        assert_eq!(r.cpu.r, 0x55);
        assert!(r.cpu.iff1);
        assert!(!r.cpu.iff2);
        assert!(!r.cpu.halted);
        assert_eq!(r.cpu.interrupt_mode, 1);
        assert_eq!(r.cpu.irq_data, 0xFF);
    }

    #[test]
    fn roundtrip_preserves_mmu_fields() {
        let state = sample_state();
        let bytes = state.serialize();
        let r = SaveState::deserialize(&bytes).unwrap();
        assert_eq!(r.mmu.ram[0], 0xAB);
        assert_eq!(r.mmu.ram[8191], 0xAB);
        assert_eq!(r.mmu.cart_ram[0], 0xCD);
        assert_eq!(r.mmu.cart_ram[16383], 0xCD);
        assert_eq!(r.mmu.ram_control, 0x08);
        assert_eq!(r.mmu.rom_bank_0, 0);
        assert_eq!(r.mmu.rom_bank_1, 1);
        assert_eq!(r.mmu.rom_bank_2, 2);
    }

    #[test]
    fn roundtrip_preserves_vdp_fields() {
        let state = sample_state();
        let bytes = state.serialize();
        let r = SaveState::deserialize(&bytes).unwrap();
        assert_eq!(r.vdp.vram[0], 0xDE);
        assert_eq!(r.vdp.vram[16383], 0xAD);
        assert_eq!(r.vdp.cram[0], 0x3F);
        assert_eq!(r.vdp.control_word, 0x4001);
        assert!(r.vdp.first_byte_received);
        assert_eq!(r.vdp.mode, 1);
        assert_eq!(r.vdp.address_register, 0x1234);
        assert_eq!(r.vdp.read_buffer, 0x77);
        assert!(r.vdp.vblank_flag);
        assert!(!r.vdp.line_interrupt_flag);
        assert!(r.vdp.sprite_collision);
        assert!(!r.vdp.sprite_overflow);
        assert_eq!(r.vdp.v_counter, 0xC0);
        assert_eq!(r.vdp.h_counter, 0x55);
        assert!(r.vdp.h_latched);
        assert_eq!(r.vdp.latched_h_counter, 0x42);
        assert_eq!(r.vdp.latched_v_counter, 0xBB);
        assert_eq!(r.vdp.cram_latch, 0x0F);
    }

    #[test]
    fn roundtrip_preserves_psg_fields() {
        let state = sample_state();
        let bytes = state.serialize();
        let r = SaveState::deserialize(&bytes).unwrap();
        assert_eq!(r.psg.registers[0], 100);
        assert_eq!(r.psg.registers[7], 3);
        assert_eq!(r.psg.latch, 0x90);
        assert_eq!(r.psg.counters[0], 10);
        assert_eq!(r.psg.counters[3], 40);
        assert_eq!(r.psg.polarity[0], 1);
        assert_eq!(r.psg.polarity[1], -1);
        assert_eq!(r.psg.noise_lfsr, 0x8000);
        assert!((r.psg.clock_frac - std::f64::consts::PI).abs() < 1e-15);
        assert_eq!(r.psg.stereo, 0xFF);
    }

    #[test]
    fn roundtrip_preserves_timing_fields() {
        let state = sample_state();
        let bytes = state.serialize();
        let r = SaveState::deserialize(&bytes).unwrap();
        assert_eq!(r.timing.vcounter, 192);
        assert_eq!(r.timing.cycles_accumulator, -10);
        assert_eq!(r.timing.line_interrupt_counter, 7);
        assert_eq!(r.timing.frame_cycles, 59736);
    }

    #[test]
    fn serialized_bytes_start_with_magic_and_version() {
        let bytes = sample_state().serialize();
        assert_eq!(&bytes[0..4], b"VSMS");
        assert_eq!(bytes[4], VERSION);
    }

    #[test]
    fn deserialize_bad_magic_returns_none() {
        let mut bytes = sample_state().serialize();
        bytes[0] = b'X';
        assert!(SaveState::deserialize(&bytes).is_none());
    }

    #[test]
    fn deserialize_bad_version_returns_none() {
        let mut bytes = sample_state().serialize();
        bytes[4] = 99;
        assert!(SaveState::deserialize(&bytes).is_none());
    }

    #[test]
    fn deserialize_truncated_returns_none() {
        let bytes = sample_state().serialize();
        assert!(SaveState::deserialize(&bytes[..5]).is_none());
    }

    #[test]
    fn deserialize_empty_returns_none() {
        assert!(SaveState::deserialize(&[]).is_none());
    }
}
