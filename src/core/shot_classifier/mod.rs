//! Shot classifier: is the subject touching an image border? Answered from segmentation, not a
//! bounding box, in two passes:
//! - **Pass 1** ([`gate`]): a cheap per-edge scan of the mask's border band, which keeps pass 2
//!   off the common case (subject nowhere near an edge).
//! - **Pass 2** ([`refine`]): for each flagged edge, trimap + colour matting at full original
//!   resolution. Only its refined result decides touching or not.
//!
//! [`segmentation::SegmentationModel`] lets gate/refine run against a stand-in model;
//! [`super::classify`] wires the real BiRefNet up and feeds [`classify_instance`] the
//! preprocessor's two resolutions. Last, [`route`] reads the verdict, picks the route and sends
//! the image to it; [`plan`] is the square R1 is told to cut, which the choice of route depends on.
//! See docs/specs/03-shot-classifier.md.

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

/// Full verdict for one detected instance: which edges, if any, it
/// actually touches, and whether the mask behind that answer is worth
/// believing at all.
#[derive(Debug, Clone, Default)]
pub struct ShotClassification {
    pub touches_edges: Vec<Edge>,
    /// The mask covers too little of the frame to be a subject — see [`shotcode::mask_too_small`].
    ///
    /// Measured here, next to the mask it is measured from, and consumed in two places: [`route`]
    /// sends it to the route that crops rather than the one that frames, and the debug tag reports
    /// it. Neither recomputes it, so the tag can never disagree with the route.
    pub mask_too_small: bool,
    /// Per-edge refinement detail, for edges pass 1 flagged as worth
    /// checking — useful for logging or visual export even when the
    /// verdict came back "not touching".
    pub refinements: Vec<EdgeRefinement>,
}

/// Runs both passes for one instance: gates on all four edges, then
/// refines only the ones the gate flagged.
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
