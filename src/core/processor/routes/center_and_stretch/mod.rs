//! R1 — the subject reaches at most three edges (EIX `0000` through any three bits).
//!
//! Build a square around the subject ([`plan`]), then fill whatever it reaches past the image by
//! stretching the background bands outward ([`compose`]). A side that lands inside the image crops
//! instead — same code, no branch.
//!
//! ## One rule, not six behaviours
//!
//! An edge the subject bleeds off is *blocked*: the square may not move it, because the subject
//! continues past it and any background put there would be painted over a subject we cannot see.
//! Everything else follows from that, per axis — an axis with a free side takes the margin, an axis
//! blocked at both ends takes none, and the slack goes to whichever sides are free.
//!
//! That single rule reproduces four of the five bleed behaviours `docs/pproni-spec.md` §5 lists
//! separately: flush to one edge, fill an axis bled at both ends, flush into a shared corner, fill
//! the boxed-in axis. They were never five behaviours — they are one rule seen from four angles,
//! which is why they live here rather than in five branches of [`super::crop_square`].
//!
//! Only the full bleed stays out. With all four edges blocked the subject's box *is* the frame,
//! there is no background anywhere to stretch from, and R1 has nothing to work with.
//!
//! The subject region is the segmentation mask's own bounding box. **No saliency here**: the
//! saliency-weighted centre of gravity belongs to [`super::crop_square`], where a fully-bled
//! subject leaves no background to anchor against.
//!
//! ## Deliberately not validated
//!
//! The background band is taken as trustworthy from `safety_px` outside the mask all the way to
//! the border. Two validations of it were designed and then *deferred*, not forgotten: trimming a
//! flat, artificial background run off the outer end, and advancing the inner end past a cast
//! shadow the mask does not cover. Plain resampling of the whole band proved good enough on the
//! test set, so neither is worth its complexity until an image argues otherwise. The argument will
//! look like a smeared shadow, or a stretched band of the wrong colour along one side.

pub mod compose;
pub mod extend;
pub mod inpaint;
pub mod plan;

use super::super::super::config;
use super::super::super::preprocessor::Prepared;
use super::{fallback, Shot};
use crate::core::shot_classifier::Mask;
use crate::core::trace;
use image::RgbImage;
use plan::{Pins, SquarePlan};

/// Per-row reach of the silhouette in *original*-image coordinates: the min/max column the mask
/// occupies on each row, or `None` where the row holds no subject.
///
/// Built once from the working-resolution mask, scaled up. Pass 1 of [`compose::compose`] uses it
/// so the untouched product band follows the actual silhouette rather than its bounding box — true
/// background the bbox over-claims on a narrow row becomes stretchable again (the plan's A.1). A
/// row with no subject keeps the bbox band, so background rows behave exactly as before, and a
/// rectangular mask reproduces the bbox on every row — which is why every existing test is
/// unaffected.
pub struct Silhouette {
    /// Indexed by original-image `y`. `Some((min_x, max_x))` inclusive, or `None`.
    pub row_x: Vec<Option<(u32, u32)>>,
}

impl Silhouette {
    pub fn from_mask(mask: &Mask, orig_w: u32, orig_h: u32) -> Silhouette {
        let fx = mask.width as f32 / orig_w.max(1) as f32;
        let fy = mask.height as f32 / orig_h.max(1) as f32;
        let mut row_x = vec![None; orig_h as usize];
        for oy in 0..orig_h {
            let my = (((oy as f32 + 0.5) * fy) as u32).min(mask.height.saturating_sub(1));
            let (mut lo, mut hi) = (u32::MAX, 0u32);
            for ox in 0..orig_w {
                let mx = (((ox as f32 + 0.5) * fx) as u32).min(mask.width.saturating_sub(1));
                if mask.get(mx, my) {
                    lo = lo.min(ox);
                    hi = hi.max(ox);
                }
            }
            if lo != u32::MAX {
                row_x[oy as usize] = Some((lo, hi));
            }
        }
        Silhouette { row_x }
    }
}

/// How far outside the mask the background is trusted, in full-resolution pixels.
///
/// Two errors stack: the mask's own boundary is only good to a couple of *working*-resolution
/// pixels, which is that many times the upscale factor once it lands here, and the full-resolution
/// boundary is itself soft by a pixel or two. Doubled for headroom.
fn safety_px(prep: &Prepared) -> u32 {
    (2.0 * (config::R1_SAFETY_MASK_PX * working_scale(prep) + config::R1_SAFETY_EDGE_PX)).round() as u32
}

/// How many full-resolution pixels inward from a gated edge the stretch/inpaint classifier probes
/// — the working-space constant `R1_STRETCH_PROBE_LEN_WORKING_PX` scaled up to original pixels.
fn probe_px(prep: &Prepared) -> u32 {
    (config::R1_STRETCH_PROBE_LEN_WORKING_PX as f32 * working_scale(prep)).round().max(1.0) as u32
}

/// How many full-resolution pixels of silhouette-adjacent background are kept out of the stretch
/// source — `R1_FILL_SOURCE_CLEARANCE_WORKING_PX` scaled to original pixels (plan A.2).
fn clearance_px(prep: &Prepared) -> u32 {
    (config::R1_FILL_SOURCE_CLEARANCE_WORKING_PX as f32 * working_scale(prep)).round() as u32
}

fn working_scale(prep: &Prepared) -> f32 {
    (prep.original.width() as f32 / prep.working.width().max(1) as f32)
        .max(prep.original.height() as f32 / prep.working.height().max(1) as f32)
        .max(1.0)
}

pub fn apply(prep: &Prepared, shot: Shot<'_>) -> RgbImage {
    // An empty mask means the classifier marked nothing: there is no subject to centre on, so the
    // safe framing of R3 is a better answer than a square around an arbitrary point.
    let Some(subject) = shot.subject_bbox_in(prep) else {
        trace::note(|| "R1: the mask is empty, so there is no subject to centre on: using R3 framing".into());
        return fallback::apply(prep);
    };
    // The edges the subject bleeds off are the ones the square may not move. `None` means no
    // square can respect them all — R3 frames the image safely rather than R1 guessing.
    let pins = Pins::from_edges(&shot.class.touches_edges);
    let Some(plan) = SquarePlan::new(subject, config::R1_MARGIN, pins) else {
        trace::note(|| format!("R1: no square can respect the bled edges {pins:?}: using R3 framing"));
        return fallback::apply(prep);
    };
    // Every edge the pass-1 gate flagged — including the ones refinement then cleared ("orange").
    // Clean edges aren't in the list, so an image with no gated edge still runs the same code path
    // compose always has, which keeps the no-gate case fully resample-only.
    let gated: Vec<_> = shot.class.refinements.iter().map(|r| r.edge).collect();
    let silhouette = Silhouette::from_mask(shot.mask, prep.original.width(), prep.original.height());
    compose::compose(
        &prep.original,
        &plan,
        subject,
        &silhouette,
        safety_px(prep),
        clearance_px(prep),
        &gated,
        shot.bgc,
        probe_px(prep),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::shot_classifier::{geometry::Rect, Edge, EdgeRefinement, Mask, ShotClassification};
    use image::Rgb;

    /// A `w x h` image: uniform background, with `subject` painted a different colour.
    fn scene(w: u32, h: u32, subject: Rect, bg: [u8; 3], fg: [u8; 3]) -> (RgbImage, Mask) {
        let mut img = RgbImage::from_pixel(w, h, Rgb(bg));
        let mut mask = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
        for y in subject.y..subject.y + subject.h {
            for x in subject.x..subject.x + subject.w {
                img.put_pixel(x, y, Rgb(fg));
                mask.data[(y * w + x) as usize] = 255;
            }
        }
        (img, mask)
    }

    fn run(img: RgbImage, mask: &Mask) -> RgbImage {
        let prep = Prepared { original: img.clone(), working: img.clone(), model_input: img.clone(), crop: Rect { x: 0, y: 0, w: img.width(), h: img.height() } };
        let class = ShotClassification::default();
        apply(&prep, Shot { class: &class, mask, bgc: None })
    }

    fn run_gated(
        img: RgbImage,
        mask: &Mask,
        gated: &[Edge],
        bgc: Option<Rgb<u8>>,
    ) -> RgbImage {
        let prep = Prepared {
            original: img.clone(),
            working: img.clone(),
            model_input: img.clone(),
            crop: Rect { x: 0, y: 0, w: img.width(), h: img.height() },
        };
        let class = ShotClassification {
            touches_edges: Vec::new(),
            mask_too_small: false,
            refinements: gated
                .iter()
                .map(|&edge| EdgeRefinement {
                    edge,
                    touching: false,
                    crop_region: Rect { x: 0, y: 0, w: 1, h: 1 },
                    coverage: None,
                })
                .collect(),
        };
        apply(&prep, Shot { class: &class, mask, bgc })
    }

    #[test]
    fn silhouette_scales_row_extents_from_working_to_original() {
        // 10x10 working mask, subject columns 2..=5 on rows 3..=6. Original is 20x20 (2x).
        let mut mask = Mask { width: 10, height: 10, data: vec![0u8; 100] };
        for y in 3..=6 {
            for x in 2..=5 {
                mask.data[(y * 10 + x) as usize] = 255;
            }
        }
        let s = Silhouette::from_mask(&mask, 20, 20);
        // Original row 8 maps to working row 4, inside the band; columns 2..=5 at 2x cover ~4..=11.
        let (lo, hi) = s.row_x[8].expect("row 8 is inside the subject");
        assert!(lo <= 5 && hi >= 10, "expected ~4..=11, got {lo}..={hi}");
        // Rows above and below the subject hold no mask.
        assert!(s.row_x[0].is_none());
        assert!(s.row_x[19].is_none());
    }

    #[test]
    fn output_is_square_and_the_subject_is_centred() {
        // Subject sits well off-centre, low and left; R1 must put it in the middle.
        let (img, mask) = scene(400, 300, Rect { x: 20, y: 180, w: 60, h: 90 }, [200, 200, 200], [10, 10, 10]);
        let out = run(img, &mask);

        assert_eq!(out.width(), out.height());
        assert_eq!(out.width(), 98, "90 * 1.084 = 97.56");
        // Centre of the output is subject, and the corners are background.
        assert_eq!(*out.get_pixel(47, 47), Rgb([10, 10, 10]));
        assert_eq!(*out.get_pixel(1, 1), Rgb([200, 200, 200]));
        assert_eq!(*out.get_pixel(92, 92), Rgb([200, 200, 200]));
    }

    #[test]
    fn a_square_that_fits_is_a_pure_crop() {
        // Every side lands on real pixels, so nothing is resampled: a uniform background stays
        // exactly its own value, and the subject band is copied untouched.
        let (img, mask) = scene(400, 400, Rect { x: 150, y: 150, w: 100, h: 100 }, [240, 240, 240], [0, 0, 0]);
        let out = run(img, &mask);
        assert_eq!(out.dimensions(), (108, 108));
        assert_eq!(*out.get_pixel(52, 52), Rgb([0, 0, 0]));
        assert_eq!(*out.get_pixel(0, 0), Rgb([240, 240, 240]));
    }

    #[test]
    fn a_subject_wider_than_the_frame_is_filled_on_the_narrow_axis() {
        // 200x100 frame, subject nearly filling it: the square is taller than the image, so the
        // top and bottom bands stretch. Corners come out background — the proof that the
        // two-pass order fills them, since they are outside the image on both axes.
        let (img, mask) = scene(200, 100, Rect { x: 40, y: 20, w: 120, h: 60 }, [180, 190, 200], [30, 40, 50]);
        let out = run(img, &mask);
        assert_eq!(out.dimensions(), (130, 130));
        assert_eq!(*out.get_pixel(2, 2), Rgb([180, 190, 200]), "top-left corner");
        assert_eq!(*out.get_pixel(122, 122), Rgb([180, 190, 200]), "bottom-right corner");
        assert_eq!(*out.get_pixel(62, 62), Rgb([30, 40, 50]), "centre is still subject");
    }

    #[test]
    fn a_gradient_background_keeps_its_gradient_when_stretched() {
        // The case a flat fill loses. The ramp runs left-to-right and the stretch is horizontal,
        // so the left band has to carry its gradient across 44 invented columns rather than
        // settle at one averaged value.
        let mut img = RgbImage::new(120, 400);
        for y in 0..400 {
            for x in 0..120 {
                let v = (x * 255 / 119) as u8;
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        let mut mask = Mask { width: 120, height: 400, data: vec![0u8; 120 * 400] };
        for y in 100..300 {
            for x in 20..100 {
                img.put_pixel(x, y, Rgb([255, 0, 0]));
                mask.data[(y * 120 + x) as usize] = 255;
            }
        }
        let out = run(img, &mask);
        assert_eq!(out.dimensions(), (217, 217), "200 * 1.084 = 216.8");

        // Left band: source columns 0..12 (subject at 20, safety 8) stretched over output
        // columns 0..61. Both ends pinned, and no reversal in between.
        let row = out.height() / 2;
        let band: Vec<u8> = (0..61).map(|x| out.get_pixel(x, row)[0]).collect();
        assert_eq!(band[0], 0, "outer end is pinned to the image's own border pixel");
        assert_eq!(band[60], 23, "inner end meets column 11 of the source, value 23");
        assert!(
            band.windows(2).all(|w| w[0] <= w[1]),
            "a stretched gradient must stay monotonic, got {band:?}"
        );
    }

    #[test]
    fn an_empty_mask_falls_back_instead_of_centring_on_nothing() {
        let img = RgbImage::from_pixel(200, 100, Rgb([1, 2, 3]));
        let mask = Mask { width: 200, height: 100, data: vec![0u8; 200 * 100] };
        let out = run(img, &mask);
        // R3's answer: whole image on a square canvas at the longest side.
        assert_eq!(out.dimensions(), (200, 200));
    }

    #[test]
    fn a_shadow_sliver_on_a_gated_bottom_does_not_stretch_into_the_overhang() {
        // 11.jpg in a bottle: a low frame with a product in the middle so the planner's square
        // overhangs the bottom edge, plus a dark shadow sliver along part of that bottom edge.
        // The bottom edge is gated. Blind stretching would smear the sliver across the whole
        // bottom overhang; the new classifier should route those columns to the inpainter.
        let w = 120u32;
        let h = 40u32;
        let mut img = RgbImage::from_pixel(w, h, Rgb([255, 255, 255]));
        let subject = Rect { x: 30, y: 5, w: 60, h: 20 };
        let mut mask = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
        for y in subject.y..subject.y + subject.h {
            for x in subject.x..subject.x + subject.w {
                img.put_pixel(x, y, Rgb([200, 20, 20]));
                mask.data[(y * w + x) as usize] = 255;
            }
        }
        // Sliver: four rows at the bottom, columns 50..70. Dark grey, well outside the BGC
        // tolerance of 18 against white.
        for y in (h - 4)..h {
            for x in 50..70 {
                img.put_pixel(x, y, Rgb([60, 60, 60]));
            }
        }

        let out = run_gated(img.clone(), &mask, &[Edge::Bottom], Some(Rgb([255, 255, 255])));
        // Side = 60 * 1.084 = 65.04 → 65.
        assert_eq!(out.dimensions(), (65, 65));

        // Pick a mid-column inside the sliver's horizontal range: src columns 50..70 map to the
        // same output columns because plan.x ≈ 27, side = 65, so src col 60 → out col 33.
        let col_x = 33u32;
        let out_h = out.height();
        // Bottom 10 output pixels should not drift anywhere near the sliver's grey (60).
        let mut smeared = false;
        for y in (out_h - 10)..out_h {
            let p = out.get_pixel(col_x, y);
            if p[0] < 180 {
                smeared = true;
                break;
            }
        }
        assert!(
            !smeared,
            "shadow sliver smeared into the overhang at x={col_x}, first dark px at y={}",
            (out_h - 10..out_h)
                .find(|&y| out.get_pixel(col_x, y)[0] < 180)
                .unwrap_or(out_h)
        );
    }

    #[test]
    fn a_clean_bottom_edge_still_stretches_when_the_gate_flagged_it() {
        // Same geometry as the sliver test, no sliver. A gated-but-clean bottom should stretch
        // identically to the no-gate case — the classifier calls every column "stretchable".
        let w = 120u32;
        let h = 40u32;
        let subject = Rect { x: 30, y: 5, w: 60, h: 20 };
        let mut img = RgbImage::from_pixel(w, h, Rgb([255, 255, 255]));
        let mut mask = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
        for y in subject.y..subject.y + subject.h {
            for x in subject.x..subject.x + subject.w {
                img.put_pixel(x, y, Rgb([200, 20, 20]));
                mask.data[(y * w + x) as usize] = 255;
            }
        }

        let out = run_gated(img, &mask, &[Edge::Bottom], Some(Rgb([255, 255, 255])));
        // Bottom overhang on a pure-white backdrop stays white — a resample of white is white.
        let out_h = out.height();
        for y in (out_h - 5)..out_h {
            let p = out.get_pixel(33, y);
            assert_eq!(p[0], 255, "clean white bottom should stay white, got {p:?} at y={y}");
            assert_eq!(p[1], 255);
            assert_eq!(p[2], 255);
        }
    }
}
