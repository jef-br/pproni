//! Part C — re-framing a trimmed no-backdrop / textured scene to a square.
//!
//! After the preprocessor trims letterbox/pillarbox bars (`preprocessor::optimizer::trim_letterbox`),
//! a textured scene must be re-framed to a square **without** R1's background stretch — stretching a
//! textured scene invents a smeared fill across the overhang (measured on 268/46). The plan's
//! priority, highest first:
//!
//! 1. **Crop** a square that preserves the whole subject — no invented pixels. [`square_crop_preserving`].
//! 2. **Seam-carve** to extend the scene, accepted only if a perceptual-quality check passes.
//!    **Not built here** — its energy map, DP seam selection, and the quality-gate threshold are
//!    tuned on the real mountain/268 stems in `/pair` (plan Part C; `docs/agent-knowledge/acceptance-checks.md`
//!    leaves the gate threshold open). [`seam_carve_to_square`] returns `None` so the chain falls
//!    through to white-pad until that session builds it.
//! 3. **White-pad** as a last resort. [`white_pad`].
//!
//! The crop test and white-pad are deterministic and are built and unit-tested below. **Nothing
//! here is wired into the route yet**: choosing crop vs seam-carve vs white-pad per image, and the
//! `268` edge-touch (EIX) consequence of trimming, must be validated on the real stems first — that
//! is the `/pair` work this module is staged for.

use crate::core::shot_classifier::geometry::Rect;
use image::{Rgb, RgbImage};

/// Priority 1 — the largest square crop (the shorter frame side) that still contains the whole
/// `subject` box, centred on the subject and slid inside the frame `w`x`h`.
///
/// `None` when the subject is longer than that side on either axis: a square crop then cannot
/// preserve it, and the caller falls through to seam-carve / white-pad. When the dimensions do fit,
/// centring on the subject and clamping into the frame always keeps the whole subject inside
/// (`side >= subject.w` and `side >= subject.h`), so a crop is returned.
pub fn square_crop_preserving(subject: Rect, w: u32, h: u32) -> Option<Rect> {
    let side = w.min(h);
    if subject.w > side || subject.h > side {
        return None;
    }
    let cx = subject.x as i64 + subject.w as i64 / 2;
    let cy = subject.y as i64 + subject.h as i64 / 2;
    let x = (cx - side as i64 / 2).clamp(0, (w - side) as i64) as u32;
    let y = (cy - side as i64 / 2).clamp(0, (h - side) as i64) as u32;
    Some(Rect { x, y, w: side, h: side })
}

/// Priority 2 — seam-carve the scene out to a `target`-pixel square, accepted only if a perceptual
/// quality check passes. **Not implemented**: the energy map, seam DP and the quality threshold are
/// tuned on the real stems in `/pair`. Returns `None` so the caller falls through to [`white_pad`].
// <jbpair goal="Part C seam-carve: energy map + DP seam insertion, its perceptual quality gate, AND wiring the trim -> crop|seam-carve|white-pad route in center_and_stretch; build and tune on the real 41/46 stems in /pair">
pub fn seam_carve_to_square(_src: &RgbImage, _target: u32) -> Option<RgbImage> {
    None
}
// </jbpair>

/// Priority 3 — centre `src` on a square white canvas of its longer side. The last resort, used
/// only when neither a preserving crop nor an accepted seam-carve is available.
pub fn white_pad(src: &RgbImage) -> RgbImage {
    let (w, h) = src.dimensions();
    let side = w.max(h);
    if w == h {
        return src.clone();
    }
    let mut canvas = RgbImage::from_pixel(side, side, Rgb([255, 255, 255]));
    image::imageops::replace(&mut canvas, src, ((side - w) / 2) as i64, ((side - h) / 2) as i64);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_preserves_a_subject_that_fits_the_shorter_side() {
        // 2000x1245 (268 after a letterbox trim): a 1245 square around a subject that fits.
        let subject = Rect { x: 600, y: 200, w: 800, h: 900 };
        let keep = square_crop_preserving(subject, 2000, 1245).expect("subject fits a 1245 square");
        assert_eq!((keep.w, keep.h), (1245, 1245), "square at the shorter side");
        // The whole subject is inside the crop.
        assert!(keep.x <= subject.x && keep.y <= subject.y);
        assert!(keep.x + keep.w >= subject.x + subject.w);
        assert!(keep.y + keep.h >= subject.y + subject.h);
        // And the crop stays inside the frame.
        assert!(keep.x + keep.w <= 2000 && keep.y + keep.h <= 1245);
    }

    #[test]
    fn crop_declines_a_subject_taller_than_the_shorter_side() {
        // 972x1310 (mountain after a pillarbox trim): the subject is 1218 tall, taller than the
        // 972 shorter side, so no square crop preserves it — this is the case that needs seam-carve.
        let subject = Rect { x: 0, y: 46, w: 617, h: 1218 };
        assert!(square_crop_preserving(subject, 972, 1310).is_none());
    }

    #[test]
    fn seam_carve_is_not_built_yet() {
        let img = RgbImage::new(10, 10);
        assert!(seam_carve_to_square(&img, 12).is_none());
    }

    #[test]
    fn white_pad_centres_on_a_square_white_canvas() {
        let src = RgbImage::from_pixel(40, 100, Rgb([10, 20, 30]));
        let out = white_pad(&src);
        assert_eq!(out.dimensions(), (100, 100));
        assert_eq!(*out.get_pixel(50, 50), Rgb([10, 20, 30]), "centre is the scene");
        assert_eq!(*out.get_pixel(5, 50), Rgb([255, 255, 255]), "side band is white");
    }
}
