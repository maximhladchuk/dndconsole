//! Input level metering.
//!
//! Pure arithmetic, kept separate so the meter can be tested without a microphone —
//! and so the audio callback stays trivial.

/// Peak and RMS of a block of mono samples, in linear amplitude.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Level {
    pub peak: f32,
    pub rms: f32,
}

impl Level {
    pub fn of(samples: &[f32]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }

        let mut peak = 0.0f32;
        let mut sum_squares = 0.0f64;

        for &sample in samples {
            // A NaN from a misbehaving driver must not poison the meter.
            if !sample.is_finite() {
                continue;
            }
            peak = peak.max(sample.abs());
            sum_squares += f64::from(sample) * f64::from(sample);
        }

        Self {
            peak,
            rms: (sum_squares / samples.len() as f64).sqrt() as f32,
        }
    }

    /// Smooth towards `self`, so the meter falls gently instead of flickering.
    ///
    /// `attack` and `release` are per-block coefficients in `0.0..=1.0`.
    pub fn smoothed(self, previous: Level, attack: f32, release: f32) -> Level {
        fn blend(previous: f32, current: f32, attack: f32, release: f32) -> f32 {
            let coefficient = if current > previous { attack } else { release };
            previous + (current - previous) * coefficient.clamp(0.0, 1.0)
        }

        Level {
            peak: blend(previous.peak, self.peak, attack, release),
            rms: blend(previous.rms, self.rms, attack, release),
        }
    }

    /// Convert to a 0..1 bar position on a decibel scale, which matches how loudness
    /// is actually perceived. `floor_db` is the bottom of the meter.
    pub fn bar(value: f32, floor_db: f32) -> f32 {
        if !value.is_finite() || value <= 0.0 {
            return 0.0;
        }
        let db = 20.0 * value.log10();
        ((db - floor_db) / -floor_db).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_measures_zero() {
        let level = Level::of(&[0.0; 256]);
        assert_eq!(level.peak, 0.0);
        assert_eq!(level.rms, 0.0);
    }

    #[test]
    fn an_empty_block_is_silence_rather_than_a_panic() {
        assert_eq!(Level::of(&[]), Level::default());
    }

    #[test]
    fn a_full_scale_square_wave_reads_one() {
        let samples: Vec<f32> = (0..128)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let level = Level::of(&samples);
        assert!((level.peak - 1.0).abs() < 1e-6);
        assert!((level.rms - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_sine_wave_has_rms_of_about_point_seven_of_its_peak() {
        let samples: Vec<f32> = (0..4096)
            .map(|i| (i as f32 * std::f32::consts::TAU / 64.0).sin())
            .collect();
        let level = Level::of(&samples);

        assert!((level.peak - 1.0).abs() < 0.01, "peak {}", level.peak);
        // RMS of a sine is 1/sqrt(2).
        assert!((level.rms - 0.707).abs() < 0.01, "rms {}", level.rms);
    }

    #[test]
    fn non_finite_samples_are_ignored_rather_than_poisoning_the_meter() {
        let level = Level::of(&[0.5, f32::NAN, 0.5, f32::INFINITY]);
        assert!(level.peak.is_finite());
        assert!(level.rms.is_finite());
        assert!((level.peak - 0.5).abs() < 1e-6);
    }

    #[test]
    fn smoothing_rises_fast_and_falls_slowly() {
        let quiet = Level {
            peak: 0.0,
            rms: 0.0,
        };
        let loud = Level {
            peak: 1.0,
            rms: 1.0,
        };

        let rising = loud.smoothed(quiet, 0.8, 0.1);
        assert!(rising.peak > 0.7, "attack should be quick: {}", rising.peak);

        let falling = quiet.smoothed(loud, 0.8, 0.1);
        assert!(
            falling.peak > 0.85,
            "release should be slow: {}",
            falling.peak
        );
    }

    #[test]
    fn the_bar_maps_the_decibel_range_onto_zero_to_one() {
        assert_eq!(Level::bar(0.0, -60.0), 0.0);
        assert_eq!(Level::bar(-1.0, -60.0), 0.0);
        assert!((Level::bar(1.0, -60.0) - 1.0).abs() < 1e-6);

        // -6 dB is roughly 0.5 amplitude and should land near the top of the bar.
        let half = Level::bar(0.5, -60.0);
        assert!((0.85..0.95).contains(&half), "got {half}");

        // -60 dB is the floor.
        assert!(Level::bar(0.001, -60.0) < 0.02);
    }
}
