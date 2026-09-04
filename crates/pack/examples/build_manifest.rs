//! Resolve the catalog against Freesound and write `manifest.json`.
//!
//! ```text
//! FREESOUND_API_KEY=… cargo run -p dndsound-pack --example build_manifest
//! ```
//!
//! Refuses to write anything that is not CC0, and refuses to write at all if any id
//! failed to resolve: a manifest with holes is worse than the previous complete one.

use std::path::Path;
use std::time::Duration;

use dndsound_freesound::{Client, License};
use dndsound_pack::manifest::{Manifest, ManifestSound};
use dndsound_pack::THEMES;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("FREESOUND_API_KEY")
        .map_err(|_| "FREESOUND_API_KEY is not set; the generator needs one")?;
    let client = Client::new(key);

    let mut sounds = Vec::new();
    let mut problems = Vec::new();

    for theme in THEMES {
        for &id in theme.sound_ids {
            match resolve(&client, id) {
                Ok(sound) => {
                    if sound.license != License::Cc0 {
                        problems.push(format!(
                            "{id} ({}) is {:?}, not CC0 — remove it from the catalog",
                            sound.name, sound.license
                        ));
                        continue;
                    }
                    println!(
                        "  {:<22} {:>7}  {:>5.1}s  {}",
                        theme.event_id, id, sound.duration_s, sound.name
                    );
                    sounds.push(ManifestSound {
                        id,
                        event_id: theme.event_id.to_string(),
                        name: sound.name,
                        author: sound.username,
                        duration_s: sound.duration_s,
                        preview_url: sound.preview_url,
                        license_url: "http://creativecommons.org/publicdomain/zero/1.0/"
                            .to_string(),
                    });
                }
                Err(e) => problems.push(format!("{id}: {e}")),
            }
        }
    }

    if !problems.is_empty() {
        eprintln!("\nrefusing to write the manifest:");
        for p in &problems {
            eprintln!("  {p}");
        }
        std::process::exit(1);
    }

    let manifest = Manifest {
        generated: today(),
        sounds,
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("manifest.json");
    std::fs::write(&path, serde_json::to_string_pretty(&manifest)?)?;
    println!(
        "\nwrote {} sounds (~{} kB) to {}",
        manifest.sounds.len(),
        manifest.total_bytes_estimate() / 1024,
        path.display()
    );
    Ok(())
}

/// Fetch one sound, respecting the rate limit.
///
/// Freesound allows 60 requests a minute. The catalog is larger than that, so the
/// generator paces itself and backs off when told to rather than reporting a rate limit
/// as if the sound did not exist.
fn resolve(
    client: &Client,
    id: u64,
) -> Result<dndsound_freesound::Sound, dndsound_freesound::Error> {
    const PACING: Duration = Duration::from_millis(1_100);
    const BACKOFF: Duration = Duration::from_secs(30);

    for attempt in 0..4 {
        std::thread::sleep(PACING);
        match client.sound(id) {
            Err(dndsound_freesound::Error::RateLimited) => {
                eprintln!(
                    "  rate limited on {id}, waiting {BACKOFF:?} (attempt {})",
                    attempt + 1
                );
                std::thread::sleep(BACKOFF);
            }
            other => return other,
        }
    }
    client.sound(id)
}

/// ISO date without pulling in a date crate for one line.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    // Civil-from-days, Howard Hinnant's algorithm.
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
