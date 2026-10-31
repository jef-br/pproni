//! Framing: what the padding or frame round the picture means, read from the two trims and the mask.
//!
//! Trim 1 ([`crate::core::preprocessor::optimizer`]) boxes everything that is not backdrop; what it
//! trims off is real backdrop round a floating product, or not picture at all. The mask of the one
//! model run, on trim 1's crop, is the reliable fact that tells them apart.
//! * **Bars** on one axis: [`Decision::Centre`], R1 cuts the square from the content box with equal
//!   padding either side, aligned on nothing. See docs/architecture/02-modules-core.md.
//! * **Frame** all round: [`Decision::Peel`] crops to trim 1's box and the edge check runs again (the
//!   product the frame hid now meets the new border). See docs/work-state/2026-10-21-trims-and-framing.md.
//!
//! Everything else is [`Decision::Keep`]. The decisions are pure, so they are tested without a model.

use crate::core::config;
use crate::core::shot_classifier::geometry::Rect;
use crate::core::shot_classifier::Mask;
use image::RgbImage;

/// What to do about the padding or frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to do: R1 frames the subject as usual.
    Keep,
    /// White bars on one axis: centre this content box (`working` coordinates) in the square.
    Centre(Rect),
    /// A frame round the picture: crop to this box (`working` coordinates).
    Peel(Rect),
}

/// Decide, and say why in one line for the log.
///
/// `frame` is the working picture's size, `first`/`second` the two trims' tight boxes, `touched` how
/// many image edges the subject touches (the number of ones in EIX) and `mask` the box around the mask.
pub fn decide(frame: (u32, u32), first: Rect, second: Option<Rect>, touched: usize, mask: Option<Rect>) -> (Decision, String) {
    let touching = touched > 0;
    let (w, h) = frame;
    let l = first.x;
    let t = first.y;
    let r = w.saturating_sub(first.x + first.w);
    let b = h.saturating_sub(first.y + first.h);
    let tol = config::TRIM_RECHECK_TOLERANCE_PX;
    let insets = format!("trim 1 {first:?} cut l={l} t={t} r={r} b={b}");

    if l.max(t).max(r).max(b) <= tol {
        return (Decision::Keep, format!("{insets}: nothing was cut off"));
    }

    // One axis only: both its sides cut, the other axis nearly whole.
    let slack_x = (config::FRAMING_OTHER_AXIS_SLACK * w as f32) as u32;
    let slack_y = (config::FRAMING_OTHER_AXIS_SLACK * h as f32) as u32;
    let bars_x = l > tol && r > tol && t + b <= slack_y;
    let bars_y = t > tol && b > tol && l + r <= slack_x;
    let even = |p: u32, q: u32| p.min(q) as f32 >= config::FRAMING_BARS_SYMMETRY * p.max(q) as f32;
    if (bars_x || bars_y) && touching {
        let (p, q) = if bars_x { (l, r) } else { (t, b) };
        if !even(p, q) {
            return (Decision::Keep, format!("{insets}: the two cut sides are {p} and {q} px, unequal, so this is backdrop round an off-centre subject, not padding"));
        }
        return (Decision::Centre(first), format!("{insets}: bars on one axis and the subject touches {touched} edge(s), so the content is centred"));
    }

    if touching {
        return (Decision::Keep, format!("{insets}: the subject touches an edge but this is not one-axis padding"));
    }
    if !(l > tol && t > tol && r > tol && b > tol) {
        return (Decision::Keep, format!("{insets}: not cut on all four sides, so not a frame"));
    }
    if let Some(s) = second {
        return (Decision::Keep, format!("{insets}: trim 2 moved it to {s:?}, so there is more than one layer round the picture"));
    }
    let (long, short) = (first.w.max(first.h), first.w.min(first.h));
    if (long - short) as f32 > config::FRAMING_SQUARE_TOLERANCE * long as f32 {
        return (Decision::Keep, format!("{insets}: the box is {}x{}, not square", first.w, first.h));
    }
    let Some(m) = mask else {
        return (Decision::Keep, format!("{insets}: no mask to check against"));
    };
    let tol_mask = config::TRIM_SEGMASK_TOLERANCE_PX;
    if rects_near(m, first, tol_mask) {
        return (Decision::Keep, format!("{insets}: the box is the product itself (mask {m:?}), so what is cut is real backdrop"));
    }
    if !rect_within(m, first, tol_mask) {
        return (Decision::Keep, format!("{insets}: the mask {m:?} reaches outside the box, so the box cut into the product"));
    }
    (Decision::Peel(first), format!("{insets}: a square box round a smaller subject (mask {m:?}) inside a ring on all four sides, so the frame is peeled"))
}

/// The colour of the frame: the dominant colour of the picture's outer four lines all the way round.
///
/// Dominant, not median, and four lines deep rather than one: the very last row or column of a JPEG is
/// often a one-pixel line of another colour, and on a thin frame that line can be half of the outer ring
/// (it was, on the frame this was written for). Colours are grouped in 5-bit bins; the answer is the
/// mean of the fullest bin.
pub fn border_colour(img: &RgbImage) -> [u8; 3] {
    let (w, h) = img.dimensions();
    let mut count = vec![0u32; 32 * 32 * 32];
    let mut sum = vec![[0u64; 3]; 32 * 32 * 32];
    let mut add = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        let bin = ((p[0] >> 3) as usize * 32 + (p[1] >> 3) as usize) * 32 + (p[2] >> 3) as usize;
        count[bin] += 1;
        for c in 0..3 {
            sum[bin][c] += p[c] as u64;
        }
    };
    for d in 0..4.min(w.min(h) / 2) {
        for x in d..w - d {
            add(x, d);
            add(x, h - 1 - d);
        }
        for y in d + 1..h - 1 - d {
            add(d, y);
            add(w - 1 - d, y);
        }
    }
    let best = (0..count.len()).max_by_key(|&i| count[i]).unwrap_or(0);
    let n = count[best].max(1) as u64;
    [(sum[best][0] / n) as u8, (sum[best][1] / n) as u8, (sum[best][2] / n) as u8]
}

/// Peel deeper than trim 1's box wherever the frame goes on inside it.
///
/// Trim 1 stops where something not of the frame's colour begins, so where the product runs into the
/// frame (a dark product against a dark frame) the box can stop several pixels inside it. Each side
/// of `first` is moved inward while its outermost line is still frame: at least 99% of the pixels
/// that are NOT under the mask lie within `T_EDGE_FLAT` of `frame` on every channel. The mask is what
/// makes this safe — product pixels never count against the frame, and a line with real background
/// or product outside the mask stops the peel. No side moves more than a quarter of the box.
pub fn deepen_peel(img: &RgbImage, mask: &Mask, first: Rect, frame: [u8; 3]) -> Rect {
    let near = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        (0..3).all(|c| p[c].abs_diff(frame[c]) as u16 <= config::T_EDGE_FLAT)
    };
    // Is the line of pixels `(x0.., y)` (a row) or `(x, y0..)` (a column) frame?
    let is_frame = |fixed: u32, from: u32, to: u32, row: bool| {
        let (mut total, mut ok) = (0u32, 0u32);
        for k in from..to {
            let (x, y) = if row { (k, fixed) } else { (fixed, k) };
            if mask.get(x, y) {
                continue;
            }
            total += 1;
            ok += near(x, y) as u32;
        }
        total > 0 && ok * 100 >= total * 99
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.w, first.y + first.h);
    let (qw, qh) = (first.w / 4, first.h / 4);
    let (ox0, oy0, ox1, oy1) = (x0, y0, x1, y1);
    while y0 - oy0 < qh && y0 + 1 < y1 && is_frame(y0, x0, x1, true) {
        y0 += 1;
    }
    while oy1 - y1 < qh && y1 - 1 > y0 && is_frame(y1 - 1, x0, x1, true) {
        y1 -= 1;
    }
    while x0 - ox0 < qw && x0 + 1 < x1 && is_frame(x0, y0, y1, false) {
        x0 += 1;
    }
    while ox1 - x1 < qw && x1 - 1 > x0 && is_frame(x1 - 1, y0, y1, false) {
        x1 -= 1;
    }
    // The line where the frame ends is blurred by the resize, so it is part frame. One more line off
    // every side the peel moved (an erosion of one pixel) takes it.
    if y0 > oy0 && y0 + 1 < y1 {
        y0 += 1;
    }
    if y1 < oy1 && y1 - 1 > y0 {
        y1 -= 1;
    }
    if x0 > ox0 && x0 + 1 < x1 {
        x0 += 1;
    }
    if x1 < ox1 && x1 - 1 > x0 {
        x1 -= 1;
    }
    Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

/// Does `inner` lie inside `outer`, allowing `tol` pixels of slack on every side?
pub fn rect_within(inner: Rect, outer: Rect, tol: u32) -> bool {
    let t = tol as i64;
    let (ix1, ox1) = (inner.x as i64 + inner.w as i64, outer.x as i64 + outer.w as i64);
    let (iy1, oy1) = (inner.y as i64 + inner.h as i64, outer.y as i64 + outer.h as i64);
    inner.x as i64 + t >= outer.x as i64 && inner.y as i64 + t >= outer.y as i64 && ix1 <= ox1 + t && iy1 <= oy1 + t
}

/// Are all four sides of the two boxes within `tol` pixels of each other?
pub fn rects_near(a: Rect, b: Rect, tol: u32) -> bool {
    let near = |p: u32, q: u32| p.abs_diff(q) <= tol;
    near(a.x, b.x) && near(a.y, b.y) && near(a.x + a.w, b.x + b.w) && near(a.y + a.h, b.y + b.h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: u32, y: u32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn white_bars_are_centred() {
        // (image, trim 1's box, trim 2's box, edges the subject touches, mask box)
        let cases = [
            // 41: 1024x1024, bars 132 px each side, full height, subject on one edge. Trim 2 moved
            // it a little; that is not needed to know these are bars. A lifestyle scene cannot be
            // stretched, so it is centred.
            ("41", r(132, 0, 760, 1024), Some(r(132, 72, 760, 952)), 1, Some(r(234, 72, 482, 952))),
            // 46: 193 px above and 212 below, and 14 px short on the right where the product is
            // pale: a sliver missed on one side does not stop it being bars.
            ("46", r(0, 193, 1010, 619), None, 1, None),
            // 35: 1024x1024, bars 30 and 27 px either side, subject on the top and the bottom
            // edge: centred with padding.
            ("35", r(30, 0, 967, 1024), None, 2, None),
        ];
        for (image, first, second, touched, mask) in cases {
            let (d, why) = decide((1024, 1024), first, second, touched, mask);
            assert_eq!(d, Decision::Centre(first), "image {image}: {why}");
        }
    }

    #[test]
    fn unequal_cuts_are_backdrop_round_an_off_centre_subject_not_bars() {
        // 19: plain grey backdrop, 127 px cut on the left and 6 on the right, subject on the bottom edge.
        // R1 frames it as usual: bottom pinned, subject centred, margin on top.
        let (d, _) = decide((768, 1024), r(127, 16, 635, 1008), None, 1, Some(r(129, 60, 474, 964)));
        assert_eq!(d, Decision::Keep);
    }

    #[test]
    fn a_subject_that_touches_nothing_is_not_centred_on_bars() {
        // Bars, but EIX is 0000: not this rule's business.
        let (d, _) = decide((1024, 1024), r(132, 0, 760, 1024), None, 0, Some(r(300, 100, 300, 800)));
        assert!(!matches!(d, Decision::Centre(_)));
    }

    #[test]
    fn a_thin_ring_round_a_square_picture_is_peeled() {
        // 45: a dark frame ~1% thick all round; the product sits in the middle of the inside.
        let first = r(9, 15, 1008, 1003);
        let (d, why) = decide((1024, 1024), first, None, 0, Some(r(380, 130, 310, 880)));
        assert_eq!(d, Decision::Peel(first), "{why}");
    }

    #[test]
    fn these_are_left_alone() {
        // (why it is left alone, trim 1's box, trim 2's box, edges the subject touches, mask box)
        let cases = [
            // Plain backdrop, product in the middle, trim 1's box is the mask (within the tolerance).
            ("a floating product whose box is the product", r(300, 300, 400, 400), None, 0, Some(r(304, 298, 394, 402))),
            ("a ring when trim two found another layer", r(9, 15, 1008, 1003), Some(r(100, 100, 800, 800)), 0, Some(r(380, 130, 310, 880))),
            ("a box that is not square", r(9, 15, 1008, 700), None, 0, Some(r(380, 130, 310, 500))),
            ("a mask reaching outside the box: the box cut the product", r(9, 15, 1008, 1003), None, 0, Some(r(0, 130, 400, 880))),
            ("nothing cut: nothing to do", r(1, 0, 1022, 1024), None, 1, Some(r(10, 10, 900, 900))),
            // An ordinary studio shot that bleeds off the bottom: backdrop trimmed on three sides.
            ("a subject on an edge with trim on both axes: left to R1", r(200, 80, 600, 944), None, 1, Some(r(210, 90, 580, 934))),
        ];
        for (name, first, second, touched, mask) in cases {
            let (d, why) = decide((1024, 1024), first, second, touched, mask);
            assert_eq!(d, Decision::Keep, "{name}: {why}");
        }
    }

    /// A 100x100 picture: a dark frame 8 px thick round a light inside.
    fn frame_100() -> RgbImage {
        let mut img = RgbImage::from_pixel(100, 100, image::Rgb([20, 20, 20]));
        for y in 8..92 {
            for x in 8..92 {
                img.put_pixel(x, y, image::Rgb([200, 200, 200]));
            }
        }
        img
    }

    #[test]
    fn the_peel_goes_on_inside_a_frame_the_product_runs_into() {
        // The frame of `frame_100`, and a red product from y=30 right through the bottom of the
        // frame. Trim 1 stopped 3 px in on every side, and 3 px from the bottom, where the product
        // merges with the frame.
        let mut img = frame_100();
        let mut mask = Mask { width: 100, height: 100, data: vec![0; 10_000] };
        for y in 30..100 {
            for x in 40..60 {
                img.put_pixel(x, y, image::Rgb([200, 30, 30]));
                mask.data[(y * 100 + x) as usize] = 255;
            }
        }
        assert_eq!(border_colour(&img), [20, 20, 20]);
        // A one-pixel grey line round the very edge (a JPEG edge) must not become "the frame".
        let mut edged = img.clone();
        for i in 0..100 {
            edged.put_pixel(i, 99, image::Rgb([99, 99, 97]));
            edged.put_pixel(99, i, image::Rgb([99, 99, 97]));
        }
        assert_eq!(border_colour(&edged), [20, 20, 20]);
        let peeled = deepen_peel(&img, &mask, r(3, 3, 94, 94), border_colour(&img));
        assert_eq!(peeled, r(9, 9, 82, 82), "the frame is gone on all four sides (and one blurred line more), product columns ignored");
    }

    #[test]
    fn the_peel_works_without_a_product_and_only_for_the_frames_own_colour() {
        let img = frame_100();
        let mask = Mask { width: 100, height: 100, data: vec![0; 10_000] };
        let peeled = deepen_peel(&img, &mask, r(3, 3, 94, 94), [20, 20, 20]);
        assert_eq!(peeled, r(9, 9, 82, 82));
        // A frame colour that does not match peels nothing.
        assert_eq!(deepen_peel(&img, &mask, r(3, 3, 94, 94), [90, 90, 90]), r(3, 3, 94, 94));
    }

    #[test]
    fn the_box_helpers() {
        assert!(rect_within(r(10, 10, 20, 20), r(0, 0, 100, 100), 0));
        assert!(rect_within(r(95, 10, 10, 20), r(0, 0, 100, 100), 5));
        assert!(!rect_within(r(95, 10, 10, 20), r(0, 0, 100, 100), 4));
        assert!(rects_near(r(100, 100, 50, 50), r(102, 98, 48, 54), 4));
        assert!(!rects_near(r(100, 100, 50, 50), r(100, 100, 50, 80), 4));
    }
}
