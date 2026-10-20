//! Imaging core: load → preprocess → classify → dispatch → export.

pub mod config;
pub mod exporter;
pub mod preprocessor;
pub mod processor;
pub mod runtime;
pub mod shot_classifier;
pub mod trace;

use shot_classifier::{Mask, ShotClassification};
use preprocessor::Prepared;
use std::path::Path;

/// Full per-file pipeline.
pub fn process_file(input: &Path, output: &Path, backup: &Path) -> Result<(), String> {
    let _trace = trace::begin(output);
    let prep = preprocessor::prepare(input)?;
    let classified = classify(&prep);
    // Sample BGC/FGC once per image. We used to only pay for this with the debug marker on;
    // R1's stretch/inpaint split needs the BGC on every image where a gate flagged an edge, so
    // the guard widens to also fire when the classifier reports at least one gated edge. The
    // single sample is still shared by the stage-90 log, the exporter's filename tag, and R1.
    let gated_edges_present = classified
        .as_ref()
        .map(|(c, _)| !c.refinements.is_empty())
        .unwrap_or(false);
    let colors = (trace::debug_enabled() || gated_edges_present)
        .then(|| classified.as_ref().map(|(_, mask)| exporter::colors::sample(&prep.working, mask)))
        .flatten();
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

/// Run the shot classifier: the model sees the optimiser's crop, the edge checks the true-colour image.
///
/// `None` means no verdict — the model is unavailable, or inference failed on this image. That is
/// not an error for the batch: `routes::dispatch` sends a no-verdict image down R3, which frames it
/// safely without claiming to know where the subject is.
///
/// The mask is carried out alongside the verdict: the exporter samples the BGC/FGC debug tags from
/// it and writes it as `<stem>_segmask.jpg` when `PPRONI_DEBUG_TAGS` is set.
fn classify(prep: &Prepared) -> Option<(ShotClassification, Mask)> {
    use crate::core::shot_classifier::{
        classify_instance, CroppedModel, RefineParams, RefinementInput, SegmentationModel,
    };

    // The model is shown only the optimiser's crop; its mask is put back into the full working frame.
    let model = CroppedModel::new(model().ok()?, prep.working.dimensions(), prep.crop);
    let instances = model.segment(&prep.model_input).ok()?;
    let instance = instances.into_iter().next()?;
    trace::note(|| format!("mask covers {:.1}% of the frame", trace::coverage_pct(&instance.mask)));

    let input = RefinementInput {
        working_image: &prep.working,
        original_image: &prep.original,
        instance: &instance,
    };
    let params = RefineParams {
        band_px: config::REFINE_BAND_PX,
        safety_px: config::REFINE_SAFETY_PX,
        context_px: config::REFINE_CONTEXT_PX,
        narrow_band_px: config::REFINE_NARROW_BAND_PX,
        graze_ratio_max: config::GRAZE_RATIO_MAX,
    };
    let class = classify_instance(&input, config::GATE_MARGIN_PX, params);
    Some((class, instance.mask))
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
    use processor::routes::center_and_stretch::compose;
    use processor::routes::center_and_stretch::plan::{Pins, SquarePlan};
    use processor::routes::Route;
    use shot_classifier::shotcode::{eix_bits, NO_VERDICT_EIX};

    if !trace::active() {
        return;
    }

    let subject = shot.and_then(|s| s.subject_bbox_in(prep));
    let plan = shot.and_then(|s| {
        subject.and_then(|subj| {
            SquarePlan::new(subj, config::R1_MARGIN, Pins::from_edges(&s.class.touches_edges))
        })
    });
    let gated: Vec<_> = shot
        .map(|s| s.class.refinements.iter().map(|r| r.edge).collect())
        .unwrap_or_default();
    let touching: Vec<_> = shot.map(|s| s.class.touches_edges.clone()).unwrap_or_default();
    // Per-column stretchable/unstretchable call for the stage-90 strip overlay. Only computed
    // when R1 will actually run — R2/R3 do not stretch, so the strip would be meaningless there.
    let strips: Vec<(shot_classifier::Edge, Vec<bool>)> = match route {
        Route::CenterAndStretch if shot.is_some() => {
            let scale = (prep.original.width() as f32 / prep.working.width().max(1) as f32)
                .max(prep.original.height() as f32 / prep.working.height().max(1) as f32)
                .max(1.0);
            let probe = (config::R1_STRETCH_PROBE_LEN_WORKING_PX as f32 * scale).round().max(1.0) as u32;
            let bgc = shot.and_then(|s| s.bgc);
            gated
                .iter()
                .map(|&e| (e, compose::classify_edge(&prep.original, e, bgc, probe)))
                .collect()
        }
        _ => Vec::new(),
    };

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
        trace::plan_view(&prep.original, subject, plan_tuple, &gated, &touching, &strips)
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
