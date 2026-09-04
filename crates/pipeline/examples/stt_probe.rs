//! Does an initial prompt, or beam search, transcribe fantasy narration better?
//!
//! Answered: a narration-shaped Ukrainian prompt does, a word list does not, beam search
//! makes it worse, and an English prompt is recited back out of silence. The English and
//! beam rows are kept so the rejected options stay measurable rather than remembered.
//!
//! Both are cheap to enable and neither is free, so neither is enabled on a hunch. This
//! runs the same fixtures through every combination and prints the transcript and the
//! time, and the decision is made from what it prints.
//!
//! ```text
//! cargo run --release -p dndsound-pipeline --example stt_probe
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use dndsound_pipeline::{read_wav_16k_mono, SpeechRecognizer, SttConfig};

fn models_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/test-audio")
        .join(name)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = models_dir().join("ggml-large-v3-turbo-q5_0.bin");
    if !model.is_file() {
        eprintln!("skipping: {} is not there", model.display());
        return Ok(());
    }

    let cases = [
        (
            "uk_open_door.wav",
            "uk",
            "Ви повільно відчиняєте старі дерев'яні двері.",
        ),
        (
            "uk_sword.wav",
            "uk",
            "Гоблін дістає меч і різко б'є по тобі.",
        ),
        (
            "en_open_door.wav",
            "en",
            "You slowly push open the old wooden door.",
        ),
        (
            "en_sword.wav",
            "en",
            "The goblin pulls out his sword and swings at you.",
        ),
    ];

    for (name, language, expected) in cases {
        let path = fixture(name);
        if !path.is_file() {
            eprintln!("missing fixture {name}");
            continue;
        }
        let samples = read_wav_16k_mono(&path)?;
        println!("\n=== {name} ===\n  want: {expected:?}");

        for (label, prompt, beam) in [
            ("baseline", None, None),
            ("prompt: word list", Some(prompt_for(language)), None),
            ("prompt: narration", Some(narration_prompt(language)), None),
            ("beam 5", None, Some(5)),
            ("prompt + beam 5", Some(prompt_for(language)), Some(5)),
            // A control. If a prompt this strange changes nothing either, the knob is
            // not connected and "the vocabulary prompt did not help" would be the wrong
            // conclusion to draw from the rows above.
            (
                "CONTROL: shouty",
                Some("HELLO. THIS IS ALL IN CAPITAL LETTERS. EVERY WORD."),
                None,
            ),
        ] {
            let config = SttConfig {
                language: Some(language.to_string()),
                initial_prompt: prompt.map(str::to_string),
                beam_size: beam,
                ..SttConfig::default()
            };
            let recognizer = SpeechRecognizer::load(&model, config)?;

            // One warm-up, then the timed run.
            let _ = recognizer.transcribe(&samples, false)?;
            let started = Instant::now();
            let transcript = recognizer.transcribe(&samples, false)?;
            let elapsed = started.elapsed().as_millis();

            let mark = if transcript.text.trim() == expected {
                "ok "
            } else {
                "DIFF"
            };
            println!(
                "  {mark} {label:<16} {elapsed:>5} ms  {:?}",
                transcript.text
            );
        }
    }

    auto_language_check(&model)?;
    hallucination_check(&model)?;

    Ok(())
}

/// Does the prompt still help when the language is left on automatic?
///
/// It is a different question, because the prompt is written in one language and
/// automatic detection reads the prompt as context. A Ukrainian prompt in front of
/// English audio could push the whole decode into the wrong language, which is the worst
/// failure this pipeline has.
fn auto_language_check(model: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== language on automatic ===");

    for (name, want) in [
        ("uk_sword.wav", "Гоблін дістає меч і різко б'є по тобі."),
        (
            "en_sword.wav",
            "The goblin pulls out his sword and swings at you.",
        ),
    ] {
        let path = fixture(name);
        if !path.is_file() {
            continue;
        }
        let samples = read_wav_16k_mono(&path)?;
        println!("  {name}  want: {want:?}");

        for (label, prompt) in [
            ("no prompt", None),
            (
                "uk narration prompt",
                Some(narration_prompt("uk").to_string()),
            ),
            (
                "both languages",
                Some(format!(
                    "{} {}",
                    narration_prompt("uk"),
                    narration_prompt("en")
                )),
            ),
        ] {
            let config = SttConfig {
                language: None,
                initial_prompt: prompt,
                ..SttConfig::default()
            };
            let recognizer = SpeechRecognizer::load(model, config)?;
            let transcript = recognizer.transcribe(&samples, false)?;
            println!(
                "    {label:<22} lang={:<5} {:?}",
                transcript.language.as_deref().unwrap_or("?"),
                transcript.text
            );
        }
    }
    Ok(())
}

/// The risk a narration-shaped prompt creates, and the reason it is not enabled on the
/// strength of the table above alone.
///
/// The prompt is fed to the decoder as the previous sentence. Given silence, whisper is
/// perfectly capable of deciding the previous sentence simply continued — which would
/// mean the application inventing "Гоблін дістає меч" out of a quiet room and playing a
/// sword. That is worse than any misspelling it fixes.
fn hallucination_check(model: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let silence = fixture("silence.wav");
    if !silence.is_file() {
        eprintln!("no silence fixture; the hallucination check did not run");
        return Ok(());
    }
    let samples = read_wav_16k_mono(&silence)?;

    println!("\n=== silence.wav — what does a prompt invent from nothing? ===");
    for (label, prompt) in [
        ("baseline", None),
        ("prompt: word list", Some(prompt_for("uk"))),
        ("prompt: narration", Some(narration_prompt("uk"))),
    ] {
        let config = SttConfig {
            language: Some("uk".to_string()),
            initial_prompt: prompt.map(str::to_string),
            ..SttConfig::default()
        };
        let recognizer = SpeechRecognizer::load(model, config)?;
        let transcript = recognizer.transcribe(&samples, false)?;
        let trusted = recognizer.is_trustworthy(&transcript);
        println!(
            "  {label:<18} trusted={trusted:<5} no_speech={:.2}  {:?}",
            transcript.no_speech_probability, transcript.text
        );
    }
    Ok(())
}

/// The same vocabulary, written the way a Dungeon Master would say it.
///
/// The prompt is fed to the decoder as the *previous sentence*, so a comma-separated
/// glossary is not the shape whisper is expecting, and the control run shows it follows
/// the shape of a prompt closely. This is the same words as prose.
fn narration_prompt(language: &str) -> &'static str {
    match language {
        "uk" => {
            "Гоблін дістає меч і б'є по тобі. Чарівник кидає вогняну кулю. \
                 Жрець зцілює твої рани. Кидай д20 на ініціативу."
        }
        _ => {
            "The goblin draws his sword and strikes at you. The wizard hurls a fireball. \
              The cleric heals your wounds. Roll a d20 for initiative."
        }
    }
}

/// Words a Dungeon Master says that whisper has no reason to expect.
///
/// An initial prompt is fed to the decoder as if it were the previous sentence, so it
/// biases the vocabulary without constraining it. Too long and it starts steering the
/// content rather than the spelling.
fn prompt_for(language: &str) -> &'static str {
    match language {
        "uk" => {
            "Гоблін, орк, дракон, скелет, чарівник, жрець, паладин, меч, щит, кинджал, \
                 арбалет, заклинання, фаєрбол, зілля, сувій, кубик, д20, ініціатива, \
                 таверна, підземелля."
        }
        _ => {
            "Goblin, orc, dragon, skeleton, wizard, cleric, paladin, longsword, shield, \
              dagger, crossbow, spell, fireball, potion, scroll, d20, initiative, tavern, \
              dungeon."
        }
    }
}
