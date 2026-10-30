//! Shot code: the classifier's side of the per-image tag — the `EIX` (edge intersection) bits.
//!
//! The `BGC`/`FGC` colours are not here: they are sampled by the exporter
//! (`exporter::colors`), which also assembles the full tag.
//!
//! Partial — a background-type (`BG`) tag is still to come. Until then,
//! `mask_too_small` reports the case that tag would cover — a mask covering too
//! little of the frame to be a subject — as *information only*.
//! `eix` always reports the classifier's real verdict:
//! a tag must never claim an edge intersection the pipeline didn't actually
//! route on. Routing (`route::decide`) reads only `touches_edges` and
//! `mask_too_small`, never this heuristic, so letting it override `eix` would
//! make the tag lie about what the pipeline did with the image.

use super::super::config::{MASK_MIN_COVERAGE, MASK_MIN_LONGEST_SIDE_PX};
use super::{Edge, Mask};
use image::RgbImage;

/// EIX string when the classifier produced no verdict at all. Four
/// underscores mirror the 4-bit TRBL width of a real verdict.
pub const NO_VERDICT_EIX: &str = "____";

/// Does the mask cover too little of the frame to be a subject?
///
/// Two measurements. The share of the frame the mask covers, against [`MASK_MIN_COVERAGE`]; and, only
/// when that share is too low, the mask's longest side in original pixels, against
/// [`MASK_MIN_LONGEST_SIDE_PX`]. A tall or thin subject can cover little of the frame and still be
/// big; it is too small only when both say so. Nothing about *why* it is small — the images this
/// catches are close-ups where the frame was already all product and the segmenter latched onto the
/// highest-contrast detail in it, but the route does not need that story to be true, only that
/// there is no subject here worth framing around.
///
/// A further condition — "and no uniform background was found" — was measured and left out. It is
/// the only thing that would tell a genuinely small product on clean paper, jewellery being the
/// obvious case, from this. Across the 37-image set the two rules flag exactly the same images, so
/// it earns nothing yet; add it back against a picture that needs it.
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
