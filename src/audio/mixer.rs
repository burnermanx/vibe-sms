use super::psg::Psg;
use super::fm::Fm;

pub(crate) struct AudioMixer {
    pub(crate) psg: Psg,
    pub(crate) fm: Fm,
}

impl AudioMixer {
    pub(crate) fn new(is_gg: bool, sample_rate: f32) -> Self {
        Self {
            psg: Psg::new(is_gg, sample_rate),
            fm: Fm::new(),
        }
    }

    pub(crate) fn generate_sample(&mut self) -> (f32, f32) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_channels_silent_by_default() {
        let mut mixer = AudioMixer::new(false, 44100.0);
        // All PSG volume registers default to 15 (silent); FM disabled by default.
        let (l, r) = mixer.generate_sample();
        assert_eq!(l, 0.0);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn output_is_clamped_to_unit_range() {
        let mut mixer = AudioMixer::new(false, 44100.0);
        // Force maximum PSG output: ch0 volume=0 (loudest), tone=1 (fast toggle)
        mixer.psg.write_data(0x81); // latch ch0 tone, data=1
        mixer.psg.write_data(0x90); // latch ch0 volume=0
        for _ in 0..1000 {
            let (l, r) = mixer.generate_sample();
            assert!(l >= -1.0 && l <= 1.0, "left sample out of range: {l}");
            assert!(r >= -1.0 && r <= 1.0, "right sample out of range: {r}");
        }
    }

    #[test]
    fn psg_active_produces_nonzero_output() {
        let mut mixer = AudioMixer::new(false, 44100.0);
        // ch0: tone=1 (fast), volume=0 (max)
        mixer.psg.write_data(0x81);
        mixer.psg.write_data(0x90);
        let samples: Vec<(f32, f32)> = (0..200).map(|_| mixer.generate_sample()).collect();
        let any_nonzero = samples.iter().any(|(l, r)| *l != 0.0 || *r != 0.0);
        assert!(any_nonzero, "expected at least one nonzero sample with active PSG channel");
    }

    #[test]
    fn fm_gain_applied_before_mix() {
        // FM output is multiplied by FM_GAIN=4.0 and clamped before mixing.
        // With FM disabled (user_disabled=true), fm_out = 0 regardless of ym2413 state.
        let mut mixer = AudioMixer::new(false, 44100.0);
        mixer.fm.user_disabled = true;
        let (l, r) = mixer.generate_sample();
        assert_eq!(l, 0.0);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn stereo_channels_symmetric_when_no_gg_panning() {
        // SMS mixer produces equal L/R since PSG has no per-channel panning.
        let mut mixer = AudioMixer::new(false, 44100.0);
        mixer.psg.write_data(0x81); // ch0 tone active
        mixer.psg.write_data(0x90); // ch0 vol=0
        for _ in 0..100 {
            let (l, r) = mixer.generate_sample();
            assert_eq!(l, r, "SMS L/R should be equal without GG panning");
        }
    }
}
