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
        let fm_raw = self.fm.generate_sample();

        // The YM2413 per-channel output tops out at ~±2047 before the /32768
        // normalisation, giving ~±0.063 per active channel.  The PSG sits at
        // MAX_VOLUME = 0.25 per channel — about 4× louder.  Apply a matching
        // gain so FM and PSG are balanced as they are on real hardware.
        const FM_GAIN: f32 = 4.0;
        let fm_out = (fm_raw * FM_GAIN).clamp(-1.0, 1.0);

        // Divide by 2 for headroom when both sources are at maximum.
        ((psg_l + fm_out) / 2.0, (psg_r + fm_out) / 2.0)
    }
}
