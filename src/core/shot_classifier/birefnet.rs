//! Real BiRefNet (dichotomous image segmentation) inference, via ONNX Runtime: one foreground mask
//! per image, no boxes or classes. The input/output tensors (side `S` read from the model, not
//! hardcoded) and why the shipped model has no `DeformConv` are on the module page.
//! See docs/architecture/modules/core-shot-classifier-birefnet.md.
//!
//! Where it runs, best first: a real graphics card (most dedicated video memory), then a
//! shared-memory one, then the CPU — see [`BiRefNetModel::build`] and [`BiRefNetModel::backend`].
//!
//! To slot into the gate/refine pipeline unchanged, the mask is wrapped as a single [`Instance`]
//! whose `bbox` is the full image (there's no per-object box) and whose `class_id`/`confidence` are
//! unused placeholders — [`SegmentationModel`] only cares about the mask from here on.

use super::geometry::Rect;
use super::gpu_adapters;
use super::segmentation::{Instance, Mask, SegmentationModel};
use crate::core::trace;
use image::{imageops::FilterType, RgbImage};
use ort::execution_providers::DirectMLExecutionProvider;
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use ort::value::Tensor;
use std::sync::Mutex;

/// The BiRefNet weights, encrypted at rest and compiled into the executable by `build.rs` (from the
/// plaintext its `MODEL_FILE` names in `models/`; a missing one fails the *build*).
///
/// Obfuscation, not secrecy: [`MODEL_KEY`]/[`MODEL_NONCE`] ship in the binary too. [`load`] decrypts
/// into a heap buffer that lives only until ONNX Runtime copies the graph into its session; the
/// plaintext never touches disk. See docs/architecture/05-single-exe.md.
static ENC_MODEL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/model.onnx.enc"));

// MODEL_KEY / MODEL_NONCE, generated fresh for this build by build.rs (never committed).
include!(concat!(env!("OUT_DIR"), "/model_key.rs"));

const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

#[derive(Debug, Clone)]
pub struct BiRefNetConfig {
    /// Threshold applied to the sigmoid output to binarize the mask.
    pub mask_threshold: f32,
}

impl Default for BiRefNetConfig {
    fn default() -> Self {
        BiRefNetConfig { mask_threshold: 0.5 }
    }
}

pub struct BiRefNetModel {
    session: Session,
    config: BiRefNetConfig,
    /// First stage number this model's debug-trace images use — see `core::trace::stage`.
    trace_base: u8,
    /// Where the session runs, for the operator: the graphics card's name, or "CPU".
    backend: String,
    /// Present only on a graphics card: DirectML takes one `run` at a time, so the batch's workers
    /// queue here. The card does one image at a time anyway; the workers' own CPU work (resizing,
    /// the mask upsample, everything after) still overlaps with it.
    gpu_gate: Option<Mutex<()>>,
}

impl BiRefNetModel {
    /// Points `ort` at the ONNX Runtime shared library, once per process.
    ///
    /// The path is checked before `ort` is touched: `ort::init_from(..).commit()` *panics* rather
    /// than returning `Err` when the dylib is missing, so without this the operator gets a stack
    /// trace instead of a sentence naming the file. The path is `core::runtime`'s temp copy of the
    /// embedded `onnxruntime.dll` (see `Cargo.toml`'s `ort` entry); this function goes once a static
    /// ORT >= 1.22 exists. See docs/architecture/05-single-exe.md.
    pub fn init_runtime(onnxruntime_dylib_path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let dylib = onnxruntime_dylib_path.as_ref();
        if !dylib.is_file() {
            return Err(format!("ONNX Runtime not found at {}", dylib.display()));
        }
        ort::init_from(dylib.to_string_lossy().to_string())
            .commit()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Loads the embedded model.
    ///
    /// There is nothing to find on disk: the weights are [`ENC_MODEL`], compiled in. They are
    /// decrypted into a heap buffer that ORT copies into its session, then dropped — the plaintext
    /// model never lands on disk. Failure here means the runtime cannot build a session from that
    /// graph, which is fatal for the run — see `core::preflight`. Call [`Self::init_runtime`] first.
    pub fn load(config: BiRefNetConfig) -> Result<Self, String> {
        let model = super::cipher::decrypt(ENC_MODEL, &MODEL_KEY, &MODEL_NONCE);
        Self::build(config, trace::stage::PRIMARY, |b| b.commit_from_memory(&model))
    }

    /// Loads a model from disk instead of the embedded one.
    ///
    /// For the `examples/` dev harness, which exists to run *other* exports — a re-quantized model,
    /// a different side — against the same session settings. The shipped binary never calls this.
    pub fn load_from_file(
        model_path: impl AsRef<std::path::Path>,
        config: BiRefNetConfig,
    ) -> Result<Self, String> {
        let model = model_path.as_ref();
        if !model.is_file() {
            return Err(format!("BiRefNet model not found at {}", model.display()));
        }
        Self::build(config, trace::stage::PRIMARY, |b| b.commit_from_file(model))
    }

    /// Picks where the model runs, best first: graphics cards in order of dedicated video memory
    /// (a real card before a shared-memory one), then the CPU.
    ///
    /// A card counts as soon as a session starts on it; nothing is run to prove it works. If every
    /// card fails to start, or there is none, the CPU session is built; only a failure of that one
    /// is an error.
    fn build(
        config: BiRefNetConfig,
        trace_base: u8,
        commit: impl Fn(SessionBuilder) -> ort::Result<Session>,
    ) -> Result<Self, String> {
        let adapters = gpu_adapters::ranked();
        let mut last_gpu_error = None;
        for adapter in &adapters {
            match Self::session(Some(adapter.device_id), &commit) {
                Ok(session) => {
                    return Ok(BiRefNetModel {
                        session,
                        config,
                        trace_base,
                        backend: adapter.name.clone(),
                        gpu_gate: Some(Mutex::new(())),
                    });
                }
                Err(e) => last_gpu_error = Some(format!("{}: {e}", adapter.name)),
            }
        }

        let session = Self::session(None, &commit).map_err(|e| e.to_string())?;
        let backend = match last_gpu_error {
            Some(why) => format!("CPU (no graphics card worked — {why})"),
            None => "CPU (no graphics card found)".to_owned(),
        };
        Ok(BiRefNetModel { session, config, trace_base, backend, gpu_gate: None })
    }

    /// One session: on the graphics card `device_id` names, or on the CPU when `None`.
    fn session(
        device_id: Option<i32>,
        commit: &impl Fn(SessionBuilder) -> ort::Result<Session>,
    ) -> ort::Result<Session> {
        // One intra-op thread on purpose: `app::batch` fans images across cores with rayon, so the
        // parallelism lives one level up. Peak memory (~400MB per worker) tracks that fan-out, not
        // the optimization level. See docs/architecture/modules/core-shot-classifier-birefnet.md.
        let mut builder = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_memory_pattern(false)?
            .with_intra_threads(1)?;
        if let Some(id) = device_id {
            builder = builder
                // DirectML's graph fusion is switched off: with it on, the NVIDIA card hung ("device
                // removed") on the first run of this model; with it off, NVIDIA and Intel both run
                // it correctly, and NVIDIA is as fast as before.
                .with_config_entry("ep.dml.disable_graph_fusion", "1")?
                // `error_on_failure`: a card that cannot be set up must be an `Err` here so the
                // next candidate is tried, not a silent drop to the CPU inside the session.
                .with_execution_providers([DirectMLExecutionProvider::default()
                    .with_device_id(id)
                    .build()
                    .error_on_failure()])?;
        }
        commit(builder)
    }

    /// Where this model runs: the graphics card's name, or "CPU" (with the reason when a card
    /// was tried and rejected).
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Reads the model's declared NCHW input side — see [`input_dims`].
    fn input_size(&self) -> Result<(u32, u32), String> {
        input_dims(&self.session)
    }
}

/// Reads a session's declared NCHW input side. Dynamic axes come back as `-1`, which would
/// otherwise become a ~4-billion-pixel allocation, so they are rejected here with a message
/// naming the fix rather than dying in the allocator.
fn input_dims(session: &Session) -> Result<(u32, u32), String> {
    let dims = match &session.inputs[0].input_type {
        ort::value::ValueType::Tensor { dimensions, .. } => dimensions,
        other => return Err(format!("unexpected BiRefNet input type: {other:?}")),
    };
    if dims.len() != 4 {
        return Err(format!("expected a 4-D NCHW input, got {} dims", dims.len()));
    }
    if dims[2] <= 0 || dims[3] <= 0 {
        return Err(format!(
            "model declares a dynamic input size ({}x{}); re-export at a fixed side",
            dims[3], dims[2]
        ));
    }
    Ok((dims[3] as u32, dims[2] as u32)) // NCHW: width, height
}


/// The logit a probability corresponds to — the inverse of a sigmoid.
///
/// A sigmoid is monotonic, so thresholding the raw logit at this answers the same question as
/// thresholding the sigmoid, for one `ln` per image. A threshold outside `(0, 1)` maps to an
/// infinity and keeps the sigmoid answer: at `>= 1` nothing passes, at `<= 0` everything does. At
/// 0.5 the two agree exactly (`sigmoid(0)` is 0.5 with no rounding).
/// See docs/architecture/modules/core-shot-classifier-birefnet.md.
fn logit(p: f32) -> f32 {
    (p / (1.0 - p)).ln()
}

impl SegmentationModel for BiRefNetModel {
    fn segment(&self, image: &RgbImage) -> Result<Vec<Instance>, String> {
        let (in_w, in_h) = self.input_size()?;

        // Plain resize (no letterbox/padding) per BiRefNet's own
        // preprocessing, then ImageNet normalization.
        let resized = image::imageops::resize(image, in_w, in_h, FilterType::Triangle);
        let mut chw = vec![0f32; 3 * (in_h * in_w) as usize];
        let plane = (in_h * in_w) as usize;
        for (i, px) in resized.pixels().enumerate() {
            for c in 0..3 {
                chw[c * plane + i] = (px[c] as f32 / 255.0 - IMAGENET_MEAN[c]) / IMAGENET_STD[c];
            }
        }
        trace::image(self.trace_base + 1, trace::MODEL_STEPS[1], || trace::stretched_chw(&chw, in_w, in_h));
        let input = Tensor::from_array(([1usize, 3, in_h as usize, in_w as usize], chw))
            .map_err(|e| format!("build input tensor: {e}"))?;

        let input_values =
            ort::inputs!["input_image" => input].map_err(|e| format!("build session inputs: {e}"))?;
        // On a graphics card the workers take turns for the run itself; the lock is let go before
        // the output is read, so the mask upsample below overlaps with the next worker's run.
        let outputs = {
            let _turn = self.gpu_gate.as_ref().map(|gate| gate.lock().unwrap_or_else(|e| e.into_inner()));
            self.session.run(input_values)
        }
        .map_err(|e| format!("BiRefNet inference: {e}"))?;

        // Borrow the runtime's own buffer rather than collecting it into a
        // fresh Vec — the output plane is a megabyte of f32 per image, and
        // nothing here needs to own or outlive it.
        let (shape, logit_data) = outputs["output_image"]
            .try_extract_raw_tensor::<f32>()
            .map_err(|e| format!("output_image was not an f32 tensor: {e}"))?;

        // The upsample below slices this a row at a time, so the buffer has
        // to be exactly the plane the model's own metadata declared.
        let plane = (in_w as usize) * (in_h as usize);
        if logit_data.len() != plane {
            return Err(format!(
                "output_image holds {} values, expected {plane} for a {in_w}x{in_h} mask (shape {shape:?})",
                logit_data.len()
            ));
        }

        // Resize the model-resolution mask back to the working image's size, nearest-neighbor on the
        // thresholded result: downstream consumes a binary mask, not a soft alpha. The threshold
        // moves into logit space once and the column map is shared by every row — neither answer
        // varies per pixel.
        trace::image(self.trace_base + 2, trace::MODEL_STEPS[2], || trace::stretched_gray(logit_data, in_w, in_h));
        let (out_w, out_h) = (image.width(), image.height());
        let threshold = logit(self.config.mask_threshold);
        let x_map: Vec<usize> = (0..out_w)
            .map(|ox| {
                (((ox as f32 + 0.5) / out_w as f32 * in_w as f32) as usize)
                    .min(in_w as usize - 1)
            })
            .collect();

        let mut data = vec![0u8; (out_w * out_h) as usize];
        for oy in 0..out_h {
            let my = (((oy as f32 + 0.5) / out_h as f32 * in_h as f32) as usize)
                .min(in_h as usize - 1);
            let src = &logit_data[my * in_w as usize..][..in_w as usize];
            let dst = &mut data[(oy * out_w) as usize..][..out_w as usize];
            for (out, &mx) in dst.iter_mut().zip(&x_map) {
                if src[mx] > threshold {
                    *out = 255;
                }
            }
        }
        let mask = Mask { width: out_w, height: out_h, data };
        trace::image(self.trace_base + 5, trace::MODEL_STEPS[5], || trace::mask_image(&mask));

        // No box/class from this model — report the full image as the
        // bbox so the gate/refine pipeline (which only reads the mask
        // near the border) works unmodified.
        let bbox = Rect::from_bounds(0, 0, out_w.saturating_sub(1), out_h.saturating_sub(1));
        Ok(vec![Instance { mask, bbox, class_id: 0, confidence: 1.0 }])
    }
}
