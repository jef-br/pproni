//! Shot classifier: is the subject touching an image border? Answered from segmentation, not a
//! bounding box, in two passes: [`gate`] (a cheap border-band scan per edge), then [`refine`]
//! (trimap + colour matting at full resolution; only its result decides). See
//! docs/specs/03-shot-classifier.md. Wiring to [`super::classify`], [`route`] and [`plan`]: see
//! docs/architecture/modules/core-shot-classifier.md.

pub mod birefnet;
pub(crate) mod cipher;
pub mod gate;
pub mod geometry;
mod gpu_adapters;
pub mod plan;
pub mod refine;
pub mod route;
pub mod sampling;
pub mod segmentation;
pub mod shotcode;

use super::trace;

pub use birefnet::{BiRefNetConfig, BiRefNetModel};
pub use geometry::Edge;
pub use refine::{EdgeRefinement, RefineParams, RefinementInput};
pub use segmentation::{CroppedModel, Instance, Mask, SegmentationModel};

fn edge_name(e: Edge) -> &'static str {
    match e {
        Edge::Top => "Top",
        Edge::Right => "Right",
        Edge::Bottom => "Bottom",
        Edge::Left => "Left",
    }
}

/// Verdict for one instance: the edges it touches, and whether its mask is worth believing.
#[derive(Debug, Clone, Default)]
pub struct ShotClassification {
    pub touches_edges: Vec<Edge>,
    /// The mask covers too little of the frame to be a subject ([`shotcode::mask_too_small`]).
    /// Measured once, here; [`route`] (crop, not frame) and the debug tag both read it. Never
    /// recompute it elsewhere, or the tag can disagree with the route.
    pub mask_too_small: bool,
    /// One entry per edge pass 1 flagged, kept even when the verdict is "not touching" (for logging
    /// and visual export).
    pub refinements: Vec<EdgeRefinement>,
}

/// Runs both passes for one instance: gate on all four edges, refine only the flagged ones.
pub fn classify_instance(input: &RefinementInput, gate_margin_px: u32, params: RefineParams) -> ShotClassification {
    let proximity = gate::gate(&input.instance.mask, gate_margin_px);
    trace::note(|| format!("gate flagged {:?} (band {gate_margin_px}px)", proximity.near));

    let mut touches_edges = Vec::new();
    let mut refinements = Vec::with_capacity(proximity.near.len());
    for edge in proximity.near {
        let result = refine::refine_edge(input, edge, params);
        if result.touching {
            touches_edges.push(edge);
        }
        refinements.push(result);
    }

    let mask_too_small =
        shotcode::mask_too_small(input.working_image, input.original_image, &input.instance.mask);
    trace::note(|| format!(
        "EIX={} (Top,Right,Bottom,Left), mask_too_small = {mask_too_small}",
        shotcode::eix_bits(&touches_edges)
    ));
    let coverage_parts: Vec<String> = refinements
        .iter()
        .filter_map(|r| r.coverage.map(|c| format!("{}:{c:.4}", edge_name(r.edge))))
        .collect();
    if !coverage_parts.is_empty() {
        trace::note(|| format!(
            "Coverage = {}  (below {:.2} counts as a graze)",
            coverage_parts.join(", "),
            params.graze_ratio_max
        ));
    }

    ShotClassification { touches_edges, mask_too_small, refinements }
}
