//! Local speech recognition with whisper.cpp.
//!
//! Whisper is not a streaming model: it transcribes a window of audio in one go. The
//! pipeline gets near-streaming behaviour by re-decoding the segment-so-far while the
//! Dungeon Master is still speaking, then decoding once more when the segment closes.
//!
//! Two things keep that affordable, and both matter more than they look:
//!
//! * **`audio_ctx` trimming.** Whisper always pads its input to 30 seconds internally.
//!   Telling it how much audio there really is turns a 2-second utterance from a
//!   30-second encode into a 2-second one.
//! * **Greedy decoding, single segment, no temperature fallback.** Fallback re-decodes
//!   the same audio several times at rising temperatures. Good for transcript quality on
//!   a podcast, ruinous for a 500 ms latency budget.

use std::path::Path;
use std::time::Instant;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::resample::TARGET_SAMPLE_RATE;
use crate::{Error, Result};

/// Whisper's encoder works on 30-second windows split into 1500 frames.
const FRAMES_PER_30S: f32 = 1500.0;

/// See [`SttConfig::audio_context_floor`].
///
/// Measured on this machine by `examples/ctx_probe.rs`. At 128 frames "б'є мечем" came
/// back as an endless "бі-мечем, бі-мечем, ..."; at 256 as "Бі-м-чем."; at 512 as
/// "Б'є мечем." 768 is both slower and no better. See docs/PERFORMANCE.md.
const DEFAULT_AUDIO_CONTEXT_FLOOR: i32 = 512;

/// Whisper hallucinates fluent nonsense from silence and background noise. These are the
/// phrases it invents most often — subtitle credits from its training data. The VAD is
/// the first line of defence; this is the second.
const HALLUCINATIONS: &[&str] = &[
    "субтитри створив",
    "субтитри від",
    "редактор субтитрів",
    "переклад субтитрів",
    "thank you for watching",
    "thanks for watching",
    "subtitles by",
    "amara.org",
    "продолжение следует",
    "субтитры сделал",
    "поделитесь этим видео",
];

/// Hallucinations that are only hallucinations when they are the *entire* transcript.
///
/// A wider `audio_ctx` gives whisper more room to invent, and what it invents from
/// silence is short and polite: a bare "Thank you." with a no-speech probability of
/// 0.00, which no threshold will catch. Substring matching cannot be used for these —
/// "дякую" is a perfectly ordinary thing for a Dungeon Master to narrate. Standing alone
/// as a whole segment, it is whisper talking to itself.
const STANDALONE_HALLUCINATIONS: &[&str] = &[
    "thank you",
    "thanks",
    "thank you very much",
    "you",
    "bye",
    "goodbye",
    "дякую",
    "спасибо",
    "спасибі",
    "продолжение следует",
    "субтитри",
    "субтитры",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    /// Language whisper decided on, when it was left to detect one.
    pub language: Option<String>,
    /// Highest per-segment probability that the audio was not speech at all.
    pub no_speech_probability: f32,
    /// Wall-clock time the decode took. Measured, never estimated.
    pub elapsed_ms: u32,
    /// True when this came from a partial re-decode of speech still in progress.
    pub is_partial: bool,
}

impl Transcript {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SttConfig {
    /// `None` asks whisper to detect the language per segment, which is what makes
    /// code-switched narration work.
    pub language: Option<String>,
    /// Which languages automatic detection is allowed to choose between.
    ///
    /// Whisper's own detector ranges over all 99 languages it knows, and on a short
    /// segment of Ukrainian narration it very often answers `ru`. That is not a cosmetic
    /// error: the decode is conditioned on the language token, so the wrong answer
    /// produces Russian-shaped nonsense — "б'є мечем" came back as "Бъде мачам" — and no
    /// phrase in any event will ever match it.
    ///
    /// Restricting the choice to the languages this application actually supports turns
    /// a 99-way guess into a 2-way one. Empty means "no restriction", which is whisper's
    /// stock behaviour.
    pub auto_languages: Vec<String>,
    pub threads: u16,
    /// Above this, a transcript is treated as noise rather than speech.
    pub no_speech_threshold: f32,
    /// Trim the encoder context to the real audio length.
    pub trim_audio_context: bool,
    /// Smallest encoder context trimming is allowed to ask for, in frames.
    ///
    /// Trimming hard is the single biggest speed win available, but below a certain
    /// width whisper stops producing usable Ukrainian: words come out misspelled and
    /// short utterances collapse into a repetition loop. This floor is where that
    /// stops, measured — see `docs/PERFORMANCE.md`.
    pub audio_context_floor: i32,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            language: None,
            auto_languages: vec!["uk".to_string(), "en".to_string()],
            // Performance cores only: the efficiency cores make whisper slower, not
            // faster, because the work is split evenly and then waits on the stragglers.
            threads: 8,
            no_speech_threshold: 0.6,
            trim_audio_context: true,
            audio_context_floor: DEFAULT_AUDIO_CONTEXT_FLOOR,
        }
    }
}

pub struct SpeechRecognizer {
    context: WhisperContext,
    config: SttConfig,
}

impl SpeechRecognizer {
    /// Load a ggml model. Expensive — do it once, at session start.
    pub fn load(model_path: impl AsRef<Path>, config: SttConfig) -> Result<Self> {
        let path = model_path.as_ref();
        let path_str = path.to_string_lossy().to_string();

        let started = Instant::now();
        let context =
            WhisperContext::new_with_params(&path_str, WhisperContextParameters::default())
                .map_err(|e| Error::Stt(format!("could not load {}: {e}", path.display())))?;

        tracing::info!(
            model = %path.display(),
            load_ms = started.elapsed().as_millis(),
            multilingual = context.is_multilingual(),
            "speech model loaded"
        );

        Ok(Self { context, config })
    }

    pub fn config(&self) -> &SttConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: SttConfig) {
        self.config = config;
    }

    pub fn is_multilingual(&self) -> bool {
        self.context.is_multilingual()
    }

    /// Transcribe 16 kHz mono audio.
    pub fn transcribe(&self, samples: &[f32], is_partial: bool) -> Result<Transcript> {
        if samples.is_empty() {
            return Err(Error::Stt("nothing to transcribe".to_string()));
        }

        let started = Instant::now();

        let mut state = self
            .context
            .create_state()
            .map_err(|e| Error::Stt(e.to_string()))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(i32::from(self.config.threads));
        params.set_translate(false);
        params.set_no_context(true);
        params.set_single_segment(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Non-speech tokens are noise for our purpose: we want words, not "(door creaks)".
        params.set_suppress_nst(true);
        // No temperature fallback: it re-decodes the same audio repeatedly.
        params.set_temperature(0.0);
        params.set_temperature_inc(0.0);

        // "auto" makes whisper detect the language as part of decoding. Note that
        // `set_detect_language(true)` is NOT the way to do this: it tells whisper.cpp to
        // detect the language and return *without decoding*, which yields a language and
        // an empty transcript.
        //
        // When the user has not pinned a language, `restricted_language` narrows the
        // stock 99-way detection to the languages this application supports, and the
        // decode is then conditioned on that answer rather than on whisper's.
        let forced = match self.config.language.as_deref() {
            Some(language) => Some(language.to_string()),
            None => self.restricted_language(&mut state, samples),
        };
        params.set_language(Some(forced.as_deref().unwrap_or("auto")));

        if self.config.trim_audio_context {
            params.set_audio_ctx(audio_context_for(
                samples.len(),
                self.config.audio_context_floor,
            ));
        }

        state
            .full(params, samples)
            .map_err(|e| Error::Stt(e.to_string()))?;

        let mut text = String::new();
        let mut no_speech_probability: f32 = 0.0;

        for index in 0..state.full_n_segments() {
            let Some(segment) = state.get_segment(index) else {
                continue;
            };
            no_speech_probability = no_speech_probability.max(segment.no_speech_probability());
            if let Ok(chunk) = segment.to_str_lossy() {
                text.push_str(&chunk);
            }
        }

        let language = language_of(&state);
        let text = collapse_repetition(&clean(&text));

        Ok(Transcript {
            text,
            language,
            no_speech_probability,
            elapsed_ms: started.elapsed().as_millis() as u32,
            is_partial,
        })
    }

    /// Detect the spoken language, choosing only between `auto_languages`.
    ///
    /// Returns `None` when no restriction is configured, when the model is
    /// English-only, or when detection fails — in every case the caller falls back to
    /// whisper's own detection, which is a degradation rather than a failure.
    ///
    /// This costs one extra encoder pass over the segment. See `docs/PERFORMANCE.md` for
    /// what that measures at; it is the reason the session runs detection on the small
    /// model and reuses the answer for the turbo decode.
    fn restricted_language(
        &self,
        state: &mut whisper_rs::WhisperState,
        samples: &[f32],
    ) -> Option<String> {
        if self.config.auto_languages.is_empty() || !self.context.is_multilingual() {
            return None;
        }

        let threads = usize::from(self.config.threads).max(1);
        if let Err(error) = state.pcm_to_mel(samples, threads) {
            tracing::warn!(%error, "language detection could not compute the mel spectrogram");
            return None;
        }

        let probabilities = match state.lang_detect(0, threads) {
            Ok((_, probabilities)) => probabilities,
            Err(error) => {
                tracing::warn!(%error, "language detection failed; falling back to whisper's own");
                return None;
            }
        };

        let best = self
            .config
            .auto_languages
            .iter()
            .filter_map(|language| {
                let id = whisper_rs::get_lang_id(language)?;
                let probability = *probabilities.get(id as usize)?;
                Some((language.clone(), probability))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))?;

        tracing::debug!(language = %best.0, probability = best.1, "language detected");
        Some(best.0)
    }

    /// Whether a transcript should be believed.
    ///
    /// Rejecting here rather than downstream keeps the reason attached to the audio that
    /// caused it, which is what Debug Mode needs to explain a missing sound.
    pub fn is_trustworthy(&self, transcript: &Transcript) -> bool {
        has_words(&transcript.text)
            && transcript.no_speech_probability <= self.config.no_speech_threshold
            && !is_hallucination(&transcript.text)
    }
}

/// How many encoder frames this many samples actually need.
///
/// Whisper's encoder covers 30 seconds in 1500 frames. Shorter audio only needs a
/// proportional slice, plus a small margin so the tail is not clipped.
fn audio_context_for(sample_count: usize, floor: i32) -> i32 {
    let seconds = sample_count as f32 / TARGET_SAMPLE_RATE as f32;
    let frames = (seconds / 30.0 * FRAMES_PER_30S).ceil() + 32.0;
    frames.clamp(floor as f32, FRAMES_PER_30S) as i32
}

fn language_of(state: &whisper_rs::WhisperState) -> Option<String> {
    let id = state.full_lang_id_from_state();
    if id < 0 {
        return None;
    }
    whisper_rs::get_lang_str(id).map(str::to_string)
}

/// Collapse whitespace and drop the leading space whisper always emits.
fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How many consecutive repeats of the same block count as a decoding loop rather than
/// as something a Dungeon Master actually said. Three is a plausible flourish —
/// "грім, грім, грім" — so the bar is four.
const REPETITION_LIMIT: usize = 4;

/// The longest phrase a loop is looked for at, in words.
const REPETITION_MAX_PERIOD: usize = 6;

/// Undo whisper's repetition loop.
///
/// Greedy decoding with no temperature fallback occasionally latches onto a phrase and
/// emits it until it runs out of tokens: "б'є мечем, б'є мечем, б'є мечем, ...". A wider
/// `audio_ctx` makes this rare, but rare is not never, and the raw text is poison for
/// detection — it inflates the fuzzy score of whatever it repeats and it fills the debug
/// panel with noise.
///
/// The loop is collapsed rather than rejected, because the repeated block is what was
/// actually said. "б'є мечем" thirty times over becomes "б'є мечем", which is right.
fn collapse_repetition(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();

    // Compare on a key that ignores the punctuation whisper sprinkles between repeats:
    // the loop reads "бі-мечем, бі-мечем" and only the comma differs.
    let keys: Vec<String> = words
        .iter()
        .map(|word| {
            word.to_lowercase()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .collect();

    // The run is searched for at every offset rather than anchored to the end, because
    // whisper stops mid-word when it runs out of tokens: "... після після п". Anchoring
    // on the tail makes that dangling "п" the block being counted, and the loop in front
    // of it goes unnoticed.
    for period in 1..=REPETITION_MAX_PERIOD {
        if words.len() < period * REPETITION_LIMIT {
            continue;
        }

        for start in 0..=words.len() - period * REPETITION_LIMIT {
            let block = &keys[start..start + period];
            let mut repeats = 1;
            let mut end = start + period;
            while end + period <= keys.len() && &keys[end..end + period] == block {
                repeats += 1;
                end += period;
            }

            if repeats >= REPETITION_LIMIT {
                let mut kept: Vec<&str> = words[..start + period].to_vec();
                kept.extend_from_slice(&words[end..]);

                // The first occurrence is the one kept, so it carries the separator that
                // joined it to the next repeat: "бі-мечем," rather than "бі-мечем". A
                // trailing separator at the end of a transcript never means anything.
                let joined = kept.join(" ");
                let joined = joined
                    .trim_end_matches([',', ';', ':', '-', '—'])
                    .to_string();

                // Collapsing can expose a second loop that was hidden behind the first.
                // Each pass strictly shortens the text, so this terminates.
                return collapse_repetition(&joined);
            }
        }
    }

    text.to_string()
}

/// Does the transcript contain actual words?
///
/// Whisper answers silence with punctuation surprisingly often — a bare "!!" or "..." —
/// and reports a *low* no-speech probability while doing it, so that threshold alone
/// does not catch it. Two letters is the cheapest reliable filter.
fn has_words(text: &str) -> bool {
    text.chars().filter(|c| c.is_alphabetic()).count() >= 2
}

/// Does this look like one of whisper's stock hallucinations?
fn is_hallucination(text: &str) -> bool {
    let lowered = text.to_lowercase();

    let bare = lowered
        .trim_matches(|c: char| !c.is_alphanumeric())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if STANDALONE_HALLUCINATIONS.contains(&bare.as_str()) {
        return true;
    }

    HALLUCINATIONS.iter().any(|phrase| lowered.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOOR: i32 = DEFAULT_AUDIO_CONTEXT_FLOOR;

    #[test]
    fn audio_context_shrinks_with_the_audio() {
        let ten_seconds = audio_context_for(TARGET_SAMPLE_RATE as usize * 10, FLOOR);
        let twenty_seconds = audio_context_for(TARGET_SAMPLE_RATE as usize * 20, FLOOR);

        assert!(ten_seconds < twenty_seconds);
        assert!(twenty_seconds < FRAMES_PER_30S as i32);
    }

    #[test]
    fn audio_context_never_goes_below_a_usable_floor() {
        // Short clips are the common case — a single narrated line — and they are also
        // where an aggressive trim destroys the transcript. Measured: below this floor
        // whisper misspells Ukrainian and can fall into a repetition loop.
        assert_eq!(audio_context_for(160, FLOOR), FLOOR);
        assert_eq!(audio_context_for(0, FLOOR), FLOOR);
        assert_eq!(
            audio_context_for(TARGET_SAMPLE_RATE as usize * 2, FLOOR),
            FLOOR
        );
    }

    #[test]
    fn audio_context_is_capped_at_the_full_window() {
        let a_minute = audio_context_for(TARGET_SAMPLE_RATE as usize * 60, FLOOR);
        assert_eq!(a_minute, FRAMES_PER_30S as i32);
    }

    #[test]
    fn a_decoding_loop_is_collapsed_to_what_was_said_once() {
        let looped = "бі-мечем, ".repeat(30) + "бі-мечем";
        assert_eq!(collapse_repetition(&looped), "бі-мечем");

        assert_eq!(
            collapse_repetition("Гримець грім. Гримець грім. Гримець грім. Гримець грім."),
            "Гримець грім."
        );
    }

    #[test]
    fn a_loop_whisper_cut_off_mid_word_is_still_recognised() {
        // Verbatim shape of what turbo produced from English narration while pinned to
        // Ukrainian: a long loop ending in a truncated token.
        let text = "Звісно, ".to_string() + &"після ".repeat(40) + "п";
        assert_eq!(collapse_repetition(&text), "Звісно, після п");
    }

    #[test]
    fn a_loop_that_starts_mid_sentence_keeps_the_part_before_it() {
        let text = "Гоблін нападає і ".to_string() + &"б'є мечем ".repeat(6) + "б'є мечем";
        assert_eq!(collapse_repetition(&text), "Гоблін нападає і б'є мечем");
    }

    #[test]
    fn ordinary_narration_is_left_alone() {
        // Deliberate repetition happens. Three of anything is a flourish, not a loop.
        for text in [
            "Грім, грім, грім!",
            "Ви повільно відчиняєте старі дерев'яні двері.",
            "The goblin pulls out his sword and swings at you.",
            "",
        ] {
            assert_eq!(collapse_repetition(text), text);
        }
    }

    #[test]
    fn cleaning_collapses_whitespace() {
        assert_eq!(clean("  You open   the door. "), "You open the door.");
        assert_eq!(
            clean("\n\tВи\u{00a0}відчиняєте  двері"),
            "Ви відчиняєте двері"
        );
        assert_eq!(clean(""), "");
    }

    #[test]
    fn known_hallucinations_are_recognised_in_both_languages() {
        assert!(is_hallucination("Субтитри створив Дмитро"));
        assert!(is_hallucination("Thank you for watching!"));
        assert!(is_hallucination("Subtitles by the Amara.org community"));
    }

    #[test]
    fn punctuation_only_transcripts_are_not_words() {
        // What whisper actually returns for silence; see the STT integration tests.
        assert!(!has_words("!!"));
        assert!(!has_words("..."));
        assert!(!has_words(" - ? "));
        assert!(!has_words(""));
        assert!(
            !has_words("a"),
            "a single letter is not a word worth acting on"
        );

        assert!(has_words("Ok"));
        assert!(has_words("Ви відчиняєте двері."));
    }

    #[test]
    fn real_narration_is_not_mistaken_for_a_hallucination() {
        assert!(!is_hallucination(
            "You slowly push open the old wooden door."
        ));
        assert!(!is_hallucination(
            "Ви повільно відчиняєте старі дерев'яні двері."
        ));
        assert!(!is_hallucination("The goblin thanks you and leaves."));
        // These are only hallucinations standing alone. Inside a sentence they are
        // ordinary narration and must survive.
        assert!(!is_hallucination(
            "The innkeeper says thank you and pours the ale."
        ));
        assert!(!is_hallucination("Він каже дякую і йде геть."));
    }

    #[test]
    fn a_polite_one_liner_from_silence_is_recognised_as_invention() {
        // Exactly what a wider audio_ctx makes whisper produce from silence, at a
        // no-speech probability of 0.00 — no threshold catches it.
        assert!(is_hallucination("Thank you."));
        assert!(is_hallucination(" thank you "));
        assert!(is_hallucination("Дякую!"));
        assert!(is_hallucination("Спасибо."));
        assert!(is_hallucination("You"));
    }
}
