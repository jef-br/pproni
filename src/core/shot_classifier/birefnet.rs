//! BiRefNet inference via ONNX Runtime: one foreground mask per image, no boxes or classes. The
//! tensors (side `S` read from the model) and why the shipped model has no `DeformConv`:
//! See docs/architecture/modules/core-shot-classifier-birefnet.md.
//!
//! Runs on a graphics card or the CPU ([`BiRefNetModel::build`]). The mask goes out as one [`Instance`] with a full-image
//! `bbox` and placeholder `class_id`/`confidence`, so the gate/refine pipeline takes it unchanged.

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

/// The BiRefNet weights, encrypted and compiled in by `build.rs` from `models/` (`MODEL_FILE`).
/// Obfuscation, not secrecy: [`MODEL_KEY`]/[`MODEL_NONCE`] ship in the binary too.
/// See docs/architecture/05-single-exe.md.
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
    backend: String,
    /// Only on a graphics card: DirectML takes one `run` at a time, so workers queue here; their
    /// CPU work (resize, mask upsample, everything after) still overlaps.
    gpu_gate: Option<Mutex<()>>,
}

impl BiRefNetModel {
    /// Points `ort` at the ONNX Runtime library (`core::runtime`'s unpacked copy), once per process.
    /// Checks the path first: `ort::init_from(..).commit()` *panics* on a missing dylib instead of
    /// returning `Err`. See docs/architecture/modules/core-shot-classifier-birefnet.md.
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

    /// Loads the embedded [`ENC_MODEL`]; call [`Self::init_runtime`] first. `Err`: no session could
    /// be built from the graph, fatal for the run (see `core::preflight`).
    pub fn load(config: BiRefNetConfig) -> Result<Self, String> {
        let model = super::cipher::decrypt(ENC_MODEL, &MODEL_KEY, &MODEL_NONCE);
        Self::build(config, trace::stage::PRIMARY, |b| b.commit_from_memory(&model))
    }

    /// Loads a model from disk instead, for the `examples/` dev harness (other exports, same session
    /// settings). The shipped binary never calls this.
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

    /// Builds the session on the first graphics card that starts one (by dedicated video memory, a
    /// real card before a shared-memory one), else on the CPU. Nothing is run to prove a card
    /// works; only a CPU failure is an `Err`.
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
        // One intra-op thread on purpose: `app::batch` already fans images across cores.
        // See docs/architecture/modules/core-shot-classifier-birefnet.md.
        let mut builder = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_memory_pattern(false)?
            .with_intra_threads(1)?;
        if let Some(id) = device_id {
            builder = builder
                // Graph fusion off: with it on, the NVIDIA card hung ("device removed") on this
                // model's first run. Off, NVIDIA and Intel both run it correctly, NVIDIA at full speed.
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

    /// Where this model runs, for the operator: the graphics card's name, or "CPU" (with the reason
    /// when a card was tried and rejected).
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Reads the model's declared NCHW input side — see [`input_dims`].
    fn input_size(&self) -> Result<(u32, u32), String> {
        input_dims(&self.session)
    }
}

/// Reads a session's declared NCHW input side. A dynamic axis (`-1`, a ~4-billion-pixel
/// allocation) is an `Err` naming the fix.
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


/// The logit of probability `p` (inverse sigmoid): thresholding raw logits at it gives the sigmoid
/// threshold's answer. Outside `(0, 1)` it is an infinity: `>= 1` passes nothing, `<= 0` everything.
/// See docs/architecture/modules/core-shot-classifier-birefnet.md.
fn logit(p: f32) -> f32 {
    (p / (1.0 - p)).ln()
}

impl SegmentationModel for BiRefNetModel {
    fn segment(&self, image: &RgbImage) -> Result<Vec<Instance>, String> {
        let (in_w, in_h) = self.input_size()?;

        // Plain resize (BiRefNet's own preprocessing), then ImageNet normalization.
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

        // Borrowed, not collected: a megabyte of f32 per image that nothing here outlives.
        let (shape, logit_data) = outputs["output_image"]
            .try_extract_raw_tensor::<f32>()
            .map_err(|e| format!("output_image was not an f32 tensor: {e}"))?;

        // The upsample slices this a row at a time: it must be exactly the declared plane.
        let plane = (in_w as usize) * (in_h as usize);
        if logit_data.len() != plane {
            return Err(format!(
                "output_image holds {} values, expected {plane} for a {in_w}x{in_h} mask (shape {shape:?})",
                logit_data.len()
            ));
        }

        // Threshold, then nearest-neighbour back to working size: downstream wants a binary mask,
        // not a soft alpha. Threshold (in logit space) and column map are computed once.
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

        // Full-image bbox: the gate/refine pipeline only reads the mask near the border.
        let bbox = Rect::from_bounds(0, 0, out_w.saturating_sub(1), out_h.saturating_sub(1));
        Ok(vec![Instance { mask, bbox, class_id: 0, confidence: 1.0 }])
    }
}
