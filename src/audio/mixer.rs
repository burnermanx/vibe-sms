use super::psg::Psg;
use super::fm::Fm;

pub struct AudioMixer {
    pub psg: Psg,
    pub fm: Fm,
}

impl AudioMixer {
    pub fn new() -> Self {
        Self {
            psg: Psg::new(),
            fm: Fm::new(),
        }
    }

    pub fn generate_sample(&mut self) -> f32 {
        let psg_out = self.psg.generate_sample();
        let fm_out = self.fm.generate_sample();
        
        // Summing and allowing a safe headroom
        (psg_out + fm_out) / 2.0
    }
}
