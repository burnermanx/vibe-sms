use super::psg::Psg;
use super::fm::Fm;

pub struct AudioMixer {
    pub psg: Psg,
    pub fm: Fm,
}

impl AudioMixer {
    pub fn new(is_gg: bool, sample_rate: f32) -> Self {
        Self {
            psg: Psg::new(is_gg, sample_rate),
            fm: Fm::new(),
        }
    }

    pub fn generate_sample(&mut self) -> (f32, f32) {
        let (psg_l, psg_r) = self.psg.generate_sample();
        let fm_out = self.fm.generate_sample();

        // Summing and allowing a safe headroom (Monophonic FM centered)
        ((psg_l + fm_out) / 2.0, (psg_r + fm_out) / 2.0)
    }
}
