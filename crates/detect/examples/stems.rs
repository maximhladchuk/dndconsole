//! Print the stem of every word given on the command line.
//!
//! Two words that share a stem need only one entry in a term list; two that do not need
//! both. Guessing which is which is how the coin event ended up matching `монета` and
//! not `монети`.
//!
//! ```text
//! cargo run -p dndsound-detect --example stems -- меч меча мечем мечі
//! ```

fn main() {
    let words: Vec<String> = std::env::args().skip(1).collect();
    if words.is_empty() {
        eprintln!("usage: stems <word> [word...]");
        return;
    }

    let mut by_stem: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for word in &words {
        let stem = dndsound_detect::stem(word);
        println!("{word:<24} -> {stem}");
        by_stem.entry(stem).or_default().push(word.clone());
    }

    println!("\ngroups that need only one entry each:");
    for (stem, group) in by_stem.iter().filter(|(_, g)| g.len() > 1) {
        println!("  {stem:<20} {}", group.join(", "));
    }
}
