//! Voice activity detection with Silero VAD.
//!
//! Two pieces, deliberately separate:
//!
//! * [`SileroVad`] runs the ONNX model on one 512-sample frame and returns a speech
//!   probability. It owns the model's recurrent state.
//! * [`Segmenter`] turns that stream of probabilities into speech segments, with
//!   hysteresis, pre-roll and post-roll. It is pure arithmetic and fully testable
//!   without the model.
//!
//! The graph signature below was read from the model itself rather than assumed
//! (`cargo run -p dndsound-pipeline --example inspect_vad`):
//!
//! ```text
//! inputs   input : f32 [batch, sequence]   (64 samples of context + 512 new)
//!          state : f32 [2, batch, 128]
//!          sr    : i64 scalar
//! outputs  output: f32 [batch, 1]
//!          stateN: f32 [2, batch, 128]
//! ```
//!
//! Note that the 16 kHz-only model still takes an `sr` input.

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::resample::TARGET_SAMPLE_RATE;
use crate::{Error, Result};

/// Silero processes exactly 512 samples at a time at 16 kHz — 32 ms per frame.
pub const FRAME_SAMPLES: usize = 512;

/// Size of the model's recurrent state: `[2, batch, 128]` with batch 1.
const STATE_LEN: usize = 2 * 128;

/// Silero prepends the tail of the previous frame to the current one before running the
/// network. At 16 kHz that context is 64 samples, so the tensor the graph actually sees
/// is 576 long even though callers hand over 512. Getting this wrong does not error —
/// the model simply reports near-zero probability for everything, including obvious
/// speech, which is exactly the kind of silent failure the VAD tests exist to catch.
const CONTEXT_SAMPLES: usize = 64;

pub struct SileroVad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
    input_scratch: Vec<f32>,
}

impl SileroVad {
    pub fn load(model_path: impl AsRef<Path>) -> Result<Self> {
        let path = model_path.as_ref();

        let session = Session::builder()
            .and_then(|mut builder| builder.commit_from_file(path))
            .map_err(|e| Error::Vad(format!("could not load {}: {e}", path.display())))?;

        Ok(Self {
            session,
            state: vec![0.0; STATE_LEN],
            context: vec![0.0; CONTEXT_SAMPLES],
            input_scratch: vec![0.0; CONTEXT_SAMPLES + FRAME_SAMPLES],
        })
    }

    /// Forget the recurrent state and context, e.g. when starting a new capture.
    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.context.fill(0.0);
    }

    /// Speech probability for one 512-sample frame of 16 kHz mono audio.
    pub fn probability(&mut self, frame: &[f32]) -> Result<f32> {
        if frame.len() != FRAME_SAMPLES {
            return Err(Error::Vad(format!(
                "expected {FRAME_SAMPLES} samples per frame, got {}",
                frame.len()
            )));
        }

        self.input_scratch[..CONTEXT_SAMPLES].copy_from_slice(&self.context);
        self.input_scratch[CONTEXT_SAMPLES..].copy_from_slice(frame);

        let input = Tensor::from_array((
            [1_usize, CONTEXT_SAMPLES + FRAME_SAMPLES],
            self.input_scratch.clone(),
        ))
        .map_err(|e| Error::Vad(e.to_string()))?;
        let state = Tensor::from_array(([2_usize, 1, 128], self.state.clone()))
            .map_err(|e| Error::Vad(e.to_string()))?;
        // `sr` is a scalar in the graph, not a one-element vector. Passing the wrong
        // rank makes the model return near-zero probabilities for everything.
        let sample_rate = Tensor::from_array(([0_usize; 0], vec![i64::from(TARGET_SAMPLE_RATE)]))
            .map_err(|e| Error::Vad(e.to_string()))?;

        let outputs = self
            .session
            .run(ort::inputs! {
                "input" => input,
                "state" => state,
                "sr" => sample_rate,
            })
            .map_err(|e| Error::Vad(e.to_string()))?;

        let (_, next_state) = outputs["stateN"]
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Vad(e.to_string()))?;
        // Carrying the state forward is what makes this a sequence model rather than a
        // frame classifier; dropping it would badly hurt accuracy.
        self.state.copy_from_slice(next_state);

        let (_, probability) = outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Vad(e.to_string()))?;

        let probability = probability
            .first()
            .copied()
            .ok_or_else(|| Error::Vad("model returned no probability".to_string()))?;

        self.context
            .copy_from_slice(&frame[FRAME_SAMPLES - CONTEXT_SAMPLES..]);

        Ok(probability)
    }
}

/// Tunable thresholds, all exposed in Settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadConfig {
    pub speech_threshold: f32,
    pub min_speech_ms: u32,
    pub silence_timeout_ms: u32,
    pub pre_roll_ms: u32,
    pub post_roll_ms: u32,
    pub max_segment_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            speech_threshold: 0.5,
            min_speech_ms: 250,
            silence_timeout_ms: 700,
            pre_roll_ms: 300,
            post_roll_ms: 200,
            // Long enough that ordinary sentences close on silence instead, short enough
            // that a Dungeon Master in full flow still gets sounds while talking. The cut
            // is seamless — see `close_if_too_long`.
            max_segment_ms: 5_000,
        }
    }
}

impl VadConfig {
    fn frames(ms: u32) -> usize {
        let per_frame_ms = FRAME_SAMPLES as f32 / TARGET_SAMPLE_RATE as f32 * 1000.0;
        ((ms as f32 / per_frame_ms).round() as usize).max(1)
    }
}

/// What the segmenter reports after each frame.
#[derive(Debug, Clone, PartialEq)]
pub enum VadEvent {
    /// Nothing of interest; carries the frame's probability for Debug Mode.
    Idle { probability: f32 },
    /// Speech has just started. Audio from the pre-roll onwards is being collected.
    SpeechStarted,
    /// Speech is ongoing.
    Speaking { probability: f32 },
    /// A segment finished. Carries the audio, pre-roll and post-roll included.
    SegmentReady(Segment),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub samples: Vec<f32>,
    /// Sample index, from the start of capture, of the first sample.
    pub start_sample: u64,
    /// True when the segment was cut short by `max_segment_ms` rather than by silence.
    pub truncated: bool,
}

impl Segment {
    pub fn duration_ms(&self) -> u32 {
        (self.samples.len() as f32 / TARGET_SAMPLE_RATE as f32 * 1000.0) as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Silence,
    /// Speech has been detected but has not yet lasted `min_speech_ms`.
    Rising,
    Speech,
    /// Speech stopped, waiting to see whether it resumes before `silence_timeout_ms`.
    Trailing,
}

/// Turns per-frame speech probabilities into segments.
///
/// Hysteresis matters here: a DM pauses mid-sentence constantly, and a segmenter that
/// closes on the first quiet frame would chop "the goblin draws… his sword" into two
/// transcripts and lose the event.
pub struct Segmenter {
    config: VadConfig,
    state: State,

    /// Ring of recent audio kept so a segment can include the moment before the VAD
    /// noticed speech. Without it, Whisper loses the attack of the first word.
    pre_roll: std::collections::VecDeque<f32>,
    pre_roll_capacity: usize,

    current: Vec<f32>,
    current_start_sample: u64,

    rising_frames: usize,
    trailing_frames: usize,

    frames_seen: u64,
    samples_seen: u64,
}

impl Segmenter {
    pub fn new(config: VadConfig) -> Self {
        let pre_roll_capacity = VadConfig::frames(config.pre_roll_ms) * FRAME_SAMPLES;
        Self {
            config,
            state: State::Silence,
            pre_roll: std::collections::VecDeque::with_capacity(pre_roll_capacity + FRAME_SAMPLES),
            pre_roll_capacity,
            current: Vec::new(),
            current_start_sample: 0,
            rising_frames: 0,
            trailing_frames: 0,
            frames_seen: 0,
            samples_seen: 0,
        }
    }

    pub fn config(&self) -> VadConfig {
        self.config
    }

    /// Apply new settings, keeping any segment already in progress.
    pub fn reconfigure(&mut self, config: VadConfig) {
        self.config = config;
        self.pre_roll_capacity = VadConfig::frames(config.pre_roll_ms) * FRAME_SAMPLES;
        while self.pre_roll.len() > self.pre_roll_capacity {
            self.pre_roll.pop_front();
        }
    }

    pub fn is_speaking(&self) -> bool {
        matches!(self.state, State::Speech | State::Trailing)
    }

    /// The audio collected so far in the segment being spoken.
    ///
    /// This is what a partial re-decode transcribes: the sentence up to this moment,
    /// while the Dungeon Master is still saying it.
    pub fn current_audio(&self) -> &[f32] {
        &self.current
    }

    /// Feed one frame and its probability.
    pub fn push(&mut self, frame: &[f32], probability: f32) -> VadEvent {
        self.frames_seen += 1;
        let frame_start_sample = self.samples_seen;
        self.samples_seen += frame.len() as u64;

        let is_speech = probability >= self.config.speech_threshold;

        match self.state {
            State::Silence => {
                self.remember_pre_roll(frame);
                if is_speech {
                    self.state = State::Rising;
                    self.rising_frames = 1;
                    self.start_segment(frame_start_sample);
                    self.current.extend_from_slice(frame);

                    // A single loud frame can be a door slam; only commit once speech
                    // has lasted long enough.
                    if self.rising_frames >= VadConfig::frames(self.config.min_speech_ms) {
                        self.state = State::Speech;
                        return VadEvent::SpeechStarted;
                    }
                }
                VadEvent::Idle { probability }
            }

            State::Rising => {
                self.current.extend_from_slice(frame);
                if is_speech {
                    self.rising_frames += 1;
                    if self.rising_frames >= VadConfig::frames(self.config.min_speech_ms) {
                        self.state = State::Speech;
                        return VadEvent::SpeechStarted;
                    }
                    VadEvent::Idle { probability }
                } else {
                    // Too short to be speech. Throw it away and keep listening.
                    self.abandon_segment();
                    self.remember_pre_roll(frame);
                    VadEvent::Idle { probability }
                }
            }

            State::Speech => {
                self.current.extend_from_slice(frame);

                if let Some(segment) = self.close_if_too_long() {
                    return VadEvent::SegmentReady(segment);
                }

                if is_speech {
                    VadEvent::Speaking { probability }
                } else {
                    self.state = State::Trailing;
                    self.trailing_frames = 1;
                    VadEvent::Speaking { probability }
                }
            }

            State::Trailing => {
                self.current.extend_from_slice(frame);

                if let Some(segment) = self.close_if_too_long() {
                    return VadEvent::SegmentReady(segment);
                }

                if is_speech {
                    // The pause was just a breath.
                    self.state = State::Speech;
                    self.trailing_frames = 0;
                    return VadEvent::Speaking { probability };
                }

                self.trailing_frames += 1;
                if self.trailing_frames >= VadConfig::frames(self.config.silence_timeout_ms) {
                    let segment = self.finish_segment();
                    return VadEvent::SegmentReady(segment);
                }
                VadEvent::Speaking { probability }
            }
        }
    }

    /// Close whatever is in progress, e.g. when the user stops listening.
    pub fn flush(&mut self) -> Option<Segment> {
        if matches!(self.state, State::Speech | State::Trailing) {
            Some(self.finish_segment())
        } else {
            self.abandon_segment();
            None
        }
    }

    pub fn reset(&mut self) {
        self.state = State::Silence;
        self.pre_roll.clear();
        self.current.clear();
        self.rising_frames = 0;
        self.trailing_frames = 0;
        self.frames_seen = 0;
        self.samples_seen = 0;
    }

    fn remember_pre_roll(&mut self, frame: &[f32]) {
        for &sample in frame {
            if self.pre_roll.len() == self.pre_roll_capacity {
                self.pre_roll.pop_front();
            }
            self.pre_roll.push_back(sample);
        }
    }

    fn start_segment(&mut self, frame_start_sample: u64) {
        self.current.clear();
        self.current.extend(self.pre_roll.iter().copied());
        self.current_start_sample = frame_start_sample.saturating_sub(self.pre_roll.len() as u64);
        self.pre_roll.clear();
    }

    fn abandon_segment(&mut self) {
        self.state = State::Silence;
        self.current.clear();
        self.rising_frames = 0;
        self.trailing_frames = 0;
    }

    /// Cut a monologue that has run past `max_segment_ms`, without interrupting it.
    ///
    /// Speech has *not* stopped here, so unlike every other way a segment ends this one
    /// must not send the segmenter back to silence. Doing that would make the next chunk
    /// wait out `min_speech_ms` all over again and start with an empty pre-roll, losing
    /// the first fraction of a second of a sentence that never actually paused.
    ///
    /// The next chunk therefore begins where this one ended, minus a short overlap so a
    /// word straddling the cut is intact in at least one of the two. Text repeated across
    /// the boundary is expected; the trigger engine's matched-span suppression is what
    /// keeps it from firing the same event twice.
    fn close_if_too_long(&mut self) -> Option<Segment> {
        let max_samples =
            (self.config.max_segment_ms as usize * TARGET_SAMPLE_RATE as usize) / 1000;
        if self.current.len() < max_samples {
            return None;
        }

        let overlap = ((self.config.pre_roll_ms as usize * TARGET_SAMPLE_RATE as usize) / 1000)
            .min(self.current.len());

        let samples = std::mem::take(&mut self.current);
        let start_sample = self.current_start_sample;
        let carried = samples.len() - overlap;

        self.current.extend_from_slice(&samples[carried..]);
        self.current_start_sample = start_sample + carried as u64;

        Some(Segment {
            samples,
            start_sample,
            truncated: true,
        })
    }

    fn finish_segment(&mut self) -> Segment {
        // Every remaining caller ends a segment because speech stopped.
        let truncated = false;

        // Trailing silence is deliberately kept: Whisper transcribes the end of a word
        // better with a little room after it.
        let post_roll_samples =
            (self.config.post_roll_ms as usize * TARGET_SAMPLE_RATE as usize) / 1000;
        let keep_silence = VadConfig::frames(self.config.silence_timeout_ms) * FRAME_SAMPLES;

        let mut samples = std::mem::take(&mut self.current);
        if !truncated && keep_silence > post_roll_samples {
            let trim = (keep_silence - post_roll_samples).min(samples.len());
            samples.truncate(samples.len() - trim);
        }

        let segment = Segment {
            samples,
            start_sample: self.current_start_sample,
            truncated,
        };

        self.state = State::Silence;
        self.rising_frames = 0;
        self.trailing_frames = 0;
        self.pre_roll.clear();

        segment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(value: f32) -> Vec<f32> {
        vec![value; FRAME_SAMPLES]
    }

    /// Drive the segmenter with a script of probabilities, returning every event.
    fn run(segmenter: &mut Segmenter, probabilities: &[f32]) -> Vec<VadEvent> {
        probabilities
            .iter()
            .map(|&p| segmenter.push(&frame(if p > 0.5 { 0.3 } else { 0.0 }), p))
            .collect()
    }

    fn fast_config() -> VadConfig {
        // 32 ms per frame: 2 frames of speech to start, 3 frames of silence to end.
        VadConfig {
            speech_threshold: 0.5,
            min_speech_ms: 64,
            silence_timeout_ms: 96,
            pre_roll_ms: 64,
            post_roll_ms: 32,
            max_segment_ms: 15_000,
        }
    }

    #[test]
    fn frame_length_conversion_matches_thirty_two_milliseconds() {
        assert_eq!(VadConfig::frames(32), 1);
        assert_eq!(VadConfig::frames(64), 2);
        assert_eq!(VadConfig::frames(700), 22);
        // Never zero, however small the setting.
        assert_eq!(VadConfig::frames(0), 1);
    }

    #[test]
    fn silence_alone_produces_no_segment() {
        let mut segmenter = Segmenter::new(fast_config());
        let events = run(&mut segmenter, &[0.0; 50]);

        assert!(events.iter().all(|e| matches!(e, VadEvent::Idle { .. })));
        assert!(!segmenter.is_speaking());
    }

    #[test]
    fn a_burst_shorter_than_the_minimum_is_rejected() {
        let mut segmenter = Segmenter::new(fast_config());
        // One loud frame — a door slam, a dice cup — then silence.
        let events = run(&mut segmenter, &[0.0, 0.0, 0.9, 0.0, 0.0, 0.0, 0.0]);

        assert!(
            !events
                .iter()
                .any(|e| matches!(e, VadEvent::SegmentReady(_))),
            "a single loud frame must not become a segment"
        );
        assert!(!segmenter.is_speaking());
    }

    #[test]
    fn speech_then_silence_produces_exactly_one_segment() {
        let mut segmenter = Segmenter::new(fast_config());
        let mut script = vec![0.0, 0.0];
        script.extend(std::iter::repeat_n(0.9, 10));
        script.extend(std::iter::repeat_n(0.0, 6));

        let events = run(&mut segmenter, &script);

        let starts = events
            .iter()
            .filter(|e| **e == VadEvent::SpeechStarted)
            .count();
        let segments: Vec<&Segment> = events
            .iter()
            .filter_map(|e| match e {
                VadEvent::SegmentReady(s) => Some(s),
                _ => None,
            })
            .collect();

        assert_eq!(starts, 1, "speech should start once");
        assert_eq!(segments.len(), 1, "and end once");
        assert!(!segments[0].truncated);
        assert!(!segments[0].samples.is_empty());
    }

    #[test]
    fn a_short_pause_mid_sentence_does_not_split_the_segment() {
        let mut segmenter = Segmenter::new(fast_config());
        // "the goblin draws" … breath … "his sword"
        let mut script = vec![0.9; 6];
        script.extend([0.0, 0.0]); // shorter than silence_timeout (3 frames)
        script.extend([0.9; 6]);
        script.extend([0.0; 6]);

        let events = run(&mut segmenter, &script);
        let segments = events
            .iter()
            .filter(|e| matches!(e, VadEvent::SegmentReady(_)))
            .count();

        assert_eq!(segments, 1, "a breath must not cut the sentence in two");
    }

    #[test]
    fn a_long_pause_does_split_the_segments() {
        let mut segmenter = Segmenter::new(fast_config());
        let mut script = vec![0.9; 6];
        script.extend([0.0; 8]); // well past the silence timeout
        script.extend([0.9; 6]);
        script.extend([0.0; 8]);

        let events = run(&mut segmenter, &script);
        let segments = events
            .iter()
            .filter(|e| matches!(e, VadEvent::SegmentReady(_)))
            .count();

        assert_eq!(segments, 2);
    }

    #[test]
    fn the_segment_includes_audio_from_before_speech_was_detected() {
        let mut segmenter = Segmenter::new(fast_config());

        // Distinctive quiet audio before the speech starts.
        for _ in 0..4 {
            segmenter.push(&vec![0.01; FRAME_SAMPLES], 0.0);
        }
        for _ in 0..6 {
            segmenter.push(&vec![0.5; FRAME_SAMPLES], 0.9);
        }
        let segment = loop {
            if let VadEvent::SegmentReady(segment) = segmenter.push(&vec![0.0; FRAME_SAMPLES], 0.0)
            {
                break segment;
            }
        };

        assert!(
            segment.samples.iter().any(|&s| (s - 0.01).abs() < 1e-6),
            "pre-roll audio should be part of the segment"
        );
    }

    #[test]
    fn a_segment_that_never_pauses_is_cut_at_the_maximum_length() {
        let config = VadConfig {
            max_segment_ms: 320, // ten frames
            ..fast_config()
        };
        let mut segmenter = Segmenter::new(config);

        let events = run(&mut segmenter, &[0.9; 40]);
        let truncated: Vec<&Segment> = events
            .iter()
            .filter_map(|e| match e {
                VadEvent::SegmentReady(s) => Some(s),
                _ => None,
            })
            .collect();

        assert!(
            !truncated.is_empty(),
            "an endless monologue must still produce segments"
        );
        assert!(truncated[0].truncated);
        assert!(
            truncated[0].duration_ms() <= 400,
            "got {} ms",
            truncated[0].duration_ms()
        );
    }

    #[test]
    fn cutting_a_monologue_loses_no_audio_and_does_not_pause_it() {
        let config = VadConfig {
            max_segment_ms: 320,
            ..fast_config()
        };
        let overlap = (config.pre_roll_ms as usize * TARGET_SAMPLE_RATE as usize) / 1000;
        let mut segmenter = Segmenter::new(config);

        // Forty frames of unbroken speech: well past several cuts.
        let segments: Vec<Segment> = run(&mut segmenter, &[0.9; 40])
            .into_iter()
            .filter_map(|e| match e {
                VadEvent::SegmentReady(s) => Some(s),
                _ => None,
            })
            .collect();

        assert!(
            segments.len() >= 3,
            "expected repeated cuts, got {}",
            segments.len()
        );

        for pair in segments.windows(2) {
            let (first, second) = (&pair[0], &pair[1]);
            assert!(first.truncated && second.truncated);

            // The next chunk picks up exactly where the previous one ended, less the
            // deliberate overlap. Any other value means audio was dropped at the cut —
            // which is what happened when a truncation reset the segmenter to silence.
            assert_eq!(
                second.start_sample,
                first.start_sample + (first.samples.len() - overlap) as u64,
                "audio was lost across the cut"
            );
        }
    }

    #[test]
    fn a_cut_monologue_that_then_stops_still_closes_normally() {
        let config = VadConfig {
            max_segment_ms: 320,
            ..fast_config()
        };
        let mut segmenter = Segmenter::new(config);

        let mut script = vec![0.9; 25];
        script.extend([0.0; 10]);

        let segments: Vec<Segment> = run(&mut segmenter, &script)
            .into_iter()
            .filter_map(|e| match e {
                VadEvent::SegmentReady(s) => Some(s),
                _ => None,
            })
            .collect();

        let last = segments.last().expect("a final segment");
        assert!(
            !last.truncated,
            "the last segment ended on silence, not on the length cap"
        );
    }

    #[test]
    fn flushing_mid_speech_returns_what_was_captured() {
        let mut segmenter = Segmenter::new(fast_config());
        run(&mut segmenter, &[0.9; 6]);

        let segment = segmenter.flush().expect("a segment in progress");
        assert!(!segment.samples.is_empty());
        assert!(
            segmenter.flush().is_none(),
            "nothing is left after flushing"
        );
    }

    #[test]
    fn flushing_during_silence_returns_nothing() {
        let mut segmenter = Segmenter::new(fast_config());
        run(&mut segmenter, &[0.0; 5]);
        assert!(segmenter.flush().is_none());
    }

    #[test]
    fn segments_report_where_they_started_in_the_stream() {
        let mut segmenter = Segmenter::new(fast_config());
        let mut script = vec![0.0; 10];
        script.extend([0.9; 6]);
        script.extend([0.0; 6]);

        let events = run(&mut segmenter, &script);
        let segment = events
            .iter()
            .find_map(|e| match e {
                VadEvent::SegmentReady(s) => Some(s),
                _ => None,
            })
            .expect("a segment");

        // Speech began at frame 10, minus two frames of pre-roll.
        let expected = (10 - 2) * FRAME_SAMPLES as u64;
        assert_eq!(segment.start_sample, expected);
    }

    #[test]
    fn raising_the_threshold_makes_the_segmenter_deafer() {
        let mut strict = Segmenter::new(VadConfig {
            speech_threshold: 0.95,
            ..fast_config()
        });
        let events = run(&mut strict, &[0.9; 20]);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, VadEvent::SegmentReady(_))),
            "probabilities below the threshold must not trigger speech"
        );
    }

    #[test]
    fn reconfiguring_shrinks_the_pre_roll_immediately() {
        let mut segmenter = Segmenter::new(VadConfig {
            pre_roll_ms: 640,
            ..fast_config()
        });
        run(&mut segmenter, &[0.0; 30]);

        segmenter.reconfigure(VadConfig {
            pre_roll_ms: 32,
            ..fast_config()
        });
        assert!(segmenter.pre_roll.len() <= FRAME_SAMPLES);
    }

    #[test]
    fn resetting_clears_everything() {
        let mut segmenter = Segmenter::new(fast_config());
        run(&mut segmenter, &[0.9; 6]);
        segmenter.reset();

        assert!(!segmenter.is_speaking());
        assert!(segmenter.flush().is_none());
    }
}
