//! Validated playback rates and one snapshot for UI/desktop notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate(u8);

impl Rate {
    pub const MIN: i32 = 5;
    pub const MAX: i32 = 20;
    pub const NORMAL: Self = Self(10);
    pub const SCALE: f64 = 10.0;

    pub fn checked(tenths: i32) -> Option<Self> {
        (Self::MIN..=Self::MAX)
            .contains(&tenths)
            .then_some(Self(tenths as u8))
    }
    pub fn from_multiplier(value: f64) -> Option<Self> {
        let tenths = value * Self::SCALE;
        (tenths.is_finite() && (tenths - tenths.round()).abs() < 0.000_001)
            .then(|| Self::checked(tenths.round() as i32))
            .flatten()
    }
    pub fn tenths(self) -> i32 {
        i32::from(self.0)
    }
    pub fn multiplier(self) -> f64 {
        f64::from(self.0) / Self::SCALE
    }
    pub fn faster(self) -> bool {
        self.0 > Self::NORMAL.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Inactive,
    Preparing,
    Ended,
    MissingTempo,
    LiveOnly,
    Variable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub applied: Rate,
    pub requested: Rate,
    pub availability: Availability,
    pub at_live_edge: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            applied: Rate::NORMAL,
            requested: Rate::NORMAL,
            availability: Availability::Inactive,
            at_live_edge: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_rates_must_be_finite_in_range_and_in_tenths() {
        for invalid in [f64::NAN, f64::INFINITY, -1., 0., 0.4, 2.1, 1.25, f64::MAX] {
            assert!(Rate::from_multiplier(invalid).is_none(), "{invalid}");
        }
        for tenth in Rate::MIN..=Rate::MAX {
            let rate = Rate::checked(tenth).unwrap();
            assert_eq!(Rate::from_multiplier(rate.multiplier()), Some(rate));
        }
    }
}

#[cfg(test)]
mod audio_tests {
    use super::*;
    use gstreamer::{self as gst, prelude::*};

    #[test]
    fn scaletempo_preserves_pitch_and_scales_pcm_duration() -> Result<(), Box<dyn std::error::Error>>
    {
        const SAMPLE_RATE: f64 = 48_000.;
        const TONE_HZ: f64 = 440.;
        const SOURCE_SECONDS: f64 = 2.;
        // A finite signal can leave an input window and an output stride undrained.
        const TAIL_SECONDS: f64 = 0.06;
        gst::init()?;
        for tenths in [5, 11, 15, 20] {
            // Explicit memory output. This test never creates an audio device.
            let pipeline = gst::parse::launch("audiotestsrc wave=sine freq=440 samplesperbuffer=480 ! audio/x-raw,format=F32LE,rate=48000,channels=1 ! scaletempo ! appsink name=output sync=false")?
                .downcast::<gst::Pipeline>().map_err(|_| "pipeline")?;
            struct Stop(gst::Pipeline);
            impl Drop for Stop {
                fn drop(&mut self) {
                    let _ = self.0.set_state(gst::State::Null);
                }
            }
            let _stop = Stop(pipeline.clone());
            let sink = pipeline
                .by_name("output")
                .ok_or("output")?
                .downcast::<gstreamer_app::AppSink>()
                .map_err(|_| "appsink")?;
            pipeline.set_state(gst::State::Paused)?;
            pipeline.state(gst::ClockTime::from_seconds(5)).0?;
            let rate = Rate::checked(tenths).unwrap().multiplier();
            pipeline.seek(
                rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                gst::ClockTime::ZERO,
                gst::SeekType::Set,
                gst::ClockTime::from_seconds(SOURCE_SECONDS as u64),
            )?;
            pipeline.set_state(gst::State::Playing)?;
            let mut pcm = Vec::new();
            while let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(5)) {
                let map = sample.buffer().ok_or("buffer")?.map_readable()?;
                pcm.extend(
                    map.as_slice()
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes(b.try_into().unwrap())),
                );
            }
            assert!(sink.is_eos(), "rate {rate}: no EOS");
            let expected_samples = SOURCE_SECONDS * SAMPLE_RATE / rate;
            assert!(
                (pcm.len() as f64 - expected_samples).abs()
                    < SAMPLE_RATE * TAIL_SECONDS * (1. + 1. / rate),
                "rate {rate}: {} samples vs {expected_samples}",
                pcm.len()
            );
            // Ignore the first stride; count positive zero crossings in steady output.
            let steady = &pcm[(SAMPLE_RATE * 0.1) as usize..];
            let cycles = steady
                .windows(2)
                .filter(|p| p[0] <= 0. && p[1] > 0.)
                .count();
            let pitch = cycles as f64 * SAMPLE_RATE / steady.len() as f64;
            assert!(
                (pitch - TONE_HZ).abs() < 5.,
                "rate {rate}: pitch {pitch} Hz"
            );
        }
        Ok(())
    }
}
