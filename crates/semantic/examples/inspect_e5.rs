//! Prints the real input/output signature of the embedding model.
//!
//! Same reason as the VAD inspector: a wrong assumption about a graph's inputs does not
//! error, it silently produces garbage.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/test-models/multilingual-e5-small-int8.onnx".to_string());

    let session = ort::session::Session::builder()?.commit_from_file(&path)?;

    println!("inputs:");
    for input in session.inputs() {
        println!("  {} : {:?}", input.name(), input.dtype());
    }
    println!("outputs:");
    for output in session.outputs() {
        println!("  {} : {:?}", output.name(), output.dtype());
    }
    Ok(())
}
