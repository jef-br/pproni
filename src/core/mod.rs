//! Imaging core: load → preprocess → classify → frame → dispatch → export.

pub mod config;
pub mod exporter;
pub mod framing;
pub mod preprocessor;
pub mod processor;
pub mod runtime;
pub mod shot_classifier;
pub mod trace;

use preprocessor::{Framing, Prepared, Trims};
use shot_classifier::geometry::Rect;
use shot_classifier::{Instance, Mask, ShotClassification};
use std::path::Path;

/// Full per-file pipeline.
pub fn process_file(input: &Path, output: &Path, backup: &Path) -> Result<(), String> {
    let _trace = trace::begin(output);
    let mut prep = preprocessor::prepare(input)?;
    let mut classified = classify(&prep);

    // Framing: read the two trims against the mask the one model run gave, and see whether what trim 1
    // cut off is padding or a frame (see `framing`). Bars on one axis are dropped (one edge touched,
    // smooth content) or centred with padding by R1; a frame is peeled here, and the edge check runs
    // again on the cropped picture — no second model run.
    let decision = classified.as_ref().map(|(class, mask)| {
        let bounds = mask_bounds(mask);
        let texture = preprocessor::optimizer::highpass_texture(&prep.working, prep.trims.first, bounds);
        let (decision, why) = framing::decide(
            prep.working.dimensions(),
            prep.trims.first,
            prep.trims.second,
            class.touches_edges.len(),
            bounds,
            texture,
        );
        trace::note(|| format!("framing: {why}"));
        decision
    });
    match decision {
        Some(framing::Decision::Centre(content)) => prep.framing = Framing::CentreContent(content),
        Some(framing::Decision::Crop(content)) => {
            // The bars go; R1 frames the content like any picture. The verdict is kept as it was (the
            // bars never touched the subject, so the edges it touches do not change, and EIX cannot
            // flip); only the mask moves into the cropped frame.
            if let Some((class, mask)) = classified.take() {
                let mask = mask.cropped(content);
                prep = peel(&prep, content);
                trace::note(|| format!("bars dropped: the picture is now {}x{}", prep.original.width(), prep.original.height()));
                classified = Some((class, mask));
            }
        }
        Some(framing::Decision::Peel(first)) => {
            if let Some((_, mask)) = &classified {
                // Trim 1 can stop inside a frame the product runs into; go on while it is still frame.
                let frame = framing::deepen_peel(&prep.working, mask, first, framing::border_colour(&prep.working));
                trace::note(|| format!("frame peel: trim 1's box {first:?} deepened to {frame:?}"));
                let instance = Instance {
                    mask: mask.cropped(frame),
                    bbox: Rect { x: 0, y: 0, w: frame.w, h: frame.h },
                    class_id: 0,
                    confidence: 1.0,
                };
                prep = peel(&prep, frame);
                trace::note(|| {
                    format!("frame peeled: the picture is now {}x{} (working {}x{})", prep.original.width(), prep.original.height(), prep.working.width(), prep.working.height())
                });
                classified = Some(judge(&prep, instance));
            }
        }
        _ => {}
    }

    // Sample BGC/FGC once per image. R1 holds every pixel it invents to the BGC, so it is needed on
    // every image that has a verdict. The single sample is shared by the stage-90 log, the
    // exporter's filename tag, and R1.
    let colors = classified.as_ref().map(|(_, mask)| exporter::colors::sample(&prep.working, mask));
    let bgc = colors
        .as_ref()
        .and_then(|c| c.bgc.as_deref())
        .and_then(exporter::colors::parse_hex);
    let shot = classified
        .as_ref()
        .map(|(class, mask)| processor::routes::Shot { class, mask, bgc });
    let route = processor::routes::Route::select(shot.as_ref().map(|s| s.class));
    trace_stage_90(&prep, shot.as_ref(), route, colors.as_ref());
    let img = processor::routes::dispatch(&prep, shot);
    let shot = classified
        .as_ref()
        .zip(colors.as_ref())
        .map(|((class, mask), colors)| exporter::ShotInputs {
            working: &prep.working,
            mask,
            class,
            colors,
        });
    exporter::export(&img, output, input, backup, shot)
}

/// The picture cropped to `frame` (`working` coordinates): both resolutions, so R1 and the edge check
/// see the picture without the frame. The model's crop and the trims are spent, and the bar detector's
/// box was in the old coordinates, so they are cleared.
fn peel(prep: &Prepared, frame: Rect) -> Prepared {
    let (ww, wh) = prep.working.dimensions();
    let (ow, oh) = prep.original.dimensions();
    let (fx, fy) = (ow as f32 / ww.max(1) as f32, oh as f32 / wh.max(1) as f32);
    let x0 = ((frame.x as f32 * fx).round() as u32).min(ow.saturating_sub(1));
    let y0 = ((frame.y as f32 * fy).round() as u32).min(oh.saturating_sub(1));
    let x1 = (((frame.x + frame.w) as f32 * fx).round() as u32).clamp(x0 + 1, ow);
    let y1 = (((frame.y + frame.h) as f32 * fy).round() as u32).clamp(y0 + 1, oh);
    let original = image::imageops::crop_imm(&prep.original, x0, y0, x1 - x0, y1 - y0).to_image();
    let working = image::imageops::crop_imm(&prep.working, frame.x, frame.y, frame.w, frame.h).to_image();
    let crop = Rect { x: 0, y: 0, w: working.width(), h: working.height() };
    Prepared {
        original,
        working,
        model_input: image::RgbImage::new(1, 1),
        crop,
        trims: Trims::NONE,
        framing: Framing::Free,
        letterbox: None,
    }
}

/// Run the shot classifier: the model sees the optimiser's crop, the edge checks the true-colour image.
///
/// `None` means no verdict — the model is unavailable, or inference failed on this image. That is
/// not an error for the batch: `routes::dispatch` sends a no-verdict image down R3, which frames it
/// safely without claiming to know where the subject is.
///
/// The model runs once, on trim 1's crop. The mask is carried out alongside the verdict: the exporter
/// samples the BGC/FGC debug tags from it and writes it as `<stem>_segmask.jpg` when
/// `PPRONI_DEBUG_TAGS` is set.
fn classify(prep: &Prepared) -> Option<(ShotClassification, Mask)> {
    use crate::core::shot_classifier::{CroppedModel, SegmentationModel};

    // The model is shown only the optimiser's crop; its mask is put back into the full working frame.
    let found = CroppedModel::new(model().ok()?, prep.working.dimensions(), prep.crop).segment(&prep.model_input);
    let instance = found.ok()?.into_iter().next()?;
    trace::note(|| format!("mask covers {:.1}% of the frame", trace::coverage_pct(&instance.mask)));
    Some(judge(prep, instance))
}

/// The edge verdict for one instance: pass 1 gate, pass 2 refinement. Needs no model.
fn judge(prep: &Prepared, instance: Instance) -> (ShotClassification, Mask) {
    use crate::core::shot_classifier::{classify_instance, RefineParams, RefinementInput};

    let input = RefinementInput {
        working_image: &prep.working,
        original_image: &prep.original,
        instance: &instance,
    };
    let params = RefineParams {
        band_px: config::GATE_MARGIN_PX,
        safety_px: config::REFINE_SAFETY_PX,
        context_px: config::REFINE_CONTEXT_PX,
        narrow_band_px: config::REFINE_NARROW_BAND_PX,
        graze_ratio_max: config::GRAZE_RATIO_MAX,
    };
    let class = classify_instance(&input, config::GATE_MARGIN_PX, params);
    (class, instance.mask)
}

/// The tight box around everything the mask marks, or `None` for an empty mask.
fn mask_bounds(mask: &Mask) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..mask.height {
        for x in 0..mask.width {
            if mask.get(x, y) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (x0 != u32::MAX).then(|| Rect::from_bounds(x0, y0, x1, y1))
}

/// The one shared model instance.
///
/// `batch::run` fans `process_file` across cores with rayon, so this must not load per image — the
/// session is ~180MB. Loaded once on first use; the outcome is cached either way.
fn model() -> Result<&'static shot_classifier::BiRefNetModel, String> {
    use shot_classifier::{BiRefNetConfig, BiRefNetModel};
    use std::sync::OnceLock;

    static MODEL: OnceLock<Result<BiRefNetModel, String>> = OnceLock::new();
    MODEL
        .get_or_init(|| {
            // Weights and runtime are both embedded; the runtime is unpacked to a temp
            // file first because `ort`'s loader needs a path. See `core::runtime`.
            BiRefNetModel::init_runtime(runtime::onnxruntime_dylib()?)?;
            BiRefNetModel::load(BiRefNetConfig::default())
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// Stage 90: "what we know" about the image, from which the route is picked. The same drawing
/// code runs every image; a value that is absent (no bbox, no plan, no overhang) is simply not
/// drawn, and not written to the log. The header names the chosen route so the picture and the
/// routing decision are read together, not from two separate places.
fn trace_stage_90(
    prep: &Prepared,
    shot: Option<&processor::routes::Shot<'_>>,
    route: processor::routes::Route,
    colors: Option<&exporter::colors::ShotColors>,
) {
    use processor::routes::center_and_stretch::plan_square;
    use processor::routes::Route;
    use shot_classifier::shotcode::{eix_bits, NO_VERDICT_EIX};

    if !trace::active() {
        return;
    }

    let subject = shot.and_then(|s| s.subject_bbox_in(prep));
    let plan = shot.and_then(|s| {
        subject.and_then(|subj| plan_square(prep, subj, &s.class.touches_edges))
    });
    let gated: Vec<_> = shot
        .map(|s| s.class.refinements.iter().map(|r| r.edge).collect())
        .unwrap_or_default();
    let touching: Vec<_> = shot.map(|s| s.class.touches_edges.clone()).unwrap_or_default();
    let route_label = match route {
        Route::CenterAndStretch => "[R1 - CenterAndStretch]",
        Route::CropSquare => "[R2 - CropSquare]",
        Route::Fallback => "[R3 - Fallback]",
    };

    let plan_tuple = plan.as_ref().map(|p| (p.x, p.y, p.side));
    let header = format!(
        "{route_label} - bbox (cyan), final canvas (yellow), grey (invented area); red (edge touches) orange (edge gated but cleared)"
    );
    trace::image(trace::stage::R1_PLAN, &header, || {
        trace::plan_view(&prep.original, subject, plan_tuple, &gated, &touching)
    });

    // Line 2: bbox and R1's planned canvas, each shown only if we have it.
    let mut l2 = String::new();
    if let Some(b) = subject {
        l2.push_str(&format!("bbox [x:{}, y:{}, w:{}, h:{}]", b.x, b.y, b.w, b.h));
    }
    if let Some(p) = &plan {
        if !l2.is_empty() {
            l2.push_str(" → ");
        }
        l2.push_str(&format!(
            "canvas: [x:{}, y:{}, w:{}, h:{}]",
            p.x, p.y, p.side, p.side
        ));
    }
    if !l2.is_empty() {
        trace::note(move || l2);
    }

    // Line 3: shot code. EIX is always known; BGC/FGC come from the one sample taken in
    // `process_file` and shared with the exporter's filename tag.
    let eix = shot
        .map(|s| eix_bits(&s.class.touches_edges))
        .unwrap_or_else(|| NO_VERDICT_EIX.to_owned());
    let mut l3 = format!("EIX={eix}");
    if let Some(c) = colors.and_then(|c| c.bgc.as_ref()) {
        l3.push_str(&format!("  BGC={c}"));
    }
    if let Some(c) = colors.and_then(|c| c.fgc.as_ref()) {
        l3.push_str(&format!("  FGC={c}"));
    }
    trace::note(move || l3);

    // Line 4: overhang (only when there is a plan), mask flag and touched edges (only with a verdict).
    let mut l4 = String::new();
    if let Some(p) = &plan {
        let o = p.overhang(prep.original.width(), prep.original.height());
        l4.push_str(&format!(
            "overhang [T:{}, B:{}, L:{}, R:{}]",
            o.top, o.bottom, o.left, o.right
        ));
    }
    if let Some(s) = shot {
        if !l4.is_empty() {
            l4.push_str("  ");
        }
        l4.push_str(&format!(
            "mask_too_small = {}, touches {:?}",
            s.class.mask_too_small, s.class.touches_edges
        ));
    }
    if !l4.is_empty() {
        trace::note(move || l4);
    }
}

/// Check the classifier can actually run, before any image is touched.
///
/// The weights are embedded now, so the only file this can fail to find is `onnxruntime.dll`.
/// That, or a session that will not build, is an environment fault rather than an image outcome:
/// it should stop the run loudly rather than quietly sending every image down the no-verdict
/// route. R3 is for images the classifier looked at and could not judge — not for a broken build.
///
/// Callers run this once at startup and abort on `Err`. It doubles as the point where the ~180MB
/// session is paid for, off the batch's critical path.
pub fn preflight() -> Result<(), String> {
    model()?;
    Ok(())
}

