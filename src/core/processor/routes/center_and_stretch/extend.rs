//! Part C — re-framing a trimmed no-backdrop / textured scene to a square.
//!
//! After the preprocessor trims letterbox/pillarbox bars (`preprocessor::optimizer::trim_letterbox`),
//! a textured scene must be re-framed to a square **without** R1's background stretch — stretching a
//! textured scene invents a smeared fill across the overhang (measured on 268/46). The plan's
//! priority, highest first:
//!
//! 1. **Crop** a square that preserves the whole subject — no invented pixels. [`square_crop_preserving`].
//! 2. **Seam-carve** to extend the scene, accepted only if a perceptual-quality check passes.
//!    [`seam_carve_to_square`] grows the short axis with minimum-energy seam insertion and rejects
//!    (returns `None`) when the cheapest seams still cross too much structure — the energy threshold
//!    is `config::R1_SEAM_CARVE_MAX_SEAM_ENERGY`.
//! 3. **White-pad** as a last resort. [`white_pad`].
//!
//! [`reframe_to_square`] applies that priority order. R1 calls it when the preprocessor found flat
//! letterbox/pillarbox bars round a textured scene (`Prepared::letterbox`).

use crate::core::config::R1_SEAM_CARVE_MAX_SEAM_ENERGY;
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


/// Rec. 601 luma of one pixel.
fn luma(p: &Rgb<u8>) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// Per-pixel energy, row-major: forward-difference luma gradient `|dL/dx| + |dL/dy|`, clamped at the
/// right/bottom edge (difference against itself, so 0 there). Flat scenes read 0; busy scenes read
/// high, which is what the quality gate keys on.
fn energy_map(img: &RgbImage) -> Vec<f32> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let l: Vec<f32> = img.pixels().map(luma).collect();
    let mut e = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let xr = (x + 1).min(w - 1);
            let yd = (y + 1).min(h - 1);
            e[y * w + x] = (l[y * w + xr] - l[y * w + x]).abs() + (l[yd * w + x] - l[y * w + x]).abs();
        }
    }
    e
}

/// The minimum-energy top-to-bottom seam over a working energy grid of width `cw`, height `h`
/// (row-major). Returns the working column chosen on each row. Classic cumulative-energy DP.
fn min_vertical_seam(e: &[f32], cw: usize, h: usize) -> Vec<usize> {
    let mut m = e.to_vec();
    for y in 1..h {
        for x in 0..cw {
            let left = if x > 0 { m[(y - 1) * cw + x - 1] } else { f32::INFINITY };
            let mid = m[(y - 1) * cw + x];
            let right = if x + 1 < cw { m[(y - 1) * cw + x + 1] } else { f32::INFINITY };
            m[y * cw + x] += left.min(mid).min(right);
        }
    }
    let mut seam = vec![0usize; h];
    let last = (h - 1) * cw;
    let mut best = 0usize;
    for x in 1..cw {
        if m[last + x] < m[last + best] {
            best = x;
        }
    }
    seam[h - 1] = best;
    for y in (0..h - 1).rev() {
        let x = seam[y + 1];
        let mut bx = x;
        let mut bv = m[y * cw + x];
        if x > 0 && m[y * cw + x - 1] < bv {
            bv = m[y * cw + x - 1];
            bx = x - 1;
        }
        if x + 1 < cw && m[y * cw + x + 1] < bv {
            bx = x + 1;
        }
        seam[y] = bx;
    }
    seam
}

/// Find the `k` lowest-energy vertical seams in `img`, in ORIGINAL column coordinates, and the mean
/// per-pixel energy over them. Seams are found by repeatedly removing the current minimum seam from
/// a working copy (so the `k` are distinct, not the same cheap seam `k` times — Avidan–Shamir), each
/// seam's energy read from the original map so the gate is stable. `None` when the image is too
/// narrow to carve `k` seams.
fn k_lowest_vertical_seams(img: &RgbImage, k: usize) -> Option<(Vec<Vec<usize>>, f32)> {
    let (w0, h) = (img.width() as usize, img.height() as usize);
    if k == 0 || w0 < 2 {
        return None;
    }
    let e0 = energy_map(img);
    let mut cw = w0;
    let mut we = e0.clone();
    let mut orig: Vec<usize> = (0..h).flat_map(|_| 0..w0).collect();

    let mut seams: Vec<Vec<usize>> = Vec::with_capacity(k);
    let mut total_energy = 0.0f32;
    for _ in 0..k {
        if cw < 2 {
            return None;
        }
        let seam = min_vertical_seam(&we, cw, h);
        let mut seam_orig = vec![0usize; h];
        for y in 0..h {
            let oc = orig[y * cw + seam[y]];
            seam_orig[y] = oc;
            total_energy += e0[y * w0 + oc];
        }
        seams.push(seam_orig);

        // Drop the chosen column from each row of the working grids.
        let ncw = cw - 1;
        let mut nwe = vec![0.0f32; ncw * h];
        let mut norig = vec![0usize; ncw * h];
        for y in 0..h {
            let wx = seam[y];
            let mut nx = 0;
            for x in 0..cw {
                if x == wx {
                    continue;
                }
                nwe[y * ncw + nx] = we[y * cw + x];
                norig[y * ncw + nx] = orig[y * cw + x];
                nx += 1;
            }
        }
        we = nwe;
        orig = norig;
        cw = ncw;
    }
    Some((seams, total_energy / (k as f32 * h as f32)))
}

/// Widen `img` by inserting the given seams (original column indices per row), each inserted pixel
/// the average of its two original neighbours. Output width is `img.width() + seams.len()`.
fn insert_vertical_seams(img: &RgbImage, seams: &[Vec<usize>]) -> RgbImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let k = seams.len();
    let mut out = RgbImage::new((w + k) as u32, h as u32);
    for y in 0..h {
        let mut ins: Vec<usize> = seams.iter().map(|s| s[y]).collect();
        ins.sort_unstable();
        let mut ip = 0usize;
        let mut ox = 0u32;
        for x in 0..w {
            let p = *img.get_pixel(x as u32, y as u32);
            out.put_pixel(ox, y as u32, p);
            ox += 1;
            while ip < ins.len() && ins[ip] == x {
                let q = *img.get_pixel((x + 1).min(w - 1) as u32, y as u32);
                let avg = Rgb([
                    ((p[0] as u16 + q[0] as u16) / 2) as u8,
                    ((p[1] as u16 + q[1] as u16) / 2) as u8,
                    ((p[2] as u16 + q[2] as u16) / 2) as u8,
                ]);
                out.put_pixel(ox, y as u32, avg);
                ox += 1;
                ip += 1;
            }
        }
    }
    out
}

/// Swap rows and columns, so a height grow can reuse the width-grow code.
fn transpose(img: &RgbImage) -> RgbImage {
    let (w, h) = img.dimensions();
    let mut t = RgbImage::new(h, w);
    for y in 0..h {
        for x in 0..w {
            t.put_pixel(y, x, *img.get_pixel(x, y));
        }
    }
    t
}

/// Grow the width by `add` columns if the quality gate passes, else `None`.
fn grow_width(img: &RgbImage, add: usize) -> Option<RgbImage> {
    let (seams, mean) = k_lowest_vertical_seams(img, add)?;
    if mean > R1_SEAM_CARVE_MAX_SEAM_ENERGY {
        return None;
    }
    Some(insert_vertical_seams(img, &seams))
}

/// Grow the height by `add` rows, via the transpose of [`grow_width`].
fn grow_height(img: &RgbImage, add: usize) -> Option<RgbImage> {
    let grown = grow_width(&transpose(img), add)?;
    Some(transpose(&grown))
}

/// Priority 2 — seam-carve `src` out to a `target`-pixel square, accepted only when the perceptual
/// quality gate passes on every round.
///
/// Only *extension* is handled (both axes at or below `target`); a larger axis is the crop /
/// white-pad case, so that returns `None`. Each axis grows in rounds of at most its current length
/// so no single round more than doubles it (seam insertion past ~2x degrades even on clean scenes).
/// Any round whose cheapest seams average above `config::R1_SEAM_CARVE_MAX_SEAM_ENERGY` fails the
/// whole carve, and the caller falls through to [`white_pad`].
pub fn seam_carve_to_square(src: &RgbImage, target: u32) -> Option<RgbImage> {
    let (w, h) = src.dimensions();
    if w > target || h > target {
        return None;
    }
    let mut img = src.clone();
    while img.width() < target {
        let cur = img.width();
        let add = (target - cur).min(cur.saturating_sub(1)).max(1) as usize;
        img = grow_width(&img, add)?;
    }
    while img.height() < target {
        let cur = img.height();
        let add = (target - cur).min(cur.saturating_sub(1)).max(1) as usize;
        img = grow_height(&img, add)?;
    }
    (img.width() == target && img.height() == target).then_some(img)
}

/// The Part C route decision for a trimmed textured scene: reframe to a square **without** R1's
/// stretch, in priority order — a subject-preserving crop, else a quality-gated seam-carve, else a
/// white pad. Pure; R1 calls it when `Prepared::letterbox` is set.
pub fn reframe_to_square(src: &RgbImage, subject: Rect) -> RgbImage {
    let (w, h) = src.dimensions();
    if let Some(keep) = square_crop_preserving(subject, w, h) {
        return image::imageops::crop_imm(src, keep.x, keep.y, keep.w, keep.h).to_image();
    }
    if let Some(carved) = seam_carve_to_square(src, w.max(h)) {
        return carved;
    }
    white_pad(src)
}

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

    /// A `w`x`h` flat mid-grey scene — zero energy everywhere, so the gate always passes.
    fn flat(w: u32, h: u32) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb([128, 128, 128]))
    }

    /// A `w`x`h` checkerboard — maximal forward-difference energy everywhere, so no clean seam
    /// exists and the gate always rejects.
    fn checker(w: u32, h: u32) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        img
    }

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
    fn seam_carve_grows_a_clean_scene_to_square() {
        // Narrow flat scene: the gate passes and width grows to the target square.
        let out = seam_carve_to_square(&flat(40, 60), 60).expect("a clean scene carves");
        assert_eq!(out.dimensions(), (60, 60));
        // A flat scene stays flat (inserted pixels are the average of equal neighbours).
        assert_eq!(*out.get_pixel(30, 30), Rgb([128, 128, 128]));
    }

    #[test]
    fn seam_carve_grows_the_height_axis_too() {
        // Short-and-wide flat scene exercises the transpose (grow_height) path.
        let out = seam_carve_to_square(&flat(60, 40), 60).expect("a clean scene carves");
        assert_eq!(out.dimensions(), (60, 60));
    }

    #[test]
    fn seam_carve_rejects_a_scene_with_no_clean_seam() {
        // Every seam crosses maximal structure, so the quality gate rejects → None → white-pad.
        assert!(seam_carve_to_square(&checker(40, 60), 60).is_none());
    }

    #[test]
    fn seam_carve_declines_a_reduction() {
        // A larger-than-target axis is the crop/pad case, not seam-carve.
        assert!(seam_carve_to_square(&flat(80, 60), 60).is_none());
    }

    #[test]
    fn reframe_prefers_a_preserving_crop() {
        // Subject fits the shorter side (60), so priority 1 crops — the output is that 60-square,
        // not the max-side square the other two priorities would give.
        let subject = Rect { x: 30, y: 10, w: 40, h: 40 };
        let out = reframe_to_square(&flat(100, 60), subject);
        assert_eq!(out.dimensions(), (60, 60), "a preserving crop at the shorter side");
    }

    #[test]
    fn reframe_seam_carves_when_a_crop_cannot_preserve_the_subject() {
        // Subject taller than the shorter side → crop declines; the scene is clean → seam-carve.
        let subject = Rect { x: 15, y: 1, w: 10, h: 58 };
        let out = reframe_to_square(&flat(40, 60), subject);
        assert_eq!(out.dimensions(), (60, 60), "seam-carved to the max-side square");
    }

    #[test]
    fn reframe_falls_through_to_white_pad_on_a_busy_scene() {
        // Crop declines (tall subject) and the busy scene fails the seam-carve gate → white-pad.
        let subject = Rect { x: 15, y: 1, w: 10, h: 58 };
        let out = reframe_to_square(&checker(40, 60), subject);
        assert_eq!(out.dimensions(), (60, 60));
        assert_eq!(*out.get_pixel(3, 30), Rgb([255, 255, 255]), "side band is white");
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
