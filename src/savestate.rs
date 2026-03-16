/// Save-state binary format  (magic "VSMS", version 1)
///
/// All integers are little-endian. booleans are 1 byte (0/1).
/// f64 is stored as its IEEE-754 bit pattern (u64 LE).
///
/// The file is written by `SaveState::serialize()` and read by
/// `SaveState::deserialize()`.  Both functions fail fast on any mismatch
/// so a truncated or wrong-version file is simply ignored.
const MAGIC: &[u8; 4] = b"VSMS";
const VERSION: u8 = 1;

// ── Sub-state structs ─────────────────────────────────────────────────────────

pub struct CpuState {
    pub af: u16, pub bc: u16, pub de: u16, pub hl: u16,
    pub af_alt: u16, pub bc_alt: u16, pub de_alt: u16, pub hl_alt: u16,
    pub pc: u16, pub sp: u16, pub ix: u16, pub iy: u16, pub mem_ptr: u16,
    pub i: u8, pub r: u8,
    pub iff1: bool, pub iff2: bool, pub halted: bool,
    pub interrupt_mode: u8, pub iff_delay: u8,
    pub irq_pending: u8, pub nmi_pending: u8, pub irq_data: u8,
}

pub struct MmuState {
    pub ram: [u8; 8192],
    pub cart_ram: [u8; 16384],
    pub ram_control: u8,
    pub rom_bank_0: usize,
    pub rom_bank_1: usize,
    pub rom_bank_2: usize,
}

pub struct VdpState {
    pub vram: [u8; 16384],
    pub cram: [u8; 64],
    pub registers: [u8; 16],
    pub control_word: u16,
    pub first_byte_received: bool,
    pub mode: u8,          // 0=VramRead, 1=VramWrite, 2=CramWrite
    pub address_register: u16,
    pub read_buffer: u8,
    pub vblank_flag: bool,
    pub line_interrupt_flag: bool,
    pub sprite_collision: bool,
    pub sprite_overflow: bool,
    pub v_counter: u8,
    pub h_counter: u8,
    pub h_latched: bool,
    pub latched_h_counter: u8,
    pub latched_v_counter: u8,
    pub cram_latch: u8,
}

pub struct PsgState {
    pub registers: [u16; 8],
    pub latch: u8,
    pub counters: [u16; 4],
    pub polarity: [i8; 4],
    pub noise_lfsr: u16,
    pub clock_frac: f64,
    pub stereo: u8,
}

pub struct EmuTimingState {
    pub vcounter: u16,
    pub cycles_accumulator: i32,
    pub line_interrupt_counter: u8,
    pub frame_cycles: u32,
}

pub struct SaveState {
    pub cpu:    CpuState,
    pub mmu:    MmuState,
    pub vdp:    VdpState,
    pub psg:    PsgState,
    pub timing: EmuTimingState,
}

// ── Binary serializer ─────────────────────────────────────────────────────────

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

// ── Binary deserializer ───────────────────────────────────────────────────────

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

// ── SaveState serialization ───────────────────────────────────────────────────

impl SaveState {
    pub fn serialize(&self) -> Vec<u8> {
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

    pub fn deserialize(data: &[u8]) -> Option<Self> {
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
