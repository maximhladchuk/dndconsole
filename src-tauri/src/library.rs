//! Importing sound files into the library.
//!
//! Two modes:
//!
//! * **referenced** — the default. The file stays where the user keeps it and the
//!   database records its path. Nothing is duplicated.
//! * **managed** — the file is copied into the app's own library directory, so moving
//!   or reorganising the original folder cannot break the library.
//!
//! Every path crossing this boundary is validated: a command argument is untrusted
//! input, and imported files are checked before anything is stored.

use std::path::{Path, PathBuf};

use dndsound_sound::{is_supported, probe, SUPPORTED_EXTENSIONS};
use dndsound_store::sounds::{NewSound, Provenance, Sound};
use dndsound_store::Db;

use crate::error::CommandError;

/// How deep a directory import will walk. Deep enough for the way sound packs are
/// usually organised, shallow enough that pointing at a home directory by mistake
/// cannot take minutes.
const MAX_IMPORT_DEPTH: usize = 6;

pub struct ImportOptions {
    pub managed: bool,
    pub library_dir: PathBuf,
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: Vec<Sound>,
    /// Files that were skipped, each with the reason, so nothing fails silently.
    pub skipped: Vec<SkippedFile>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

/// Import a single file.
pub fn import_file(db: &Db, path: &Path, options: &ImportOptions) -> Result<Sound, CommandError> {
    let source = validate(path)?;

    let metadata = probe(&source)
        .map_err(|e| CommandError::new("unsupportedSound", format!("{}: {e}", display(&source))))?;

    let stored_path = if options.managed {
        copy_into_library(&source, &options.library_dir)?
    } else {
        source.clone()
    };

    let display_name = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .replace(['_', '-'], " ");

    let new = NewSound {
        display_name,
        file_path: stored_path.to_string_lossy().to_string(),
        managed: options.managed,
        format: metadata.format,
        duration_ms: metadata.duration_ms,
        sample_rate: metadata.sample_rate,
        channels: metadata.channels,
        // A file the user already had. The application knows nothing about its terms and
        // must not invent any.
        provenance: Provenance::local(),
    };

    db.sounds().import(&new).map_err(CommandError::from)
}

/// Import every supported file under `dir`.
///
/// Unsupported files are reported as skipped rather than failing the whole import — a
/// sound pack containing a readme should not stop the other 200 files from importing.
pub fn import_directory(
    db: &Db,
    dir: &Path,
    options: &ImportOptions,
) -> Result<ImportReport, CommandError> {
    let dir = dir
        .canonicalize()
        .map_err(|e| CommandError::new("invalidPath", format!("{}: {e}", dir.display())))?;

    if !dir.is_dir() {
        return Err(CommandError::new(
            "invalidPath",
            format!("{} is not a folder.", display(&dir)),
        ));
    }

    let mut report = ImportReport::default();
    let mut files = Vec::new();
    collect_files(&dir, 0, &mut files, &mut report);
    files.sort();

    for file in files {
        match import_file(db, &file, options) {
            Ok(sound) => report.imported.push(sound),
            Err(e) => report.skipped.push(SkippedFile {
                path: display(&file),
                reason: e.message,
            }),
        }
    }

    Ok(report)
}

fn collect_files(dir: &Path, depth: usize, files: &mut Vec<PathBuf>, report: &mut ImportReport) {
    if depth > MAX_IMPORT_DEPTH {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            report.skipped.push(SkippedFile {
                path: display(dir),
                reason: format!("could not read folder: {e}"),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        // Do not follow symlinks: an import must not be able to walk out of the folder
        // the user actually chose.
        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        if metadata.is_dir() && !metadata.is_symlink() {
            collect_files(&path, depth + 1, files, report);
        } else if metadata.is_file() && is_supported(&path) {
            files.push(path);
        }
    }
}

/// Reject anything that is not a real, readable, supported audio file.
fn validate(path: &Path) -> Result<PathBuf, CommandError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| CommandError::new("invalidPath", format!("{}: {e}", path.display())))?;

    if !canonical.is_file() {
        return Err(CommandError::new(
            "invalidPath",
            format!("{} is not a file.", display(&canonical)),
        ));
    }

    if !is_supported(&canonical) {
        return Err(CommandError::new(
            "unsupportedSound",
            format!(
                "{} is not a supported audio format. Supported: {}.",
                display(&canonical),
                SUPPORTED_EXTENSIONS.join(", ")
            ),
        ));
    }

    Ok(canonical)
}

/// Copy a file into the managed library, keeping a readable name and avoiding
/// collisions between identically named files from different folders.
fn copy_into_library(source: &Path, library_dir: &Path) -> Result<PathBuf, CommandError> {
    std::fs::create_dir_all(library_dir).map_err(|e| {
        CommandError::new("io", format!("could not create the library folder: {e}"))
    })?;

    let stem = sanitize(
        source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("sound"),
    );
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav")
        .to_lowercase();

    // Suffix with a hash of the full source path so two different `door.wav` files do
    // not overwrite each other, while re-importing the same file stays idempotent.
    let suffix = short_hash(&source.to_string_lossy());
    let target = library_dir.join(format!("{stem}-{suffix}.{extension}"));

    if !target.exists() {
        std::fs::copy(source, &target).map_err(|e| {
            CommandError::new(
                "io",
                format!("could not copy {} into the library: {e}", display(source)),
            )
        })?;
    }

    Ok(target)
}

pub(crate) fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    let result = if trimmed.is_empty() { "sound" } else { trimmed };
    result.chars().take(64).collect()
}

/// FNV-1a. Short, stable, and not security-sensitive — it only has to keep filenames apart.
pub(crate) fn short_hash(input: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{:08x}", hash as u32)
}

fn display(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev_sounds() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/dev-sounds")
            .canonicalize()
            .expect("dev sounds exist")
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dndsound-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn sanitize_keeps_names_readable_and_safe() {
        assert_eq!(sanitize("door_wood_creak_01"), "door_wood_creak_01");
        assert_eq!(sanitize("../../etc/passwd"), "etc_passwd");
        assert_eq!(sanitize("  "), "sound");
        assert_eq!(sanitize(""), "sound");
        assert!(!sanitize("a/b\\c:d").contains(['/', '\\', ':']));
        assert!(sanitize(&"x".repeat(500)).len() <= 64);
    }

    #[test]
    fn the_hash_is_stable_and_distinguishes_paths() {
        assert_eq!(short_hash("/a/door.wav"), short_hash("/a/door.wav"));
        assert_ne!(short_hash("/a/door.wav"), short_hash("/b/door.wav"));
        assert_eq!(short_hash("/a/door.wav").len(), 8);
    }

    #[test]
    fn validation_rejects_missing_unsupported_and_directories() {
        let missing = dev_sounds().join("nope.wav");
        assert_eq!(validate(&missing).expect_err("missing").kind, "invalidPath");

        let dir = dev_sounds();
        assert_eq!(validate(&dir).expect_err("directory").kind, "invalidPath");

        let readme = dev_sounds().join("README.md");
        assert_eq!(
            validate(&readme).expect_err("unsupported").kind,
            "unsupportedSound"
        );
    }

    #[test]
    fn validation_accepts_a_real_sound_file() {
        let path = dev_sounds().join("door_wood_creak_01.wav");
        assert_eq!(validate(&path).expect("valid"), path);
    }

    #[test]
    fn importing_a_file_records_its_probed_metadata() {
        let db = Db::open_in_memory().expect("db");
        let options = ImportOptions {
            managed: false,
            library_dir: temp_dir("referenced"),
        };

        let sound = import_file(&db, &dev_sounds().join("door_wood_creak_01.wav"), &options)
            .expect("import");

        assert_eq!(sound.display_name, "door wood creak 01");
        assert_eq!(sound.format, "wav");
        assert_eq!(sound.sample_rate, Some(44_100));
        assert!(!sound.managed);
        assert!(sound.duration_ms.unwrap_or(0) > 1_000);
        // Referenced import must not copy the file anywhere.
        assert_eq!(
            sound.file_path,
            dev_sounds()
                .join("door_wood_creak_01.wav")
                .to_string_lossy()
        );
    }

    #[test]
    fn managed_import_copies_the_file_into_the_library() {
        let db = Db::open_in_memory().expect("db");
        let library = temp_dir("managed");
        let options = ImportOptions {
            managed: true,
            library_dir: library.clone(),
        };

        let sound =
            import_file(&db, &dev_sounds().join("thunder_01.wav"), &options).expect("import");

        assert!(sound.managed);
        let stored = PathBuf::from(&sound.file_path);
        assert!(
            stored.starts_with(&library),
            "{stored:?} should be inside the library"
        );
        assert!(stored.is_file(), "the copy should exist on disk");

        // Re-importing the same source is idempotent: one row, one copy.
        import_file(&db, &dev_sounds().join("thunder_01.wav"), &options).expect("reimport");
        assert_eq!(db.sounds().count().expect("count"), 1);

        std::fs::remove_dir_all(&library).ok();
    }

    #[test]
    fn directory_import_takes_the_audio_and_reports_the_rest() {
        let db = Db::open_in_memory().expect("db");
        let options = ImportOptions {
            managed: false,
            library_dir: temp_dir("dir"),
        };

        let report = import_directory(&db, &dev_sounds(), &options).expect("import");

        assert_eq!(report.imported.len(), 10, "all ten dev sounds");
        assert!(
            report.skipped.is_empty(),
            "non-audio files are filtered before import, not skipped: {:?}",
            report.skipped
        );
        assert_eq!(db.sounds().count().expect("count"), 10);
    }

    #[test]
    fn importing_a_file_that_is_not_audio_reports_a_clear_reason() {
        let db = Db::open_in_memory().expect("db");
        let dir = temp_dir("garbage");
        let path = dir.join("broken.wav");
        std::fs::write(&path, b"not audio at all").expect("write");

        let err = import_file(
            &db,
            &path,
            &ImportOptions {
                managed: false,
                library_dir: dir.clone(),
            },
        )
        .expect_err("should fail");

        assert_eq!(err.kind, "unsupportedSound");
        assert!(err.message.contains("broken.wav"), "got {}", err.message);

        std::fs::remove_dir_all(&dir).ok();
    }
}
