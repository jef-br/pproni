//! Pass 1: the cheap gate.
//!
//! Scans only a `margin_px`-wide band along each edge of the mask, so it
//! stays fast across a batch. A hit means "worth running pass 2 for this
//! edge", not "touches": the working-resolution, upsampled mask isn't
//! trusted for the verdict. See docs/specs/03-shot-classifier.md.

use super::geometry::{Edge, EdgeView, Grid};
use super::segmentation::Mask;

#[derive(Debug, Clone, Default)]
pub struct EdgeProximity {
    pub near: Vec<Edge>,
}

impl EdgeProximity {
    pub fn any(&self) -> bool {
        !self.near.is_empty()
    }
}

pub fn gate(mask: &Mask, margin_px: u32) -> EdgeProximity {
    let mut near = Vec::new();
    for edge in Edge::ALL {
        let view = EdgeView::new(mask, edge);
        let band_h = margin_px.min(view.height());
        let found = (0..band_h).any(|cy| (0..view.width()).any(|cx| view.get(cx, cy)));
        if found {
            near.push(edge);
        }
    }
    EdgeProximity { near }
}
