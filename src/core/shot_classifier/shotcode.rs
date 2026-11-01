//! Shot code: the classifier's side of the per-image tag, the `EIX` (edge intersection) bits.
//! `exporter::colors` samples `BGC`/`FGC` and assembles the full tag.
//!
//! `eix` is always the classifier's real verdict: never let `mask_too_small` override it, or the
//! tag claims an edge intersection `route::decide` didn't route on. A `BG` tag is still to come.
//! See docs/architecture/modules/core-shot-classifier-shotcode.md.

use super::super::config::{MASK_MIN_COVERAGE, MASK_MIN_LONGEST_SIDE_PX};
use super::{Edge, Mask};
use image::RgbImage;

/// EIX when the classifier gave no verdict: four underscores, the width of a TRBL verdict.
pub const NO_VERDICT_EIX: &str = "____";

/// Does the mask cover too little of the frame to be a subject? Only when both say so: its share
/// of the frame is under [`MASK_MIN_COVERAGE`], and (asked only then) its longest side in original
/// pixels is under [`MASK_MIN_LONGEST_SIDE_PX`], since a thin subject can still be big.
/// See docs/architecture/modules/core-shot-classifier-shotcode.md.
pub fn mask_too_small(working: &RgbImage, original: &RgbImage, mask: &Mask) -> bool {
    let fg = mask.data.iter().filter(|&&v| v > 0).count();
    let total = (working.width() as usize * working.height() as usize).max(1);
    if fg as f64 / total as f64 >= MASK_MIN_COVERAGE {
        return false;
    }
    // The mask is in working pixels; the longest side is measured in the original's, the same box
    // `route::Shot::subject_bbox_in` gives the routes.
    let fx = original.width() as f32 / working.width().max(1) as f32;
    let fy = original.height() as f32 / working.height().max(1) as f32;
    let longest = mask.bounds().map_or(0, |b| {
        let b = b.scaled(fx, fy);
        b.w.max(b.h)
    });
    longest < MASK_MIN_LONGEST_SIDE_PX
}

/// 4-bit edge-intersection string in fixed TRBL order.
pub fn eix_bits(edges: &[Edge]) -> String {
    let bit = |e: Edge| if edges.contains(&e) { '1' } else { '0' };
    [bit(Edge::Top), bit(Edge::Right), bit(Edge::Bottom), bit(Edge::Left)]
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eix_is_trbl_order() {
        assert_eq!(eix_bits(&[]), "0000");
        assert_eq!(eix_bits(&[Edge::Top]), "1000");
        assert_eq!(eix_bits(&[Edge::Right]), "0100");
        assert_eq!(eix_bits(&[Edge::Bottom]), "0010");
        assert_eq!(eix_bits(&[Edge::Left]), "0001");
        assert_eq!(eix_bits(&[Edge::Right, Edge::Bottom]), "0110");
    }

    #[test]
    fn an_almost_empty_mask_is_too_small() {
        // 6 in miniature: the segmenter marked next to nothing.
        let img = RgbImage::from_pixel(40, 40, image::Rgb([200, 200, 200]));
        let mut mask = Mask { width: 40, height: 40, data: vec![0; 40 * 40] };
        mask.data[20 * 40 + 20] = 255;
        assert!(mask_too_small(&img, &img, &mask));
    }

    #[test]
    fn a_mask_over_the_floor_is_kept_however_plain_the_backdrop() {
        // Coverage is the whole test — the backdrop plays no part in it. 10% of the frame, which
        // sits between 30's 4.4% and the 13.5% of the smallest mask that framed correctly.
        let img = RgbImage::from_pixel(100, 100, image::Rgb([255, 255, 255]));
        let mut mask = Mask { width: 100, height: 100, data: vec![0; 100 * 100] };
        for y in 45..55 {
            for x in 0..100 {
                mask.data[y * 100 + x] = 255;
            }
        }
        assert!(!mask_too_small(&img, &img, &mask));
    }

    /// A one-column mask, `rows` tall, on a 100x100 working image; the original is `side` px square.
    /// Its coverage is `rows / 100` of one column, far under the floor, so only the longest side decides.
    fn thin_mask_too_small(rows: usize, side: u32) -> bool {
        let working = RgbImage::from_pixel(100, 100, image::Rgb([255, 255, 255]));
        let original = RgbImage::from_pixel(side, side, image::Rgb([255, 255, 255]));
        let mut mask = Mask { width: 100, height: 100, data: vec![0; 100 * 100] };
        for y in 0..rows {
            mask.data[y * 100 + 50] = 255;
        }
        mask_too_small(&working, &original, &mask)
    }

    #[test]
    fn a_thin_mask_is_a_subject_from_733_original_pixels_up() {
        // 100 working rows at a 7.33x scale is 733 px tall: exactly the floor, which counts as big enough.
        assert!(!thin_mask_too_small(100, 733), "733 px is big enough");
        // One pixel less of original, 7.32x: 732 px.
        assert!(thin_mask_too_small(100, 732), "732 px is still too small");
        // The same mask on a small original is plainly too small.
        assert!(thin_mask_too_small(100, 100));
    }

    #[test]
    fn the_longest_side_is_never_asked_when_the_coverage_is_enough() {
        // 10% of the frame on a tiny original (longest side 100, well under 733): still a subject.
        let img = RgbImage::from_pixel(100, 100, image::Rgb([255, 255, 255]));
        let mut mask = Mask { width: 100, height: 100, data: vec![0; 100 * 100] };
        for y in 45..55 {
            for x in 0..100 {
                mask.data[y * 100 + x] = 255;
            }
        }
        assert!(!mask_too_small(&img, &img, &mask));
    }
}
