/// Hardware platform being emulated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    MasterSystem,
    GameGear,
    Sg1000,   // SG-1000 — 1 KB RAM, TMS9918A VDP, no Sega mapper
    Sc3000,   // SC-3000 — 2 KB RAM, same VDP as SG-1000
}

impl Platform {
    pub fn is_gg(self) -> bool {
        self == Platform::GameGear
    }
    /// True for SG-1000 and SC-3000.
    pub fn is_sg_family(self) -> bool {
        matches!(self, Platform::Sg1000 | Platform::Sc3000)
    }
}
