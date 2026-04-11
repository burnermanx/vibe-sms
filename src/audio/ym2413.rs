/// YM2413 (OPLL) Pure Rust implementation — adapted from Mitsutaka Okazaki's emu2413
const DP_BITS: u32 = 19;
const PG_BITS: u32 = 10;
const PG_WIDTH: u32 = 1 << PG_BITS;
const EG_BITS: u32 = 7;

const LW: usize = 16;
const SINC_RESO: usize = 256;
const SINC_AMP_BITS: usize = 12;

const ML_TABLE: [u32; 16] = [1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 20, 24, 24, 30, 30];

const OPLL_TONE_NUM: usize = 3;
const DEFAULT_INST: [[[u8; 8]; 19]; OPLL_TONE_NUM] = [
    [ // Original YM2413
        [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00], // 0: User
        [0x71,0x61,0x1e,0x17,0xd0,0x78,0x00,0x17], // 1: Violin
        [0x13,0x41,0x1a,0x0d,0xd8,0xf7,0x23,0x13], // 2: Guitar
        [0x13,0x01,0x99,0x00,0xf2,0xc4,0x21,0x23], // 3: Piano
        [0x11,0x61,0x0e,0x07,0x8d,0x64,0x70,0x27], // 4: Flute
        [0x32,0x21,0x1e,0x06,0xe1,0x76,0x01,0x28], // 5: Clarinet
        [0x31,0x22,0x16,0x05,0xe0,0x71,0x00,0x18], // 6: Oboe
        [0x21,0x61,0x1d,0x07,0x82,0x81,0x11,0x07], // 7: Trumpet
        [0x33,0x21,0x2d,0x13,0xb0,0x70,0x00,0x07], // 8: Organ
        [0x61,0x61,0x1b,0x06,0x64,0x65,0x10,0x17], // 9: Horn
        [0x41,0x61,0x0b,0x18,0x85,0xf0,0x81,0x07], // A: Synthesizer
        [0x33,0x01,0x83,0x11,0xea,0xef,0x10,0x04], // B: Harpsichord
        [0x17,0xc1,0x24,0x07,0xf8,0xf8,0x22,0x12], // C: Vibraphone
        [0x61,0x50,0x0c,0x05,0xd2,0xf5,0x40,0x42], // D: Synthsizer Bass
        [0x01,0x01,0x55,0x03,0xe9,0x90,0x03,0x02], // E: Acoustic Bass
        [0x41,0x41,0x89,0x03,0xf1,0xe4,0xc0,0x13], // F: Electric Guitar
        [0x01,0x01,0x18,0x0f,0xdf,0xf8,0x6a,0x6d], // R: Bass Drum 
        [0x01,0x01,0x00,0x00,0xc8,0xd8,0xa7,0x68], // R: High-Hat/Snare 
        [0x05,0x01,0x00,0x00,0xf8,0xaa,0x59,0x55], // R: Tom-tom/Cymbal 
    ],
    [ // VRC7 Nuke.YKT
        [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00],
        [0x03,0x21,0x05,0x06,0xe8,0x81,0x42,0x27],
        [0x13,0x41,0x14,0x0d,0xd8,0xf6,0x23,0x12],
        [0x11,0x11,0x08,0x08,0xfa,0xb2,0x20,0x12],
        [0x31,0x61,0x0c,0x07,0xa8,0x64,0x61,0x27],
        [0x32,0x21,0x1e,0x06,0xe1,0x76,0x01,0x28],
        [0x02,0x01,0x06,0x00,0xa3,0xe2,0xf4,0xf4],
        [0x21,0x61,0x1d,0x07,0x82,0x81,0x11,0x07],
        [0x23,0x21,0x22,0x17,0xa2,0x72,0x01,0x17],
        [0x35,0x11,0x25,0x00,0x40,0x73,0x72,0x01],
        [0xb5,0x01,0x0f,0x0F,0xa8,0xa5,0x51,0x02],
        [0x17,0xc1,0x24,0x07,0xf8,0xf8,0x22,0x12],
        [0x71,0x23,0x11,0x06,0x65,0x74,0x18,0x16],
        [0x01,0x02,0xd3,0x05,0xc9,0x95,0x03,0x02],
        [0x61,0x63,0x0c,0x00,0x94,0xC0,0x33,0xf6],
        [0x21,0x72,0x0d,0x00,0xc1,0xd5,0x56,0x06],
        [0x01,0x01,0x18,0x0f,0xdf,0xf8,0x6a,0x6d],
        [0x01,0x01,0x00,0x00,0xc8,0xd8,0xa7,0x68],
        [0x05,0x01,0x00,0x00,0xf8,0xaa,0x59,0x55],
    ],
    [ // YMF281B
        [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00],
        [0x62,0x21,0x1a,0x07,0xf0,0x6f,0x00,0x16],
        [0x40,0x10,0x45,0x00,0xf6,0x83,0x73,0x63],
        [0x13,0x01,0x99,0x00,0xf2,0xc3,0x21,0x23],
        [0x01,0x61,0x0b,0x0f,0xf9,0x64,0x70,0x17],
        [0x32,0x21,0x1e,0x06,0xe1,0x76,0x01,0x28],
        [0x60,0x01,0x82,0x0e,0xf9,0x61,0x20,0x27],
        [0x21,0x61,0x1c,0x07,0x84,0x81,0x11,0x07],
        [0x37,0x32,0xc9,0x01,0x66,0x64,0x40,0x28],
        [0x01,0x21,0x07,0x03,0xa5,0x71,0x51,0x07],
        [0x06,0x01,0x5e,0x07,0xf3,0xf3,0xf6,0x13],
        [0x00,0x00,0x18,0x06,0xf5,0xf3,0x20,0x23],
        [0x17,0xc1,0x24,0x07,0xf8,0xf8,0x22,0x12],
        [0x35,0x64,0x00,0x00,0xff,0xf3,0x77,0xf5],
        [0x11,0x31,0x00,0x07,0xdd,0xf3,0xff,0xfb],
        [0x3a,0x21,0x00,0x07,0x80,0x84,0x0f,0xf5],
        [0x01,0x01,0x18,0x0f,0xdf,0xf8,0x6a,0x6d],
        [0x01,0x01,0x00,0x00,0xc8,0xd8,0xa7,0x68],
        [0x05,0x01,0x00,0x00,0xf8,0xaa,0x59,0x55],
    ]
];

const EXP_TABLE: [u16; 256] = [
0,    3,    6,    8,    11,   14,   17,   20,   22,   25,   28,   31,   34,   37,   40,   42,
45,   48,   51,   54,   57,   60,   63,   66,   69,   72,   75,   78,   81,   84,   87,   90,
93,   96,   99,   102,  105,  108,  111,  114,  117,  120,  123,  126,  130,  133,  136,  139,
142,  145,  148,  152,  155,  158,  161,  164,  168,  171,  174,  177,  181,  184,  187,  190,
194,  197,  200,  204,  207,  210,  214,  217,  220,  224,  227,  231,  234,  237,  241,  244,
248,  251,  255,  258,  262,  265,  268,  272,  276,  279,  283,  286,  290,  293,  297,  300,
304,  308,  311,  315,  318,  322,  326,  329,  333,  337,  340,  344,  348,  352,  355,  359,
363,  367,  370,  374,  378,  382,  385,  389,  393,  397,  401,  405,  409,  412,  416,  420,
424,  428,  432,  436,  440,  444,  448,  452,  456,  460,  464,  468,  472,  476,  480,  484,
488,  492,  496,  501,  505,  509,  513,  517,  521,  526,  530,  534,  538,  542,  547,  551,
555,  560,  564,  568,  572,  577,  581,  585,  590,  594,  599,  603,  607,  612,  616,  621,
625,  630,  634,  639,  643,  648,  652,  657,  661,  666,  670,  675,  680,  684,  689,  693,
698,  703,  708,  712,  717,  722,  726,  731,  736,  741,  745,  750,  755,  760,  765,  770,
774,  779,  784,  789,  794,  799,  804,  809,  814,  819,  824,  829,  834,  839,  844,  849,
854,  859,  864,  869,  874,  880,  885,  890,  895,  900,  906,  911,  916,  921,  927,  932,
937,  942,  948,  953,  959,  964,  969,  975,  980,  986,  991,  996, 1002, 1007, 1013, 1018
];

const FULLSIN_TABLE_RAW: [u16; 256] = [
2137, 1731, 1543, 1419, 1326, 1252, 1190, 1137, 1091, 1050, 1013, 979,  949,  920,  894,  869, 
846,  825,  804,  785,  767,  749,  732,  717,  701,  687,  672,  659,  646,  633,  621,  609, 
598,  587,  576,  566,  556,  546,  536,  527,  518,  509,  501,  492,  484,  476,  468,  461,
453,  446,  439,  432,  425,  418,  411,  405,  399,  392,  386,  380,  375,  369,  363,  358,  
352,  347,  341,  336,  331,  326,  321,  316,  311,  307,  302,  297,  293,  289,  284,  280,
276,  271,  267,  263,  259,  255,  251,  248,  244,  240,  236,  233,  229,  226,  222,  219, 
215,  212,  209,  205,  202,  199,  196,  193,  190,  187,  184,  181,  178,  175,  172,  169, 
167,  164,  161,  159,  156,  153,  151,  148,  146,  143,  141,  138,  136,  134,  131,  129,  
127,  125,  122,  120,  118,  116,  114,  112,  110,  108,  106,  104,  102,  100,  98,   96,   
94,   92,   91,   89,   87,   85,   83,   82,   80,   78,   77,   75,   74,   72,   70,   69,
67,   66,   64,   63,   62,   60,   59,   57,   56,   55,   53,   52,   51,   49,   48,   47,  
46,   45,   43,   42,   41,   40,   39,   38,   37,   36,   35,   34,   33,   32,   31,   30,  
29,   28,   27,   26,   25,   24,   23,   23,   22,   21,   20,   20,   19,   18,   17,   17,   
16,   15,   15,   14,   13,   13,   12,   12,   11,   10,   10,   9,    9,    8,    8,    7,    
7,    7,    6,    6,    5,    5,    5,    4,    4,    4,    3,    3,    3,    2,    2,    2,
2,    1,    1,    1,    1,    1,    1,    1,    0,    0,    0,    0,    0,    0,    0,    0,
];

const AM_TABLE: [u8; 210] = [
0,  0,  0,  0,  0,  0,  0,  0,  1,  1,  1,  1,  1,  1,  1,  1, 
2,  2,  2,  2,  2,  2,  2,  2,  3,  3,  3,  3,  3,  3,  3,  3, 
4,  4,  4,  4,  4,  4,  4,  4,  5,  5,  5,  5,  5,  5,  5,  5, 
6,  6,  6,  6,  6,  6,  6,  6,  7,  7,  7,  7,  7,  7,  7,  7, 
8,  8,  8,  8,  8,  8,  8,  8,  9,  9,  9,  9,  9,  9,  9,  9, 
10, 10, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11, 11, 11, 11,
12, 12, 12, 12, 12, 12, 12, 12,                                
13, 13, 13,                                                    
12, 12, 12, 12, 12, 12, 12, 12,                                
11, 11, 11, 11, 11, 11, 11, 11, 10, 10, 10, 10, 10, 10, 10, 10,
9,  9,  9,  9,  9,  9,  9,  9,  8,  8,  8,  8,  8,  8,  8,  8, 
7,  7,  7,  7,  7,  7,  7,  7,  6,  6,  6,  6,  6,  6,  6,  6, 
5,  5,  5,  5,  5,  5,  5,  5,  4,  4,  4,  4,  4,  4,  4,  4, 
3,  3,  3,  3,  3,  3,  3,  3,  2,  2,  2,  2,  2,  2,  2,  2, 
1,  1,  1,  1,  1,  1,  1,  1,  0,  0,  0,  0,  0,  0,  0
];

const EG_STEP_TABLES: [[u8; 8]; 4] = [
    [0, 1, 0, 1, 0, 1, 0, 1],
    [0, 1, 0, 1, 1, 1, 0, 1],
    [0, 1, 1, 1, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 1, 1, 1],
];

const KL_TABLE: [f64; 16] = [
    0.0, 18.0, 24.0, 27.75, 30.0, 32.25, 33.75, 35.25, 36.0, 37.5, 38.25, 39.0, 39.75, 40.5, 41.25, 42.0
];

const PM_TABLE: [[i8; 8]; 8] = [
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, -1, 0],
    [0, 1, 2, 1, 0, -1, -2, -1],
    [0, 1, 3, 1, 0, -1, -3, -1],
    [0, 2, 4, 2, 0, -2, -4, -2],
    [0, 2, 5, 2, 0, -2, -5, -2],
    [0, 3, 6, 3, 0, -3, -6, -3],
    [0, 3, 7, 3, 0, -3, -7, -3],
];

// EG States
const EG_ATTACK: u8 = 1;
const EG_DECAY: u8 = 2;
const EG_SUSTAIN: u8 = 3;
const EG_RELEASE: u8 = 4;
const EG_DAMP: u8 = 5;

const EG_MUTE: u32 = (1 << EG_BITS) - 1;
const EG_MAX: u32 = EG_MUTE - 4;
const DAMPER_RATE: u32 = 12;

// Slot update request flags (must match emu2413 C reference)
const UPDATE_WS: u32 = 1;
const UPDATE_TLL: u32 = 2;
const UPDATE_RKS: u32 = 4;
const UPDATE_EG: u32 = 8;
const UPDATE_ALL: u32 = 255;


const SLOT_HH: usize = 14;
const SLOT_SD: usize = 15;
const SLOT_TOM: usize = 16;
const SLOT_CYM: usize = 17;
fn lookup_exp_table(i: u16) -> i16 {
    let t = (EXP_TABLE[((i & 0xff) ^ 0xff) as usize] + 1024) as i16;
    let shift = (i & 0x7f00) >> 8;
    let res = if shift > 15 { 0 } else { t >> shift };
    let val = if (i & 0x8000) != 0 { !res } else { res };
    val << 1
}

#[derive(Clone, Copy, Default)]
struct Patch {
    tl: u32, fb: u32, eg: u32, ml: u32, ar: u32,
    dr: u32, sl: u32, rr: u32, kr: u32, kl: u32,
    am: u32, pm: u32, ws: u32,
}

#[derive(Clone, Default)]
struct Slot {
    #[allow(dead_code)]
    number: u8,
    slot_type: u8,
    patch: Patch,
    output: [i32; 2],

    // Phase Generator
    pg_phase: u32,
    pg_out: u32,
    pg_keep: u8,
    blk_fnum: u16,
    fnum: u16,
    blk: u8,

    // Envelope Generator
    eg_state: u8,
    volume: i32,
    key_flag: u8,
    sus_flag: u8,
    tll: u16,
    rks: u8,
    eg_rate_h: u8,
    eg_rate_l: u8,
    eg_shift: u32,
    eg_out: u32,
    update_requests: u32,
}

pub(crate) struct RateConv {
    timer: f64,
    f_ratio: f64,
    buf: [i16; LW],
    sinc_table: Vec<i16>,
}

impl RateConv {
    pub fn new(f_inp: f64, f_out: f64) -> Self {
        let f_ratio = f_inp / f_out;
        
        let mut sinc_table = Vec::with_capacity(SINC_RESO * LW / 2);
        for i in 0..(SINC_RESO * LW / 2) {
            let x = i as f64 / SINC_RESO as f64;
            if f_out < f_inp {
                // Downsampling
                sinc_table.push(((1 << SINC_AMP_BITS) as f64 * Self::windowed_sinc(x / f_ratio) / f_ratio) as i16);
            } else {
                // Upsampling
                sinc_table.push(((1 << SINC_AMP_BITS) as f64 * Self::windowed_sinc(x)) as i16);
            }
        }
        
        Self {
            timer: 0.0,
            f_ratio,
            buf: [0; LW],
            sinc_table,
        }
    }
    
    fn blackman(x: f64) -> f64 {
        0.42 - 0.5 * (core::f64::consts::PI * 2.0 * x).cos() + 0.08 * (core::f64::consts::PI * 4.0 * x).cos()
    }
    
    fn sinc(x: f64) -> f64 {
        if x == 0.0 { 1.0 } else { (core::f64::consts::PI * x).sin() / (core::f64::consts::PI * x) }
    }
    
    fn windowed_sinc(x: f64) -> f64 {
        Self::blackman(0.5 + 0.5 * x / (LW as f64 / 2.0)) * Self::sinc(x)
    }
    
    fn lookup_sinc_table(&self, x: f64) -> i16 {
        let mut index = (x * SINC_RESO as f64) as isize;
        if index < 0 { index = -index; }
        let max_idx = (SINC_RESO * LW / 2 - 1) as isize;
        self.sinc_table[index.min(max_idx) as usize]
    }
    
    pub fn put_data(&mut self, data: i16) {
        self.buf.copy_within(1..LW, 0);
        self.buf[LW - 1] = data;
    }
    
    pub fn get_data(&mut self) -> i16 {
        let mut sum: i32 = 0;
        self.timer += self.f_ratio;
        let dn = self.timer - self.timer.floor();
        self.timer = dn;
        
        for k in 0..LW {
            let x = (k as f64 - (LW as f64 / 2.0 - 1.0)) - dn;
            sum += self.buf[k] as i32 * self.lookup_sinc_table(x) as i32;
        }
        
        (sum >> SINC_AMP_BITS) as i16
    }
}

pub(crate) struct Ym2413 {
    out_time: f64,
    out_step: f64,
    inp_step: f64,

    adr: u32,
    reg: [u8; 0x40],
    
    // Global counters
    eg_counter: u32,
    pm_phase: u32,
    am_phase: i32,
    lfo_am: u8,
    noise: u32,
    short_noise: u8,
    
    // Voices
    patch_number: [i32; 9],
    slot: [Slot; 18],
    patch: [Patch; 19 * 2], // 19 instruments * 2 operators
    
    // Outputs
    ch_out: [i16; 14],
    conv: RateConv,

    // Runtime Generated Tables
    tll_table: [[[u32; 4]; 64]; 128],
    rks_table: [[u8; 2]; 16],
    fullsin_table: [u16; PG_WIDTH as usize],
    halfsin_table: [u16; PG_WIDTH as usize],
    
    // Misc limits
    rhythm_mode: u8,
    slot_key_status: u32,
}

impl Ym2413 {
    pub fn new(clock: u32, sample_rate: u32) -> Self {
        let f_inp = clock as f64 / 72.0;
        let f_out = sample_rate as f64;

        let mut opll = Self {
            out_time: 0.0,
            out_step: f_out,
            inp_step: f_inp,
            adr: 0,
            reg: [0; 0x40],
            eg_counter: 0,
            pm_phase: 0,
            am_phase: 0,
            lfo_am: 0,
            noise: 1,
            short_noise: 0,
            patch_number: [0; 9],
            slot: core::array::from_fn(|i| Slot {
                number: i as u8,
                slot_type: (i % 2) as u8,
                ..Slot::default()
            }),
            patch: core::array::from_fn(|_| Patch::default()),
            ch_out: [0; 14],
            conv: RateConv::new(f_inp, f_out),
            tll_table: [[[0; 4]; 64]; 128],
            rks_table: [[0; 2]; 16],
            fullsin_table: [0; PG_WIDTH as usize],
            halfsin_table: [0; PG_WIDTH as usize],
            rhythm_mode: 0,
            slot_key_status: 0,
        };

        opll.make_tll_table();
        opll.make_rks_table();
        opll.make_sin_table();
        opll.apply_default_patches();

        opll.reset();

        opll
    }

    pub fn reset(&mut self) {
        self.adr = 0;
        self.pm_phase = 0;
        self.am_phase = 0;
        self.noise = 1;
        self.rhythm_mode = 0;
        self.slot_key_status = 0;
        self.eg_counter = 0;

        for i in 0..18 {
            self.reset_slot(i);
        }

        for i in 0..9 {
            self.set_patch(i, 0);
        }

        for i in 0..0x40 {
            self.reg[i] = 0;
            self.adr = i as u32; // To simulate write
            self.update_reg();
        }

        self.ch_out = [0; 14];
    }

    fn reset_slot(&mut self, i: usize) {
        let slot = &mut self.slot[i];
        slot.pg_phase = 0;
        slot.pg_out = 0;
        slot.pg_keep = 0;
        slot.eg_state = EG_RELEASE;
        slot.eg_out = EG_MUTE;
        slot.update_requests = 0;
        slot.sus_flag = 0;
    }

    fn make_tll_table(&mut self) {
        for (fnum, &kl_base) in KL_TABLE.iter().enumerate() {
            for block in 0..8 {
                for tl in 0..64 {
                    for kl in 0..4 {
                        let idx = (block << 4) | fnum;
                        if kl == 0 {
                            self.tll_table[idx][tl][kl] = (tl as u32) << 1;
                        } else {
                            let tmp = kl_base - (3.0 * (7 - block) as f64) * 2.0;
                            if tmp <= 0.0 {
                                self.tll_table[idx][tl][kl] = (tl as u32) << 1;
                            } else {
                                let shifted = (tmp as u32) >> (3 - kl);
                                let eg_step_scaled = (shifted as f64 / 0.375) as u32; // EG_STEP = 0.375 basically
                                self.tll_table[idx][tl][kl] = eg_step_scaled + ((tl as u32) << 1);
                            }
                        }
                    }
                }
            }
        }
    }

    fn make_rks_table(&mut self) {
        for fnum8 in 0..2 {
            for block in 0..8 {
                let idx = (block << 1) | fnum8;
                self.rks_table[idx][1] = ((block << 1) + fnum8) as u8;
                self.rks_table[idx][0] = (block >> 1) as u8;
            }
        }
    }

    fn make_sin_table(&mut self) {
        self.fullsin_table[..256].copy_from_slice(&FULLSIN_TABLE_RAW);
        for x in 0..(PG_WIDTH / 4) as usize {
            self.fullsin_table[((PG_WIDTH / 4) as usize) + x] = self.fullsin_table[((PG_WIDTH / 4) as usize) - x - 1];
        }
        for x in 0..(PG_WIDTH / 2) as usize {
            self.fullsin_table[((PG_WIDTH / 2) as usize) + x] = 0x8000 | self.fullsin_table[x];
        }
        for x in 0..(PG_WIDTH / 2) as usize {
            self.halfsin_table[x] = self.fullsin_table[x];
        }
        for x in ((PG_WIDTH / 2) as usize)..(PG_WIDTH as usize) {
            self.halfsin_table[x] = 0xfff;
        }
    }

    fn apply_default_patches(&mut self) {
        // We will default to the standard OPLL (YM2413) tones in DEFAULT_INST[0].
        // 19 instruments total. (User is 0, Melodic is 1-15, Rhythm is 16-18)
        for (i, data) in DEFAULT_INST[0].iter().enumerate() {
            // Modulator Patch
            let pt_mod = &mut self.patch[i * 2];
            pt_mod.am = ((data[0] >> 7) & 1) as u32;
            pt_mod.pm = ((data[0] >> 6) & 1) as u32;
            pt_mod.eg = ((data[0] >> 5) & 1) as u32;
            pt_mod.kr = ((data[0] >> 4) & 1) as u32;
            pt_mod.ml = (data[0] & 15) as u32;
            pt_mod.kl = ((data[2] >> 6) & 3) as u32;
            pt_mod.tl = (data[2] & 63) as u32;
            pt_mod.ar = ((data[4] >> 4) & 15) as u32;
            pt_mod.dr = (data[4] & 15) as u32;
            pt_mod.sl = ((data[6] >> 4) & 15) as u32;
            pt_mod.rr = (data[6] & 15) as u32;
            pt_mod.ws = ((data[3] >> 3) & 1) as u32; // Modulator waveform shape

            // Carrier Patch
            let pt_car = &mut self.patch[i * 2 + 1];
            pt_car.am = ((data[1] >> 7) & 1) as u32;
            pt_car.pm = ((data[1] >> 6) & 1) as u32;
            pt_car.eg = ((data[1] >> 5) & 1) as u32;
            pt_car.kr = ((data[1] >> 4) & 1) as u32;
            pt_car.ml = (data[1] & 15) as u32;
            pt_car.kl = ((data[3] >> 6) & 3) as u32;
            pt_car.ws = ((data[3] >> 4) & 1) as u32; // Carrier waveform shape
            pt_car.fb = (data[3] & 7) as u32;
            pt_car.ar = ((data[5] >> 4) & 15) as u32;
            pt_car.dr = (data[5] & 15) as u32;
            pt_car.sl = ((data[7] >> 4) & 15) as u32;
            pt_car.rr = (data[7] & 15) as u32;
        }
    }

    // --- ENVELOPE GENERATOR ---

    fn lookup_attack_step(slot: &Slot, counter: u32) -> u8 {
        match slot.eg_rate_h {
            12 => {
                let index = ((counter & 0xc) >> 1) as usize;
                4 - EG_STEP_TABLES[slot.eg_rate_l as usize][index]
            }
            13 => {
                let index = ((counter & 0xc) >> 1) as usize;
                3 - EG_STEP_TABLES[slot.eg_rate_l as usize][index]
            }
            14 => {
                let index = ((counter & 0xc) >> 1) as usize;
                2 - EG_STEP_TABLES[slot.eg_rate_l as usize][index]
            }
            0 | 15 => 0,
            _ => {
                let index = (counter >> slot.eg_shift) as usize;
                if EG_STEP_TABLES[slot.eg_rate_l as usize][index & 7] != 0 {
                    4
                } else {
                    0
                }
            }
        }
    }

    fn lookup_decay_step(slot: &Slot, counter: u32) -> u8 {
        match slot.eg_rate_h {
            0 => 0,
            13 => {
                let index = (((counter & 0xc) >> 1) | (counter & 1)) as usize;
                EG_STEP_TABLES[slot.eg_rate_l as usize][index]
            }
            14 => {
                let index = ((counter & 0xc) >> 1) as usize;
                EG_STEP_TABLES[slot.eg_rate_l as usize][index] + 1
            }
            15 => 2,
            _ => {
                let index = (counter >> slot.eg_shift) as usize;
                EG_STEP_TABLES[slot.eg_rate_l as usize][index & 7]
            }
        }
    }

    fn start_envelope(slot: &mut Slot) {
        let max_rate = 15.min(slot.patch.ar + (slot.rks as u32 >> 2));
        if max_rate == 15 {
            slot.eg_state = EG_DECAY;
            slot.eg_out = 0;
        } else {
            slot.eg_state = EG_ATTACK;
        }
        slot.update_requests |= UPDATE_EG;
    }

    fn calc_envelope(&mut self, slot_idx: usize, buddy_idx: Option<usize>, test: bool) {
        let slot = &mut self.slot[slot_idx];
        let mask = (1 << slot.eg_shift) - 1;
        let eg_counter = self.eg_counter;

        if slot.eg_state == EG_ATTACK {
            if slot.eg_out > 0 && slot.eg_rate_h > 0 && (eg_counter & mask & (!3)) == 0 {
                let s = Self::lookup_attack_step(slot, eg_counter);
                if s > 0 {
                    slot.eg_out = 0.max(slot.eg_out as i32 - (slot.eg_out >> s) as i32 - 1) as u32;
                }
            }
        } else if slot.eg_rate_h > 0 && (eg_counter & mask) == 0 {
            let step = Self::lookup_decay_step(slot, eg_counter);
            slot.eg_out = EG_MUTE.min(slot.eg_out + step as u32);
        }

        match slot.eg_state {
            EG_DAMP => {
                if slot.eg_out >= EG_MAX && (eg_counter & mask) == 0 {
                    Self::start_envelope(slot);
                    if (slot.slot_type & 1) != 0 && slot.pg_keep == 0 {
                        slot.pg_phase = 0;
                        // Buddy reset happens below outside borrow
                    }
                }
            }
            EG_ATTACK => {
                if slot.eg_out == 0 {
                    slot.eg_state = EG_DECAY;
                    slot.update_requests |= 4;
                }
            }
            EG_DECAY => {
                if (slot.eg_out >> 3) == slot.patch.sl {
                    slot.eg_state = EG_SUSTAIN;
                    slot.update_requests |= 4;
                }
            }
            _ => {}
        }

        if test {
            slot.eg_out = 0;
        }

        // Handle buddy phase reset if DAMP transitioned
        if let Some(b_idx) = buddy_idx {
            // Did DAMP just transition to ATTACK and reset our phase?
            if slot.eg_state == EG_ATTACK && slot.eg_out == 0 { // Just triggered attack
                 if self.slot[b_idx].pg_keep == 0 {
                     self.slot[b_idx].pg_phase = 0;
                 }
            }
        }
    }

    // --- PHASE GENERATOR ---

    fn calc_phase(&mut self, slot_idx: usize, pm_phase: u32, reset: bool) {
        let slot = &mut self.slot[slot_idx];
        let pm = if slot.patch.pm != 0 {
            PM_TABLE[((slot.fnum >> 6) & 7) as usize][((pm_phase >> 10) & 7) as usize] as i32
        } else {
            0
        };

        if reset {
            slot.pg_phase = 0;
        }

        let mut freq = (slot.fnum & 0x1ff) as i32 * 2 + pm;
        freq *= ML_TABLE[slot.patch.ml as usize] as i32;
        let shift = slot.blk;
        let inc = (freq << shift) >> 2;

        slot.pg_phase = slot.pg_phase.wrapping_add(inc as u32);
        slot.pg_phase &= (1 << DP_BITS) - 1; // DP_WIDTH - 1
        slot.pg_out = slot.pg_phase >> (DP_BITS - PG_BITS); // DP_BASE_BITS
    }
    fn commit_slot_update(&mut self, idx: usize) {
        let slot = &mut self.slot[idx];

        if (slot.update_requests & UPDATE_TLL) != 0 {
            if (slot.slot_type & 1) == 0 {
                slot.tll = self.tll_table[(slot.blk_fnum >> 5) as usize][slot.patch.tl as usize][slot.patch.kl as usize] as u16;
            } else {
                slot.tll = self.tll_table[(slot.blk_fnum >> 5) as usize][slot.volume as usize][slot.patch.kl as usize] as u16;
            }
        }

        if (slot.update_requests & UPDATE_RKS) != 0 {
            slot.rks = self.rks_table[(slot.blk_fnum >> 8) as usize][slot.patch.kr as usize];
        }

        if (slot.update_requests & (UPDATE_RKS | UPDATE_EG)) != 0 {
            let p_rate = if (slot.slot_type & 1) == 0 && slot.key_flag == 0 {
                0
            } else {
                match slot.eg_state {
                    EG_ATTACK => slot.patch.ar as u8,
                    EG_DECAY => slot.patch.dr as u8,
                    EG_SUSTAIN => if slot.patch.eg != 0 { 0 } else { slot.patch.rr as u8 },
                    EG_RELEASE => {
                        if slot.sus_flag != 0 {
                            5
                        } else if slot.patch.eg != 0 {
                            slot.patch.rr as u8
                        } else {
                            7
                        }
                    }
                    EG_DAMP => DAMPER_RATE as u8,
                    _ => 0,
                }
            };

            if p_rate == 0 {
                slot.eg_shift = 0;
                slot.eg_rate_h = 0;
                slot.eg_rate_l = 0;
            } else {
                slot.eg_rate_h = 15.min(p_rate + (slot.rks >> 2));
                slot.eg_rate_l = slot.rks & 3;
                if slot.eg_state == EG_ATTACK {
                    slot.eg_shift = if 0 < slot.eg_rate_h && slot.eg_rate_h < 12 { 13 - slot.eg_rate_h as u32 } else { 0 };
                } else {
                    slot.eg_shift = if slot.eg_rate_h < 13 { 13 - slot.eg_rate_h as u32 } else { 0 };
                }
            }
        }
        
        slot.update_requests = 0;
    }

    fn update_ampm(&mut self) {
        self.pm_phase += 1;
        self.am_phase += 1;
        self.lfo_am = AM_TABLE[((self.am_phase >> 6) as usize) % AM_TABLE.len()];
    }


    fn update_short_noise(&mut self) {
        let pg_hh = self.slot[SLOT_HH].pg_out;
        let pg_cym = self.slot[SLOT_CYM].pg_out;
        
        // 10-bit Phase generators
        let h_bit2 = (pg_hh >> (10 - 8)) & 1;
        let h_bit7 = (pg_hh >> (10 - 3)) & 1;
        let h_bit3 = (pg_hh >> (10 - 7)) & 1;

        let c_bit3 = (pg_cym >> (10 - 7)) & 1;
        let c_bit5 = (pg_cym >> (10 - 5)) & 1;

        self.short_noise = ((h_bit2 ^ h_bit7) | (h_bit3 ^ c_bit5) | (c_bit3 ^ c_bit5)) as u8;
    }

    fn update_slots(&mut self) {
        self.eg_counter = self.eg_counter.wrapping_add(1);

        for i in 0..18 {
            if self.slot[i].update_requests != 0 {
                self.commit_slot_update(i);
            }
        }
        
        for i in 0..18 {
            let buddy_idx = if self.slot[i].slot_type == 0 { Some(i + 1) } else { Some(i.saturating_sub(1)) };
            self.calc_envelope(i, buddy_idx, false);
            let pm_phase = self.pm_phase;
            self.calc_phase(i, pm_phase, false);
        }
    }

    fn calc_slot_mod(&mut self, ch: usize) -> i16 {
        let slot_idx = ch * 2;
        let slot = &mut self.slot[slot_idx];

        let fm = if slot.patch.fb > 0 {
            (slot.output[1] + slot.output[0]) >> (9 - slot.patch.fb)
        } else {
            0
        };

        let am = if slot.patch.am != 0 { self.lfo_am } else { 0 };

        slot.output[1] = slot.output[0];

        let phase = ((slot.pg_out as i32 + fm) & (PG_WIDTH as i32 - 1)) as usize;
        let wave_table = if slot.patch.ws != 0 { &self.halfsin_table } else { &self.fullsin_table };
        
        let h = wave_table[phase];
        if slot.eg_out >= EG_MAX {
            slot.output[0] = 0;
            return 0;
        }
        let att = EG_MUTE.min(slot.eg_out + slot.tll as u32 + am as u32) << 4;
        slot.output[0] = lookup_exp_table(h + att as u16) as i32;

        slot.output[0] as i16
    }

    fn calc_slot_car(&mut self, ch: usize, fm: i16) -> i16 {
        let slot_idx = ch * 2 + 1;
        let slot = &mut self.slot[slot_idx];

        let am = if slot.patch.am != 0 { self.lfo_am } else { 0 };

        slot.output[1] = slot.output[0];
        
        let phase = ((slot.pg_out as i32 + 2 * (fm >> 1) as i32) & (PG_WIDTH as i32 - 1)) as usize;
        let wave_table = if slot.patch.ws != 0 { &self.halfsin_table } else { &self.fullsin_table };

        let h = wave_table[phase];
        if slot.eg_out >= EG_MAX {
            slot.output[0] = 0;
            return 0;
        }
        let att = EG_MUTE.min(slot.eg_out + slot.tll as u32 + am as u32) << 4;
        slot.output[0] = lookup_exp_table(h + att as u16) as i32;

        slot.output[0] as i16
    }
    
    fn calc_slot_tom(&mut self) -> i16 {
        let slot = &mut self.slot[SLOT_TOM];
        let wave_table = if slot.patch.ws != 0 { &self.halfsin_table } else { &self.fullsin_table };
        let h = wave_table[slot.pg_out as usize];
        if slot.eg_out >= EG_MAX { return 0; }
        let att = EG_MUTE.min(slot.eg_out + slot.tll as u32) << 4;
        lookup_exp_table(h + att as u16)
    }

    fn calc_slot_snare(&mut self) -> i16 {
        let slot = &mut self.slot[SLOT_SD];
        let wave_table = if slot.patch.ws != 0 { &self.halfsin_table } else { &self.fullsin_table };
        let phase = if (slot.pg_out & (1 << (PG_BITS - 2))) != 0 {
            if (self.noise & 1) != 0 { 0x300 } else { 0x200 }
        } else if (self.noise & 1) != 0 { 0x0 } else { 0x100 };
        let h = wave_table[phase];
        if slot.eg_out >= EG_MAX { return 0; }
        let att = EG_MUTE.min(slot.eg_out + slot.tll as u32) << 4;
        lookup_exp_table(h + att as u16)
    }

    fn calc_slot_cym(&mut self) -> i16 {
        let slot = &mut self.slot[SLOT_CYM];
        let wave_table = if slot.patch.ws != 0 { &self.halfsin_table } else { &self.fullsin_table };
        let phase = if self.short_noise != 0 { 0x300 } else { 0x100 };
        let h = wave_table[phase];
        if slot.eg_out >= EG_MAX { return 0; }
        let att = EG_MUTE.min(slot.eg_out + slot.tll as u32) << 4;
        lookup_exp_table(h + att as u16)
    }

    fn calc_slot_hat(&mut self) -> i16 {
        let slot = &mut self.slot[SLOT_HH];
        let wave_table = if slot.patch.ws != 0 { &self.halfsin_table } else { &self.fullsin_table };
        let phase = if self.short_noise != 0 {
            if (self.noise & 1) != 0 { 0x2d0 } else { 0x234 }
        } else if (self.noise & 1) != 0 { 0x34 } else { 0xd0 };
        let h = wave_table[phase];
        if slot.eg_out >= EG_MAX { return 0; }
        let att = EG_MUTE.min(slot.eg_out + slot.tll as u32) << 4;
        lookup_exp_table(h + att as u16)
    }

    fn update_output(&mut self) {
        self.update_ampm();
        self.update_short_noise();
        self.update_slots();

        // CH1-6: always melodic
        for i in 0..6 {
            let modulated = self.calc_slot_mod(i);
            self.ch_out[i] = (-self.calc_slot_car(i, modulated)) >> 1; // _MO: negate first, then shift
        }

        // CH7
        if self.rhythm_mode == 0 {
            let p6 = self.calc_slot_mod(6);
            self.ch_out[6] = (-self.calc_slot_car(6, p6)) >> 1;
        } else {
            // BD
            let p6 = self.calc_slot_mod(6);
            self.ch_out[9] = self.calc_slot_car(6, p6); // _RO
        }
        for _ in 0..14 { self.tick_noise(); }

        // CH8
        if self.rhythm_mode == 0 {
            let p7 = self.calc_slot_mod(7);
            self.ch_out[7] = (-self.calc_slot_car(7, p7)) >> 1;
        } else {
            // HH and SD
            self.ch_out[10] = self.calc_slot_hat(); // _RO
            self.ch_out[11] = self.calc_slot_snare(); // _RO
        }
        for _ in 0..2 { self.tick_noise(); }

        // CH9
        if self.rhythm_mode == 0 {
            let p8 = self.calc_slot_mod(8);
            self.ch_out[8] = (-self.calc_slot_car(8, p8)) >> 1;
        } else {
            // TOM and CYM
            self.ch_out[12] = self.calc_slot_tom(); // _RO
            self.ch_out[13] = self.calc_slot_cym(); // _RO
        }
        for _ in 0..2 { self.tick_noise(); }
    }
    
    fn tick_noise(&mut self) {
        if (self.noise & 1) != 0 {
            self.noise ^= 0x800200;
        }
        self.noise >>= 1;
    }
    
    pub fn write_address(&mut self, addr: u8) {
        self.adr = addr as u32;
    }
    
    pub fn write_data(&mut self, data: u8) {
        if self.adr < 0x40 {
            self.reg[self.adr as usize] = data;
            self.update_reg();
        }
    }
    
    fn update_reg(&mut self) {
        let mut addr = self.adr as usize;

        // Register mirroring (C reference: 0x19-0x1F -> 0x10-0x16, etc.)
        if (0x19..=0x1f).contains(&addr) || (0x29..=0x2f).contains(&addr) || (0x39..=0x3f).contains(&addr) {
            addr -= 9;
        }
        let data = self.reg[addr];

        match addr {
            0x00 => { // Modulator AM/PM/EG/KR/ML
                self.patch[0].am = ((data >> 7) & 1) as u32;
                self.patch[0].pm = ((data >> 6) & 1) as u32;
                self.patch[0].eg = ((data >> 5) & 1) as u32;
                self.patch[0].kr = ((data >> 4) & 1) as u32;
                self.patch[0].ml = (data & 15) as u32;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.slot[i * 2].update_requests |= UPDATE_RKS | UPDATE_EG;
                    }
                }
            }
            0x01 => { // Carrier AM/PM/EG/KR/ML
                self.patch[1].am = ((data >> 7) & 1) as u32;
                self.patch[1].pm = ((data >> 6) & 1) as u32;
                self.patch[1].eg = ((data >> 5) & 1) as u32;
                self.patch[1].kr = ((data >> 4) & 1) as u32;
                self.patch[1].ml = (data & 15) as u32;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.slot[i * 2 + 1].update_requests |= UPDATE_RKS | UPDATE_EG;
                    }
                }
            }
            0x02 => { // Modulator KSL & TL
                self.patch[0].kl = ((data >> 6) & 3) as u32;
                self.patch[0].tl = (data & 63) as u32;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.slot[i * 2].update_requests |= UPDATE_TLL;
                    }
                }
            }
            0x03 => { // Carrier KSL, Waveform, Feedback
                self.patch[1].kl = ((data >> 6) & 3) as u32;
                self.patch[1].ws = ((data >> 4) & 1) as u32;
                self.patch[0].ws = ((data >> 3) & 1) as u32;
                self.patch[0].fb = (data & 7) as u32;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.slot[i * 2].update_requests |= UPDATE_WS;
                        self.slot[i * 2 + 1].update_requests |= UPDATE_WS | UPDATE_TLL;
                    }
                }
            }
            0x04 => { // Modulator AR & DR
                self.patch[0].ar = ((data >> 4) & 15) as u32;
                self.patch[0].dr = (data & 15) as u32;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.slot[i * 2].update_requests |= UPDATE_EG;
                    }
                }
            }
            0x05 => { // Carrier AR & DR
                self.patch[1].ar = ((data >> 4) & 15) as u32;
                self.patch[1].dr = (data & 15) as u32;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.slot[i * 2 + 1].update_requests |= UPDATE_EG;
                    }
                }
            }
            0x06 => { // Modulator SL & RR
                self.patch[0].sl = ((data >> 4) & 15) as u32;
                self.patch[0].rr = (data & 15) as u32;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.slot[i * 2].update_requests |= UPDATE_EG;
                    }
                }
            }
            0x07 => { // Carrier SL & RR
                self.patch[1].sl = ((data >> 4) & 15) as u32;
                self.patch[1].rr = (data & 15) as u32;
                for i in 0..9 {
                    if self.patch_number[i] == 0 {
                        self.slot[i * 2 + 1].update_requests |= UPDATE_EG;
                    }
                }
            }
            0x0E => {
                let new_rhythm_mode = (data >> 5) & 1;
                if new_rhythm_mode != self.rhythm_mode {
                    if new_rhythm_mode != 0 {
                        self.slot[SLOT_HH].slot_type = 3;
                        self.slot[SLOT_HH].pg_keep = 1;
                        self.slot[SLOT_SD].slot_type = 3;
                        self.slot[SLOT_TOM].slot_type = 3;
                        self.slot[SLOT_CYM].slot_type = 3;
                        self.slot[SLOT_CYM].pg_keep = 1;
                        self.set_patch(6, 16);
                        self.set_patch(7, 17);
                        self.set_patch(8, 18);
                        let vol_hh = ((self.reg[0x37] >> 4) & 15) as i32;
                        self.slot[SLOT_HH].volume = vol_hh << 2;
                        self.slot[SLOT_HH].update_requests |= UPDATE_TLL;
                        let vol_tom = ((self.reg[0x38] >> 4) & 15) as i32;
                        self.slot[SLOT_TOM].volume = vol_tom << 2;
                        self.slot[SLOT_TOM].update_requests |= UPDATE_TLL;
                    } else {
                        self.slot[SLOT_HH].slot_type = 0;
                        self.slot[SLOT_HH].pg_keep = 0;
                        self.slot[SLOT_SD].slot_type = 1;
                        self.slot[SLOT_TOM].slot_type = 0;
                        self.slot[SLOT_CYM].slot_type = 1;
                        self.slot[SLOT_CYM].pg_keep = 0;
                        self.set_patch(6, (self.reg[0x36] >> 4) as usize);
                        self.set_patch(7, (self.reg[0x37] >> 4) as usize);
                        self.set_patch(8, (self.reg[0x38] >> 4) as usize);
                    }
                    self.rhythm_mode = new_rhythm_mode;
                }
                self.update_key_status();
            }
            0x10..=0x18 => {
                // F-Number Low
                let ch = addr - 0x10;
                let fnum = ((self.reg[0x20 + ch] as u16 & 1) << 8) | data as u16;
                self.set_fnumber(ch, fnum);
            }
            0x20..=0x28 => {
                // F-Number High, Block, Key On / Sustain
                let ch = addr - 0x20;
                let fnum = ((data as u16 & 1) << 8) | self.reg[0x10 + ch] as u16;
                self.set_fnumber(ch, fnum);
                let blk = (data >> 1) & 7;
                self.set_block(ch, blk);
                
                let sus_flag = (data >> 5) & 1;
                self.slot[ch * 2 + 1].sus_flag = sus_flag;
                self.slot[ch * 2 + 1].update_requests |= UPDATE_EG;
                if (self.slot[ch * 2].slot_type & 1) != 0 {
                    self.slot[ch * 2].sus_flag = sus_flag;
                    self.slot[ch * 2].update_requests |= UPDATE_EG;
                }
                
                self.update_key_status();
            }
            0x30..=0x38 => {
                let ch = addr - 0x30;
                // In rhythm mode, channels 6-8 have special handling
                if (self.reg[0x0e] & 32) != 0 && addr >= 0x36 {
                    // Rhythm mode: don't overwrite rhythm patches, but update HH/TOM volume
                    match addr {
                        0x37 => {
                            let vol = ((data >> 4) & 15) as i32;
                            self.slot[SLOT_HH].volume = vol << 2;
                            self.slot[SLOT_HH].update_requests |= UPDATE_TLL;
                        }
                        0x38 => {
                            let vol = ((data >> 4) & 15) as i32;
                            self.slot[SLOT_TOM].volume = vol << 2;
                            self.slot[SLOT_TOM].update_requests |= UPDATE_TLL;
                        }
                        _ => {} // 0x36: BD handled via set_patch(6, 16) in rhythm init
                    }
                } else {
                    self.set_patch(ch, ((data >> 4) & 15) as usize);
                }
                let vol = data & 15;
                self.set_volume(ch, vol << 2);
            }
            _ => {}
        }
    }
    


    fn set_fnumber(&mut self, ch: usize, fnum: u16) {
        let car = &mut self.slot[ch * 2 + 1];
        car.fnum = fnum;
        car.blk_fnum = (car.blk_fnum & 0xe00) | (fnum & 0x1ff);
        car.update_requests |= UPDATE_TLL | UPDATE_RKS | UPDATE_EG;

        let mod_s = &mut self.slot[ch * 2];
        mod_s.fnum = fnum;
        mod_s.blk_fnum = (mod_s.blk_fnum & 0xe00) | (fnum & 0x1ff);
        mod_s.update_requests |= UPDATE_TLL | UPDATE_RKS | UPDATE_EG;
    }

    fn set_block(&mut self, ch: usize, blk: u8) {
        let car = &mut self.slot[ch * 2 + 1];
        car.blk = blk;
        car.blk_fnum = ((blk as u16 & 7) << 9) | (car.blk_fnum & 0x1ff);
        car.update_requests |= UPDATE_TLL | UPDATE_RKS | UPDATE_EG;

        let mod_s = &mut self.slot[ch * 2];
        mod_s.blk = blk;
        mod_s.blk_fnum = ((blk as u16 & 7) << 9) | (mod_s.blk_fnum & 0x1ff);
        mod_s.update_requests |= UPDATE_TLL | UPDATE_RKS | UPDATE_EG;
    }

    fn set_volume(&mut self, ch: usize, volume: u8) {
        let car = &mut self.slot[ch * 2 + 1];
        car.volume = volume as i32;
        car.update_requests |= UPDATE_TLL;
    }

    fn set_patch(&mut self, ch: usize, num: usize) {
        self.patch_number[ch] = num as i32;
        self.slot[ch * 2].patch = self.patch[num * 2];
        self.slot[ch * 2 + 1].patch = self.patch[num * 2 + 1];
        self.slot[ch * 2].update_requests |= UPDATE_ALL;
        self.slot[ch * 2 + 1].update_requests |= UPDATE_ALL;
    }

    fn update_key_status(&mut self) {
        let r14 = self.reg[0x0e];
        let rhythm_mode = (r14 >> 5) & 1;
        let mut new_slot_key_status = 0u32;

        for ch in 0..9 {
            if (self.reg[0x20 + ch] & 0x10) != 0 {
                new_slot_key_status |= 3 << (ch * 2);
            }
        }

        if rhythm_mode != 0 {
            if (r14 & 0x10) != 0 { new_slot_key_status |= 3 << 12; } // BD
            if (r14 & 0x01) != 0 { new_slot_key_status |= 1 << 14; } // HH
            if (r14 & 0x08) != 0 { new_slot_key_status |= 1 << 15; } // SD
            if (r14 & 0x04) != 0 { new_slot_key_status |= 1 << 16; } // TOM
            if (r14 & 0x02) != 0 { new_slot_key_status |= 1 << 17; } // CYM
        }

        let updated_status = self.slot_key_status ^ new_slot_key_status;
        if updated_status != 0 {
            for i in 0..18 {
                if (updated_status & (1 << i)) != 0 {
                    if (new_slot_key_status & (1 << i)) != 0 {
                        // slotOn
                        self.slot[i].key_flag = 1;
                        self.slot[i].eg_state = EG_DAMP;
                        self.slot[i].update_requests |= UPDATE_EG;
                    } else {
                        // slotOff — only carrier transitions to RELEASE
                        self.slot[i].key_flag = 0;
                        if (self.slot[i].slot_type & 1) != 0 {
                            self.slot[i].eg_state = EG_RELEASE;
                            self.slot[i].update_requests |= UPDATE_EG;
                        }
                    }
                }
            }
        }
        self.slot_key_status = new_slot_key_status;
    }

    pub fn generate_sample(&mut self) -> f32 {
        while self.out_time < self.inp_step {
            self.out_time += self.out_step;
            self.update_output();
            
            let mut out: i32 = 0;
            for i in 0..14 {
                out += self.ch_out[i] as i32;
            }
            self.conv.put_data(out.clamp(-32768, 32767) as i16);
        }
        self.out_time -= self.inp_step;

        let mix_out = self.conv.get_data();
        (mix_out as f32) / 32768.0 // Normalize to -1.0 to 1.0
    }
}
