//! Prints the real input/output signature of the Silero VAD ONNX graph.
//!
//! The model's input and output shapes are verified here rather than assumed. Run with:
//!
//! ```text
//! cargo run -p dndsound-pipeline --example inspect_vad
//! ```

use dndsound_models::ModelStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::var("DNDSOUND_MODEL_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("dndsound-models"));

    let store = ModelStore::new(&dir);
    println!("model directory: {}", dir.display());

    let path = store.ensure("silero-vad-16k", |progress| {
        if progress.downloaded_bytes % (256 * 1024) == 0 {
            println!("  {} bytes", progress.downloaded_bytes);
        }
    })?;
    println!("model file: {}", path.display());
    println!("sha256: {}", store.verify("silero-vad-16k")?);

    let session = ort::session::Session::builder()?.commit_from_file(&path)?;

    println!("\ninputs:");
    for input in session.inputs() {
        println!("  {} : {:?}", input.name(), input.dtype());
    }

    println!("\noutputs:");
    for output in session.outputs() {
        println!("  {} : {:?}", output.name(), output.dtype());
    }

    Ok(())
}
