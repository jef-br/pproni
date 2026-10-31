//! Prints a BiRefNet ONNX model's input/output tensor names and shapes.
//!
//! Inspects a model *file*, so it can be pointed at an export that is not the embedded one — a
//! re-quantized model, a different side.
//!
//!   ORT_DYLIB_PATH   ONNX Runtime shared library (default: ./onnxruntime.dll)
//!   BIREFNET_ONNX    BiRefNet model to inspect   (default: ./models/birefnet_lite_512.onnx)

use ort::session::Session;

fn main() -> ort::Result<()> {
    let ort_lib = std::env::var("ORT_DYLIB_PATH").unwrap_or_else(|_| "onnxruntime.dll".into());
    // Absolute path, or the OS search finds System32's older onnxruntime.dll first (version-mismatch
    // panic); `join` leaves an absolute `ORT_DYLIB_PATH` as is.
    // See docs/architecture/modules/examples.md.
    let ort_lib = std::env::current_dir()
        .map(|dir| dir.join(&ort_lib))
        .ok()
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or(ort_lib);
    let model_path =
        std::env::var("BIREFNET_ONNX").unwrap_or_else(|_| "models/birefnet_lite_512.onnx".into());

    ort::init_from(ort_lib).commit()?;
    let session = Session::builder()?.commit_from_file(model_path)?;
    println!("Inputs:");
    for input in session.inputs.iter() {
        println!("  {} : {:?}", input.name, input.input_type);
    }
    println!("Outputs:");
    for output in session.outputs.iter() {
        println!("  {} : {:?}", output.name, output.output_type);
    }
    Ok(())
}
