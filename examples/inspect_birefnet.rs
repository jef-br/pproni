//! Prints a BiRefNet ONNX model's input/output tensor names and shapes.
//!
//! Inspects a model *file*, so it can be pointed at an export that is not the embedded one — a
//! re-quantized model, a different side. The ONNX Runtime is the exe's own (`core::runtime`).
//!
//!   ORT_DYLIB_PATH   ONNX Runtime shared library, as for the exe (default: the embedded one)
//!   BIREFNET_ONNX    BiRefNet model to inspect   (default: the model `build.rs` embeds)

use ort::session::Session;

/// The file `build.rs` embeds as `MODEL_FILE`. Keep the two in step.
const DEFAULT_MODEL: &str = "models/birefnet_512_nodeform_op19.onnx";

fn main() -> ort::Result<()> {
    let model_path = std::env::var("BIREFNET_ONNX").unwrap_or_else(|_| DEFAULT_MODEL.into());

    // The same call the exe makes. See docs/architecture/modules/examples.md.
    let ort_lib = pproni::core::runtime::onnxruntime_dylib().expect("failed to unpack ONNX Runtime");
    ort::init_from(ort_lib.to_string_lossy().to_string()).commit()?;
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
