//! Explain what the detector did with a line of narration.
//!
//! ```text
//! cargo run -p dndsound-detect --example why -- "Він вдарив мечем."
//! ```

use dndsound_detect::{seed_events, stem_phrase, DetectionInput, Detector};

fn main() {
    use std::io::BufRead;

    // Lines on stdin, when there are any: a transcript log is far easier to pipe in than
    // to quote on a command line.
    let piped: Vec<String> = if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        Vec::new()
    } else {
        std::io::stdin()
            .lock()
            .lines()
            .map_while(Result::ok)
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    };

    let args: Vec<String> = if piped.is_empty() {
        std::env::args().skip(1).collect()
    } else {
        piped
    };

    let lines: Vec<String> = if args.is_empty() {
        vec![
            "Він підійшов до нього і вдарив мечем.".to_string(),
            "і відкрав двері.".to_string(),
            "Він відчинив двері.".to_string(),
            "б'є мечем".to_string(),
        ]
    } else {
        args
    };

    let detector = Detector::new(seed_events());

    for text in &lines {
        let detection = detector.detect(DetectionInput::final_transcript(text, 0));
        println!("\n{text:?}");
        println!("  normalized: {}", detection.normalized);
        println!(
            "  stemmed:    {}",
            stem_phrase(&detection.normalized).join(" ")
        );

        if detection.candidates.is_empty() {
            println!("  no candidates at all — nothing even looked at this");
        }
        for c in &detection.candidates {
            println!(
                "  {:<24} {:.2}/{:.2} {:?} span={:?} action={:?} accepted={}",
                c.event_id,
                c.confidence,
                c.threshold,
                c.layer,
                c.matched_span,
                c.action_word,
                c.accepted
            );
            if let Some(rejection) = &c.rejection {
                println!("      rejected: {}", rejection.explain());
            }
        }
    }
}
