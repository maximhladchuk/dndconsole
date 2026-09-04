//! Resampling microphone audio to the 16 kHz mono that Silero VAD and Whisper both
//! require.
//!
//! Microphones on macOS typically run at 44.1 or 48 kHz. 48 kHz is an exact 3:1 ratio,
//! but 44.1 kHz is not, so a real resampler is needed rather than sample dropping —
//! aliasing would be fed straight into speech recognition.

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::{Error, Result};

/// The rate everything downstream of capture works in.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Fixed input block. At 48 kHz this is 21 ms of audio, which keeps latency low while
/// giving the FFT resampler a sensible window.
const CHUNK_FRAMES: usize = 1024;

/// Streaming mono resampler with an internal input buffer, so it can be fed the
/// variable-sized blocks a sound card hands out.
pub struct MonoResampler {
    inner: Option<Fft<f32>>,
    input_rate: u32,
    pending: Vec<f32>,
    chunk_frames: usize,
    scratch_out: Vec<f32>,
}

impl MonoResampler {
    pub fn new(input_rate: u32) -> Result<Self> {
        if input_rate == 0 {
            return Err(Error::Resampler {
                from: input_rate,
                to: TARGET_SAMPLE_RATE,
                reason: "input sample rate is zero".to_string(),
            });
        }

        // A device already running at the target rate needs no resampling at all.
        if input_rate == TARGET_SAMPLE_RATE {
            return Ok(Self {
                inner: None,
                input_rate,
                pending: Vec::new(),
                chunk_frames: CHUNK_FRAMES,
                scratch_out: Vec::new(),
            });
        }

        let resampler = Fft::<f32>::new(
            input_rate as usize,
            TARGET_SAMPLE_RATE as usize,
            CHUNK_FRAMES,
            1,
            FixedSync::Input,
        )
        .map_err(|e| Error::Resampler {
            from: input_rate,
            to: TARGET_SAMPLE_RATE,
            reason: e.to_string(),
        })?;

        let chunk_frames = resampler.input_frames_next();
        let scratch_out = vec![0.0; resampler.output_frames_max()];

        Ok(Self {
            inner: Some(resampler),
            input_rate,
            pending: Vec::with_capacity(chunk_frames * 2),
            chunk_frames,
            scratch_out,
        })
    }

    pub fn input_rate(&self) -> u32 {
        self.input_rate
    }

    /// Feed mono samples at the input rate; appends 16 kHz samples to `out`.
    ///
    /// Anything that does not fill a whole chunk is held over for the next call, so no
    /// audio is lost at block boundaries.
    pub fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) -> Result<()> {
        let Some(resampler) = self.inner.as_mut() else {
            out.extend_from_slice(samples);
            return Ok(());
        };

        self.pending.extend_from_slice(samples);

        while self.pending.len() >= self.chunk_frames {
            let input =
                InterleavedSlice::new(&self.pending[..self.chunk_frames], 1, self.chunk_frames)
                    .map_err(|e| Error::Resampler {
                        from: self.input_rate,
                        to: TARGET_SAMPLE_RATE,
                        reason: e.to_string(),
                    })?;

            let mut output = InterleavedSlice::new_mut(
                self.scratch_out.as_mut_slice(),
                1,
                resampler.output_frames_max(),
            )
            .map_err(|e| Error::Resampler {
                from: self.input_rate,
                to: TARGET_SAMPLE_RATE,
                reason: e.to_string(),
            })?;

            let (consumed, produced) = resampler
                .process_into_buffer(&input, &mut output, None)
                .map_err(|e| Error::Resampler {
                    from: self.input_rate,
                    to: TARGET_SAMPLE_RATE,
                    reason: e.to_string(),
                })?;

            out.extend_from_slice(&self.scratch_out[..produced]);
            self.pending.drain(..consumed.max(1));
        }

        Ok(())
    }

    /// Samples held back waiting for a full chunk. Diagnostic only.
    pub fn pending_frames(&self) -> usize {
        self.pending.len()
    }
}

/// Collapse interleaved multi-channel audio to mono by averaging the channels.
///
/// Averaging rather than taking the first channel: a stereo interface with the mic on
/// the right input would otherwise record silence.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16, out: &mut Vec<f32>) {
    let channels = channels.max(1) as usize;

    if channels == 1 {
        out.extend_from_slice(interleaved);
        return;
    }

    for frame in interleaved.chunks_exact(channels) {
        let sum: f32 = frame.iter().copied().filter(|s| s.is_finite()).sum();
        out.push(sum / channels as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f32, seconds: f32) -> Vec<f32> {
        let count = (rate as f32 * seconds) as usize;
        (0..count)
            .map(|i| (i as f32 / rate as f32 * freq * std::f32::consts::TAU).sin())
            .collect()
    }

    #[test]
    fn a_matching_rate_passes_samples_through_untouched() {
        let mut resampler = MonoResampler::new(16_000).expect("resampler");
        let input = sine(16_000, 440.0, 0.1);

        let mut out = Vec::new();
        resampler.push(&input, &mut out).expect("push");

        assert_eq!(out, input);
    }

    #[test]
    fn forty_eight_kilohertz_becomes_sixteen() {
        let mut resampler = MonoResampler::new(48_000).expect("resampler");
        let input = sine(48_000, 440.0, 1.0);

        let mut out = Vec::new();
        resampler.push(&input, &mut out).expect("push");

        // One second in, one second out, minus whatever is still buffered.
        let expected = 16_000;
        let tolerance = 400; // a chunk's worth of slack at the tail
        assert!(
            (expected - tolerance..=expected + tolerance).contains(&(out.len() as i32 as usize)),
            "got {} samples, expected about {expected}",
            out.len()
        );
    }

    #[test]
    fn forty_four_point_one_kilohertz_also_lands_near_sixteen() {
        let mut resampler = MonoResampler::new(44_100).expect("resampler");
        let input = sine(44_100, 440.0, 1.0);

        let mut out = Vec::new();
        resampler.push(&input, &mut out).expect("push");

        assert!(
            (15_600..=16_400).contains(&out.len()),
            "got {} samples from a non-integer ratio",
            out.len()
        );
    }

    #[test]
    fn resampled_audio_is_finite_and_stays_in_range() {
        let mut resampler = MonoResampler::new(48_000).expect("resampler");
        let mut out = Vec::new();
        resampler
            .push(&sine(48_000, 1000.0, 0.5), &mut out)
            .expect("push");

        assert!(!out.is_empty());
        assert!(
            out.iter().all(|s| s.is_finite()),
            "resampler produced NaN or inf"
        );
        assert!(
            out.iter().all(|s| s.abs() <= 1.5),
            "resampler produced wildly out-of-range samples"
        );
    }

    #[test]
    fn a_sine_survives_resampling_with_its_amplitude_intact() {
        let mut resampler = MonoResampler::new(48_000).expect("resampler");
        let mut out = Vec::new();
        resampler
            .push(&sine(48_000, 440.0, 0.5), &mut out)
            .expect("push");

        // Skip the filter's start-up transient before measuring.
        let steady = &out[out.len() / 4..];
        let peak = steady.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
        assert!(
            (0.9..=1.1).contains(&peak),
            "peak amplitude drifted to {peak}"
        );
    }

    #[test]
    fn feeding_small_irregular_blocks_produces_the_same_amount_of_audio() {
        let input = sine(48_000, 440.0, 1.0);

        let mut whole = MonoResampler::new(48_000).expect("resampler");
        let mut whole_out = Vec::new();
        whole.push(&input, &mut whole_out).expect("push");

        // Sound cards deliver awkward block sizes; the resampler must not care.
        let mut chunked = MonoResampler::new(48_000).expect("resampler");
        let mut chunked_out = Vec::new();
        for block in input.chunks(377) {
            chunked.push(block, &mut chunked_out).expect("push");
        }

        assert_eq!(whole_out.len(), chunked_out.len());
    }

    #[test]
    fn a_zero_input_rate_is_rejected() {
        assert!(MonoResampler::new(0).is_err());
    }

    #[test]
    fn downmix_averages_the_channels() {
        let mut out = Vec::new();
        // Two frames of stereo: (1.0, 0.0) and (0.5, 0.5).
        downmix_to_mono(&[1.0, 0.0, 0.5, 0.5], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn downmix_passes_mono_through() {
        let mut out = Vec::new();
        downmix_to_mono(&[0.1, 0.2, 0.3], 1, &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn downmix_keeps_audio_from_a_single_populated_channel() {
        // The mic is on the right input only; taking channel 0 would record silence.
        let mut out = Vec::new();
        downmix_to_mono(&[0.0, 0.8, 0.0, 0.6], 2, &mut out);
        assert!(out.iter().all(|s| *s > 0.0), "signal was lost: {out:?}");
    }

    #[test]
    fn downmix_ignores_a_trailing_partial_frame() {
        let mut out = Vec::new();
        downmix_to_mono(&[1.0, 1.0, 1.0], 2, &mut out);
        assert_eq!(out.len(), 1, "the incomplete frame should be dropped");
    }
}
