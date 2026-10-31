//! Headcutting. Finds a human face in a square sub-section of the optimised image, locates the
//! philtrum (the dip between the bottom of the nose and the top of the upper lip), and returns the
//! section with everything above the philtrum removed. The only public entry is [`yxunomei`];
//! [`preflight`] builds the session at startup. The caller gates on EIX and rebuilds `Prepared`.
//!
//! The model is YuNet (OpenCV Zoo, BSD-3-Clause), a tiny face detector with five landmarks per
//! face, embedded encrypted like BiRefNet. Philtrum y = midpoint of the nose y and the mean
//! mouth-corner y. No face (back of head, non-human image, flat colour) or more than one (user's
//! rule: never multiple philtra on one image) → `None`; exactly one → cut. Decisions: see
//! docs/work-state/2026-10-28-headcuts.md. Embedding: see docs/architecture/05-single-exe.md.
//! YuNet's input and outputs: see docs/architecture/modules/core-preprocessor-maria-antonieta.md.

use crate::core::shot_classifier::cipher;
use image::{imageops, RgbImage};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use std::sync::OnceLock;

/// The philtrum weights, encrypted at rest, compiled in.
static ENC_MODEL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/philtrum.onnx.enc"));

// PHILTRUM_KEY / PHILTRUM_NONCE, emitted fresh per build by build.rs.
include!(concat!(env!("OUT_DIR"), "/philtrum_key.rs"));

/// Fixed input square the model runs on. YuNet declares a dynamic input; we pick one size so the
/// anchor counts are known and the preprocessing is a single plain resize. 320 is YuNet's
/// documented sweet spot — fast on CPU, enough resolution for a face that already fills a
/// headcut section.
const INPUT_SIZE: u32 = 320;

/// YuNet default score cutoff. Below this a detection is ignored; faces that miss it are not what
/// the user asked us to cut on, and the back-of-head / non-face cases fall well below it.
const SCORE_THRESHOLD: f32 = 0.6;

/// IoU cutoff for non-max suppression. 0.3 is YuNet's reference default.
const NMS_IOU: f32 = 0.3;

/// Strides the model emits predictions at. `cls_<s>`, `obj_<s>`, `bbox_<s>`, `kps_<s>` live in the
/// graph for each `s` here, in this order.
const STRIDES: [u32; 3] = [8, 16, 32];

/// YuNet's keypoint order inside each `kps_<s>` row of 10 floats.
const KP_NOSE: usize = 2;
const KP_MOUTH_RIGHT: usize = 3;
const KP_MOUTH_LEFT: usize = 4;

/// Shared philtrum-detection session: the decrypted YuNet model wrapped in an ORT `Session`, plus
/// the input name read from the model at load time. All other sizes are fixed by `INPUT_SIZE` and
/// `STRIDES`.
pub struct PhiltrumSession {
    session: Session,
    input_name: String,
}

/// Build (or return the cached) philtrum session. Mirrors `core::model`: `OnceLock` so the small
/// session is paid for once per process, off the batch's critical path when [`preflight`] calls
/// this at startup.
fn session() -> Result<&'static PhiltrumSession, String> {
    static S: OnceLock<Result<PhiltrumSession, String>> = OnceLock::new();
    S.get_or_init(|| {
        let model = cipher::decrypt(ENC_MODEL, &PHILTRUM_KEY, &PHILTRUM_NONCE);
        let session = Session::builder()
            .map_err(|e| format!("philtrum session builder: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("philtrum optimisation level: {e}"))?
            .with_intra_threads(1)
            .map_err(|e| format!("philtrum intra threads: {e}"))?
            .commit_from_memory(&model)
            .map_err(|e| format!("philtrum commit from memory: {e}"))?;

        let input_name = session
            .inputs
            .first()
            .ok_or_else(|| "philtrum model has no inputs".to_owned())?
            .name
            .clone();

        // Sanity-check every output this module reads is actually declared.
        let declared: std::collections::HashSet<&str> =
            session.outputs.iter().map(|o| o.name.as_str()).collect();
        for prefix in ["cls", "obj", "bbox", "kps"] {
            for s in STRIDES {
                let name = format!("{prefix}_{s}");
                if !declared.contains(name.as_str()) {
                    return Err(format!(
                        "philtrum model is missing output '{name}'; is this a YuNet export?"
                    ));
                }
            }
        }

        Ok(PhiltrumSession { session, input_name })
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Build the session now (startup), surfacing a corrupt model as an abort rather than a per-image
/// failure. Called from `core::preflight`.
pub fn preflight() -> Result<(), String> {
    session()?;
    Ok(())
}

/// Headcut on `section`: if exactly one face is detected with a philtrum, return the section
/// cropped so row 0 of the result sits at the philtrum y (everything above is removed); otherwise
/// `None` and the caller leaves the picture alone.
///
/// `section` is a square sub-section of the optimised image (`Prepared::model_input`), sized by
/// the caller from the segmask's top third (in model-input coords). See
/// docs/work-state/2026-10-28-headcuts.md.
pub fn yxunomei(section: &RgbImage) -> Option<RgbImage> {
    let s = session().ok()?;
    let y = detect_philtrum(s, section)?;
    Some(cut_head(section, y))
}

/// One YuNet detection: score, bbox (in model-input coords), five 2-D landmarks.
#[derive(Debug, Clone)]
struct Face {
    score: f32,
    bbox: [f32; 4],
    /// `[right_eye, left_eye, nose, mouth_right, mouth_left]` — YuNet's keypoint order.
    landmarks: [[f32; 2]; 5],
}

/// Run YuNet on `section` and return the philtrum y in `section` coords. `None` when the model
/// returns zero or more than one face (back-of-head, non-face image, or multiple people — the
/// user's rule says never cut in those cases).
fn detect_philtrum(s: &PhiltrumSession, section: &RgbImage) -> Option<u32> {
    let input_side = INPUT_SIZE;
    let (sw, sh) = section.dimensions();

    // Plain resize to the model's fixed square. YuNet is trained on BGR pixels in 0..255 with no
    // normalisation, so after resizing we write pixels straight into the CHW buffer with the
    // channel order swapped (R→2, G→1, B→0).
    let resized = imageops::resize(section, input_side, input_side, imageops::FilterType::Triangle);
    let plane = (input_side * input_side) as usize;
    let mut chw = vec![0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        chw[i] = px[2] as f32; // B
        chw[plane + i] = px[1] as f32; // G
        chw[2 * plane + i] = px[0] as f32; // R
    }

    let input = Tensor::from_array((
        [1usize, 3, input_side as usize, input_side as usize],
        chw,
    ))
    .ok()?;
    let input_values = ort::inputs![s.input_name.as_str() => input].ok()?;
    let outputs = s.session.run(input_values).ok()?;

    // Decode across the three strides. For each stride, read the four tensors by name.
    let mut faces: Vec<Face> = Vec::new();
    let fw = input_side as f32;
    for &stride in &STRIDES {
        let feat = input_side / stride;
        let locations = (feat * feat) as usize;

        let cls = extract(&outputs, &format!("cls_{stride}"))?;
        let obj = extract(&outputs, &format!("obj_{stride}"))?;
        let bbox = extract(&outputs, &format!("bbox_{stride}"))?;
        let kps = extract(&outputs, &format!("kps_{stride}"))?;

        if cls.len() < locations
            || obj.len() < locations
            || bbox.len() < locations * 4
            || kps.len() < locations * 10
        {
            return None;
        }

        for idx in 0..locations {
            // `cls` and `obj` are already sigmoid-ed in the graph.
            let score = cls[idx] * obj[idx];
            if score < SCORE_THRESHOLD {
                continue;
            }
            let col = (idx as u32) % feat;
            let row = (idx as u32) / feat;
            let b = &bbox[idx * 4..idx * 4 + 4];
            let cx = (col as f32 + b[0]) * stride as f32;
            let cy = (row as f32 + b[1]) * stride as f32;
            let w = b[2].exp() * stride as f32;
            let h = b[3].exp() * stride as f32;
            let x1 = (cx - 0.5 * w).clamp(0.0, fw);
            let y1 = (cy - 0.5 * h).clamp(0.0, fw);
            let x2 = (cx + 0.5 * w).clamp(0.0, fw);
            let y2 = (cy + 0.5 * h).clamp(0.0, fw);

            let kp = &kps[idx * 10..idx * 10 + 10];
            let mut landmarks = [[0f32; 2]; 5];
            for j in 0..5 {
                landmarks[j][0] =
                    ((col as f32 + kp[j * 2]) * stride as f32).clamp(0.0, fw);
                landmarks[j][1] =
                    ((row as f32 + kp[j * 2 + 1]) * stride as f32).clamp(0.0, fw);
            }

            faces.push(Face { score, bbox: [x1, y1, x2, y2], landmarks });
        }
    }

    let faces = nms(faces, NMS_IOU);
    if faces.len() != 1 {
        return None;
    }
    let f = &faces[0];

    // Philtrum y = midpoint of nose-y and average mouth-corner-y. YuNet landmarks are
    // [right_eye, left_eye, nose, mouth_right, mouth_left]; nose y and the two mouth y's bracket
    // the philtrum.
    let nose_y = f.landmarks[KP_NOSE][1];
    let mouth_y = (f.landmarks[KP_MOUTH_RIGHT][1] + f.landmarks[KP_MOUTH_LEFT][1]) * 0.5;
    let philtrum_y_model = (nose_y + mouth_y) * 0.5;

    // Guard: the philtrum must sit inside the face bbox.
    if philtrum_y_model < f.bbox[1] || philtrum_y_model > f.bbox[3] {
        return None;
    }

    // Translate back from model-input (input_side tall) to section coords (sh tall).
    let philtrum_y_section = philtrum_y_model * sh as f32 / input_side as f32;
    let y = philtrum_y_section
        .round()
        .clamp(0.0, sh.saturating_sub(1) as f32) as u32;

    // Degenerate cases: cutting at 0 removes nothing, cutting at sh-1 leaves a 1-row strip. Treat
    // both as "no useful cut" and return None, matching the "zero or multiple philtra" rule.
    if y == 0 || y >= sh.saturating_sub(1) {
        return None;
    }
    let _ = sw; // sw unused for the y cut; the x translation is identity (section × 1).

    Some(y)
}

/// Grab a named output as a flat `f32` slice. Returns `None` if the output is missing or not an
/// f32 tensor.
fn extract<'a>(
    outputs: &'a ort::session::SessionOutputs<'_, '_>,
    name: &str,
) -> Option<&'a [f32]> {
    let (_, data) = outputs[name].try_extract_raw_tensor::<f32>().ok()?;
    Some(data)
}

/// Standard IoU-based non-max suppression: sort by score descending, keep the top, suppress any
/// later box whose IoU with the kept one exceeds `iou_threshold`.
fn nms(mut faces: Vec<Face>, iou_threshold: f32) -> Vec<Face> {
    faces.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let n = faces.len();
    let mut suppressed = vec![false; n];
    let mut keep = Vec::new();
    for i in 0..n {
        if suppressed[i] {
            continue;
        }
        keep.push(faces[i].clone());
        for j in (i + 1)..n {
            if suppressed[j] {
                continue;
            }
            if iou(&faces[i].bbox, &faces[j].bbox) >= iou_threshold {
                suppressed[j] = true;
            }
        }
    }
    keep
}

/// Intersection over union of two `[x1, y1, x2, y2]` boxes.
fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = a[2].min(b[2]);
    let y2 = a[3].min(b[3]);
    let iw = (x2 - x1).max(0.0);
    let ih = (y2 - y1).max(0.0);
    let inter = iw * ih;
    let area_a = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Remove all rows above `y`. The returned image keeps rows `[y .. section.height())`, same width.
fn cut_head(section: &RgbImage, y: u32) -> RgbImage {
    let (w, h) = section.dimensions();
    let y = y.min(h.saturating_sub(1));
    let keep_h = h - y;
    imageops::crop_imm(section, 0, y, w, keep_h).to_image()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn cut_head_keeps_rows_from_y_down() {
        let mut img = RgbImage::new(4, 10);
        for y in 0..10 {
            for x in 0..4 {
                img.put_pixel(x, y, Rgb([y as u8, y as u8, y as u8]));
            }
        }
        let cut = cut_head(&img, 3);
        assert_eq!(cut.dimensions(), (4, 7));
        assert_eq!(cut.get_pixel(0, 0), &Rgb([3, 3, 3]));
        assert_eq!(cut.get_pixel(0, 6), &Rgb([9, 9, 9]));
    }

    #[test]
    fn cut_head_at_bottom_leaves_one_row() {
        let img = RgbImage::from_pixel(2, 5, Rgb([7, 7, 7]));
        let cut = cut_head(&img, 4);
        assert_eq!(cut.dimensions(), (2, 1));
    }

    #[test]
    fn nms_keeps_the_higher_score_and_drops_the_overlap() {
        let a = Face { score: 0.9, bbox: [0.0, 0.0, 10.0, 10.0], landmarks: [[0.0; 2]; 5] };
        let b = Face { score: 0.7, bbox: [1.0, 1.0, 11.0, 11.0], landmarks: [[0.0; 2]; 5] };
        let c = Face { score: 0.8, bbox: [50.0, 50.0, 60.0, 60.0], landmarks: [[0.0; 2]; 5] };
        let kept = nms(vec![a, b, c], 0.3);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].score, 0.9);
        assert_eq!(kept[1].score, 0.8);
    }

    #[test]
    fn iou_of_disjoint_boxes_is_zero() {
        assert_eq!(iou(&[0.0, 0.0, 1.0, 1.0], &[5.0, 5.0, 6.0, 6.0]), 0.0);
    }

    #[test]
    fn iou_of_identical_boxes_is_one() {
        assert_eq!(iou(&[0.0, 0.0, 2.0, 2.0], &[0.0, 0.0, 2.0, 2.0]), 1.0);
    }
}
