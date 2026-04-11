// Screen dimensions
pub(crate) const SMS_W: usize = 256;
pub(crate) const SMS_H: usize = 192;
pub(crate) const GG_W: usize = 160;
pub(crate) const GG_H: usize = 144;

/// Hardware platform being emulated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Platform {
    MasterSystem,
    GameGear,
    Sg1000,   // SG-1000 — 1 KB RAM, TMS9918A VDP, no Sega mapper
    Sc3000,   // SC-3000 — 2 KB RAM, same VDP as SG-1000
}

impl Platform {
    pub(crate) fn is_gg(self) -> bool {
        self == Platform::GameGear
    }
    /// True for SG-1000 and SC-3000.
    pub(crate) fn is_sg_family(self) -> bool {
        matches!(self, Platform::Sg1000 | Platform::Sc3000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_gg_only_for_game_gear() {
        assert!(Platform::GameGear.is_gg());
        assert!(!Platform::MasterSystem.is_gg());
        assert!(!Platform::Sg1000.is_gg());
        assert!(!Platform::Sc3000.is_gg());
    }

    #[test]
    fn is_sg_family_for_sg_and_sc_only() {
        assert!(Platform::Sg1000.is_sg_family());
        assert!(Platform::Sc3000.is_sg_family());
        assert!(!Platform::MasterSystem.is_sg_family());
        assert!(!Platform::GameGear.is_sg_family());
    }
}
