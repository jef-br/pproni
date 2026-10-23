//! R1 step 2 — build the square.
//!
//! The square starts as a blank canvas with the real part of the picture laid in at its place. What
//! the square reaches past the picture is then filled **one edge at a time, by one function**
//! ([`stretch_edge`]), called for the left, the right, the top and the bottom edge in that order, each
//! only when the square really reaches past that edge. Left and right come first, so the top and
//! bottom edges read rows that are already the full width: that is what fills the corners, which sit
//! outside the picture on both axes at once and so have no real pixels to read on their own.
//!
//! ## One line, one stretch
//!
//! Every line crossing an edge (a row for the left and right edge, a column for the top and bottom
//! edge) is handled on its own, in three parts:
//!
//! * the **product band** — the silhouette grown outward by the clearance — is left untouched;
//! * the **source** is the background between the band and the border of the real picture, read
//!   from the band outward;
//! * the **overhang** is what the square reaches past that border. The source is stretched evenly
//!   ([`stretch_line`]) until it covers the source and the overhang together: one scale for the whole
//!   source, so no part of it is magnified more than the rest. Nothing is invented: no mirror, no blur,
//!   no noise.
//!
//! A line with no source at all (the grown product runs to the border there) is filled flat with the
//! background colour, never black, and then replaced by the nearest clean line's fill
//! ([`borrow_neighbour_lines`]). All of that is the same for all four edges.

use image::{Rgb, RgbImage};

use super::inpaint;
use super::plan::SquarePlan;
use crate::core::shot_classifier::Edge;

/// Compose the square from `src`, stretching background outward wherever the square reaches past a
/// border. `silhouette` holds, per row and per column of `src`, the span the grown silhouette
/// covers. `bgc` is the one dominant background colour the exporter sampled, when present; it fills
/// a line that has no background to stretch.
pub fn compose(src: &RgbImage, plan: &SquarePlan, silhouette: &super::Silhouette, bgc: Option<Rgb<u8>>) -> RgbImage {
    let (w, h) = src.dimensions();
    let Some(real) = plan.real_region(w, h) else {
        return RgbImage::new(plan.side, plan.side);
    };
    let over = plan.overhang(w, h);

    // The canvas, with the real part of the picture at its place. The real part starts where the
    // overhang on the left and on the top ends.
    let mut canvas = RgbImage::new(plan.side, plan.side);
    let (ox, oy) = (over.left as i64, over.top as i64);
    let real_pixels = image::imageops::crop_imm(src, real.x, real.y, real.w, real.h).to_image();
    image::imageops::replace(&mut canvas, &real_pixels, ox, oy);
    let (rw, rh) = (real.w as i64, real.h as i64);

    // Product band per line, in canvas coordinates. Rows: the real rows only. Columns: every column
    // of the square; one past the picture's own columns takes the nearest real column's band, which
    // is where the horizontal edges took their background from.
    let row_bands: Vec<(i64, i64)> = (0..real.h)
        .map(|ry| {
            let (lo, hi) = silhouette.row_x[(real.y + ry) as usize];
            (lo - plan.x, hi - plan.x)
        })
        .collect();
    let col_bands: Vec<(i64, i64)> = (0..plan.side)
        .map(|x| {
            let c = (plan.x + x as i64).clamp(0, w as i64 - 1) as usize;
            let (lo, hi) = silhouette.col_y[c];
            (lo - plan.y, hi - plan.y)
        })
        .collect();

    if over.left > 0 {
        stretch_edge(&mut canvas, &EdgeJob { edge: Edge::Left, first: oy, bands: &row_bands, real: (ox, ox + rw) }, bgc);
    }
    if over.right > 0 {
        stretch_edge(&mut canvas, &EdgeJob { edge: Edge::Right, first: oy, bands: &row_bands, real: (ox, ox + rw) }, bgc);
    }
    if over.top > 0 {
        stretch_edge(&mut canvas, &EdgeJob { edge: Edge::Top, first: 0, bands: &col_bands, real: (oy, oy + rh) }, bgc);
    }
    if over.bottom > 0 {
        stretch_edge(&mut canvas, &EdgeJob { edge: Edge::Bottom, first: 0, bands: &col_bands, real: (oy, oy + rh) }, bgc);
    }
    canvas
}

/// What [`stretch_edge`] needs to know about one edge of the square.
struct EdgeJob<'a> {
    edge: Edge,
    /// The canvas line the first band belongs to: rows for the left and right edge, columns for the
    /// top and bottom edge.
    first: i64,
    /// Per line, the product band `[lo, hi)` along the line, in canvas coordinates.
    bands: &'a [(i64, i64)],
    /// Where the real picture lies along the line, `[lo, hi)`, in canvas coordinates. Everything
    /// outside it, up to the canvas border, is the overhang.
    real: (i64, i64),
}

/// Fill the overhang of one edge of the square. The only place where R1 stretches.
///
/// For every line crossing the edge, the segment between the canvas border and the product band is
/// rewritten: the background between the band and the real border is stretched over all of it. A
/// line that has no background there is filled flat, and then borrows the fill of the nearest line
/// that has one.
///
/// The function does not know which edge it is on beyond how to address a pixel: a line is a row or a
/// column, and it runs outward from the band towards the border at one end or the other.
fn stretch_edge(canvas: &mut RgbImage, job: &EdgeJob<'_>, bgc: Option<Rgb<u8>>) {
    let (r0, r1) = job.real;
    let horizontal = matches!(job.edge, Edge::Left | Edge::Right);
    let low_side = matches!(job.edge, Edge::Left | Edge::Top);
    // Length of a line: the canvas width for a row, its height for a column.
    let side = if horizontal { canvas.width() } else { canvas.height() } as i64;
    // Pixel `pos` along line `line`.
    let at = |line: i64, pos: i64| if horizontal { (pos as u32, line as u32) } else { (line as u32, pos as u32) };
    // Position of the `d`-th pixel outward from the band, whose edge on this side is `edge`.
    let outward = |edge: i64, d: i64| if low_side { edge - 1 - d } else { edge + d };

    // Per line: where the band starts on this side, and how long the segment and its source are.
    let mut seg_len = vec![0i64; job.bands.len()];
    let mut bled = vec![false; job.bands.len()];
    let mut band_edge = vec![0i64; job.bands.len()];

    for (i, &(lo, hi)) in job.bands.iter().enumerate() {
        let line = job.first + i as i64;
        let a = lo.clamp(r0, r1);
        let b = hi.max(a).clamp(r0, r1);
        let edge = if low_side { a } else { b };
        let total = if low_side { a } else { side - b };
        let n = if low_side { a - r0 } else { r1 - b };
        band_edge[i] = edge;
        seg_len[i] = total;
        if total <= 0 {
            continue;
        }
        if n <= 0 {
            // The grown product runs to the border: nothing to stretch. A flat fill for now, never black.
            bled[i] = true;
            let mut flat = vec![Rgb([0u8, 0, 0]); total as usize];
            inpaint::fill_segment(&mut flat, None, None, bgc);
            for (d, px) in flat.into_iter().enumerate() {
                let (x, y) = at(line, outward(edge, d as i64));
                canvas.put_pixel(x, y, px);
            }
            continue;
        }
        let seg: Vec<Rgb<u8>> = (0..n)
            .map(|d| {
                let (x, y) = at(line, outward(edge, d));
                *canvas.get_pixel(x, y)
            })
            .collect();
        for (d, px) in stretch_line(&seg, total as usize).into_iter().enumerate() {
            let (x, y) = at(line, outward(edge, d as i64));
            canvas.put_pixel(x, y, px);
        }
    }
    borrow_neighbour_lines(canvas, job, &bled, &seg_len, &band_edge, &at, &outward);
}

/// Stretch `seg` (the source, ordered from the product band outward) evenly to `total` pixels.
///
/// One scale for the whole source — `total / n` — so no part of it is magnified more than another:
/// the pixel next to the product is the source pixel next to it, and the last one is the border
/// pixel. Reads are cubic (Catmull-Rom), which keeps the edges of the background's texture. When
/// `total` is not more than the source there is nothing to cover, and the nearest `total` pixels are
/// kept as they are.
fn stretch_line(seg: &[Rgb<u8>], total: usize) -> Vec<Rgb<u8>> {
    let n = seg.len();
    if total <= n {
        return seg[..total].to_vec();
    }
    let step = n as f32 / total as f32; // source pixels per output pixel
    (0..total).map(|d| sample_at(seg, (d as f32 + 0.5) * step - 0.5)).collect()
}

/// Cubic (Catmull-Rom) read of `seg` at fractional index `s`. Where the stretch magnifies the most a
/// linear read goes soft and smeared; the cubic keeps the edges of the background's texture. It
/// passes exactly through the source pixels, and the result is clamped to a valid colour.
fn sample_at(seg: &[Rgb<u8>], s: f32) -> Rgb<u8> {
    let n = seg.len();
    let s = s.clamp(0.0, n as f32 - 1.0);
    let i = (s.floor() as usize).min(n - 1);
    let t = s - i as f32;
    let at = |k: isize| seg[(k.clamp(0, n as isize - 1)) as usize];
    let (p0, p1, p2, p3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
    let cubic = |c: usize| {
        let (a, b, cc, d) = (p0[c] as f32, p1[c] as f32, p2[c] as f32, p3[c] as f32);
        let v = 0.5
            * (2.0 * b + (cc - a) * t + (2.0 * a - 5.0 * b + 4.0 * cc - d) * t * t + (-a + 3.0 * b - 3.0 * cc + d) * t * t * t);
        v.round().clamp(0.0, 255.0) as u8
    };
    Rgb([cubic(0), cubic(1), cubic(2)])
}

/// Plan A.5 neighbour-line borrow, for one edge. A line that had no background (`bled`) holds a flat
/// BGC/white fill; each of its pixels is replaced by a blend of the pixels at the same distance from
/// the border on the clean lines either side of it (the nearest line that did have background on each
/// side), weighted by how close the line is to each. A run of bled lines therefore becomes a smooth
/// ramp from the clean line before it to the one after it, not two blocks with a step in the middle.
/// With a clean line on one side only, that line is copied. A line without a clean neighbour anywhere
/// along the edge keeps its flat fill. Only bled pixels are ever written, so every stretched or copied
/// pixel is byte-for-byte unchanged.
fn borrow_neighbour_lines(
    canvas: &mut RgbImage,
    job: &EdgeJob<'_>,
    bled: &[bool],
    seg_len: &[i64],
    band_edge: &[i64],
    at: &dyn Fn(i64, i64) -> (u32, u32),
    outward: &dyn Fn(i64, i64) -> i64,
) {
    let n = bled.len();
    if !bled.iter().any(|&b| b) {
        return;
    }
    // Nearest clean line at or before / at or after each line, by two sweeps.
    let mut before = vec![None; n];
    let mut last = None;
    for i in 0..n {
        if !bled[i] {
            last = Some(i);
        }
        before[i] = last;
    }
    let mut after = vec![None; n];
    let mut next = None;
    for i in (0..n).rev() {
        if !bled[i] {
            next = Some(i);
        }
        after[i] = next;
    }
    for i in 0..n {
        if !bled[i] {
            continue;
        }
        // The clean lines either side, and how far this line is from the first towards the second.
        let (a, b, t) = match (before[i], after[i]) {
            (Some(a), Some(b)) => (a, b, (i - a) as f32 / (b - a) as f32),
            (Some(a), None) => (a, a, 0.0),
            (None, Some(b)) => (b, b, 0.0),
            (None, None) => continue, // the whole edge is bled: keep the BGC/white fill
        };
        let line = job.first + i as i64;
        let (donor_a, donor_b) = (job.first + a as i64, job.first + b as i64);
        for d in 0..seg_len[i] {
            // Same position on the donor lines. They have background here: their segments are the
            // longer ones, so they cover every position of this line's segment.
            let pos = outward(band_edge[i], d);
            let (ax, ay) = at(donor_a, pos);
            let (bx, by) = at(donor_b, pos);
            let (pa, pb) = (*canvas.get_pixel(ax, ay), *canvas.get_pixel(bx, by));
            let px = Rgb([0, 1, 2].map(|c| (pa[c] as f32 + (pb[c] as f32 - pa[c] as f32) * t).round() as u8));
            let (x, y) = at(line, pos);
            canvas.put_pixel(x, y, px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w x h` picture whose red channel is the column index, so a read shows where it came from.
    fn ramp(w: u32, h: u32) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, Rgb([x as u8, 100, 100]));
            }
        }
        img
    }

    fn reds(seg: &[Rgb<u8>]) -> Vec<u8> {
        seg.iter().map(|p| p[0]).collect()
    }

    #[test]
    fn the_stretch_is_even_and_ends_on_the_border_pixel() {
        // Source value = distance from the product (0..60), covered over 120 pixels: one scale of 2.
        let seg: Vec<Rgb<u8>> = (0..60).map(|i| Rgb([i as u8, 0, 0])).collect();
        let out = reds(&stretch_line(&seg, 120));
        assert_eq!(out.len(), 120);
        assert_eq!(out[0], 0, "the pixel next to the product is the source pixel next to it");
        assert_eq!(out[119], 59, "the last pixel is the border pixel");
        assert!(out.windows(2).all(|p| p[0] <= p[1]), "the stretch never reverses");
        // Even: the source advances at the same rate everywhere, 10 output pixels = 5 source pixels.
        for i in 0..100 {
            let step = out[i + 10] as i32 - out[i] as i32;
            assert!((4..=6).contains(&step), "uneven stretch at {i}: advanced {step} in 10");
        }
    }

    #[test]
    fn a_stretch_invents_nothing_and_stays_inside_the_range_of_the_source() {
        let seg: Vec<Rgb<u8>> = (0..7).map(|i| Rgb([100 + i as u8 * 3, 50, 200])).collect();
        for p in stretch_line(&seg, 40) {
            assert!((100..=118).contains(&p[0]), "outside the source range: {}", p[0]);
            assert_eq!((p[1], p[2]), (50, 200));
        }
    }

    #[test]
    fn no_overhang_keeps_the_nearest_pixels_as_they_are() {
        let seg: Vec<Rgb<u8>> = (0..10).map(|i| Rgb([i as u8, 0, 0])).collect();
        assert_eq!(reds(&stretch_line(&seg, 10)), (0..10).collect::<Vec<u8>>());
        assert_eq!(reds(&stretch_line(&seg, 4)), vec![0, 1, 2, 3]);
    }

    /// A `side x side` canvas for the left edge: real pixels from column `r0` on (value = a function of
    /// the position), nothing before it. Rows 10..13 have a band that starts at the border of the real
    /// picture, so they have no source.
    fn left_scene(side: u32, r0: i64) -> (RgbImage, Vec<(i64, i64)>) {
        let mut img = RgbImage::new(side, side);
        for y in 0..side {
            for x in r0 as u32..side {
                let v = ((x * 37 + y * 101) % 211) as u8 + 20;
                img.put_pixel(x, y, Rgb([v, v / 2 + 5, 255 - v]));
            }
        }
        let bands = (0..side as i64)
            .map(|y| if (10..13).contains(&y) { (r0, side as i64) } else { (r0 + 8 + y % 7, side as i64) })
            .collect();
        (img, bands)
    }

    fn flip_h(img: &RgbImage) -> RgbImage {
        image::imageops::flip_horizontal(img)
    }

    fn transposed(img: &RgbImage) -> RgbImage {
        let (w, h) = img.dimensions();
        let mut t = RgbImage::new(h, w);
        for y in 0..h {
            for x in 0..w {
                t.put_pixel(y, x, *img.get_pixel(x, y));
            }
        }
        t
    }

    #[test]
    fn a_line_with_no_background_fills_bgc_never_black() {
        // det3/det4: the product runs to the border, so the left side has no source at all. The old
        // code left that overhang black.
        let mut canvas = RgbImage::from_pixel(20, 1, Rgb([200, 20, 20]));
        for x in 0..5 {
            canvas.put_pixel(x, 0, Rgb([0, 0, 0]));
        }
        let white = Rgb([255, 255, 255]);
        let job = EdgeJob { edge: Edge::Left, first: 0, bands: &[(5, 20)], real: (5, 20) };
        stretch_edge(&mut canvas, &job, Some(white));
        for x in 0..5u32 {
            let p = *canvas.get_pixel(x, 0);
            assert!(p.0 != [0, 0, 0], "overhang left black at {x}");
            assert!(p.0.iter().all(|&c| c >= 240), "overhang not BGC-white at {x}: {p:?}");
        }
        for x in 5..20u32 {
            assert_eq!(*canvas.get_pixel(x, 0), Rgb([200, 20, 20]), "product band altered at {x}");
        }
    }

    #[test]
    fn a_line_with_no_background_and_no_bgc_fills_white() {
        let mut canvas = RgbImage::from_pixel(20, 1, Rgb([10, 90, 10]));
        let job = EdgeJob { edge: Edge::Right, first: 0, bands: &[(0, 15)], real: (0, 15) };
        stretch_edge(&mut canvas, &job, None);
        for x in 15..20u32 {
            assert!(canvas.get_pixel(x, 0).0.iter().all(|&c| c >= 240), "not white at {x}");
        }
    }

    #[test]
    fn a_bled_line_between_two_clean_lines_is_their_blend() {
        // Three rows, real picture from column 4. Row 1 has no source (its band starts at 4), rows 0 and
        // 2 have one (150 and 100). After the edge is done, row 1's overhang (columns 0..4) is halfway.
        let mut canvas = RgbImage::new(12, 3);
        for y in 0..3 {
            for x in 4..12 {
                canvas.put_pixel(x, y, Rgb([100 + (y == 0) as u8 * 50, 100, 100]));
            }
        }
        let bands = [(8i64, 12i64), (4, 12), (8, 12)];
        stretch_edge(&mut canvas, &EdgeJob { edge: Edge::Left, first: 0, bands: &bands, real: (4, 12) }, None);
        for x in 0..4u32 {
            assert_eq!(canvas.get_pixel(x, 1)[0], 125, "row 1 is the mean of rows 0 and 2 at {x}");
        }
        // Rows 0 and 2 are untouched by the borrow: their overhang is their own stretch.
        assert_eq!(canvas.get_pixel(0, 0)[0], 150);
        assert_eq!(canvas.get_pixel(0, 2)[0], 100);
    }

    #[test]
    fn a_run_of_bled_lines_ramps_from_the_clean_line_before_to_the_one_after() {
        // Row 0 is 100, row 4 is 200, rows 1..4 are bled: 125, 150, 175, with no step in the middle.
        let mut canvas = RgbImage::new(10, 5);
        for y in 0..5 {
            for x in 4..10 {
                canvas.put_pixel(x, y, Rgb([if y == 0 { 100 } else { 200 }, 0, 0]));
            }
        }
        let bands = [(7i64, 10i64), (4, 10), (4, 10), (4, 10), (7, 10)];
        stretch_edge(&mut canvas, &EdgeJob { edge: Edge::Left, first: 0, bands: &bands, real: (4, 10) }, None);
        let col0: Vec<u8> = (0..5).map(|y| canvas.get_pixel(0, y)[0]).collect();
        assert_eq!(col0, vec![100, 125, 150, 175, 200]);
    }

    #[test]
    fn a_bled_line_at_the_end_of_the_edge_copies_its_only_clean_neighbour() {
        let mut canvas = RgbImage::new(10, 3);
        for y in 0..3 {
            for x in 4..10 {
                canvas.put_pixel(x, y, Rgb([90, 0, 0]));
            }
        }
        let bands = [(7i64, 10i64), (4, 10), (4, 10)];
        stretch_edge(&mut canvas, &EdgeJob { edge: Edge::Left, first: 0, bands: &bands, real: (4, 10) }, None);
        for y in 1..3 {
            assert_eq!(canvas.get_pixel(0, y)[0], 90, "row {y} copies row 0");
        }
    }

    #[test]
    fn a_fully_bled_edge_keeps_its_flat_fill() {
        let mut canvas = RgbImage::new(10, 3);
        for y in 0..3 {
            for x in 4..10 {
                canvas.put_pixel(x, y, Rgb([100, 100, 100]));
            }
        }
        let bands = [(4i64, 10i64); 3];
        stretch_edge(&mut canvas, &EdgeJob { edge: Edge::Left, first: 0, bands: &bands, real: (4, 10) }, None);
        for y in 0..3 {
            for x in 0..4u32 {
                assert!(canvas.get_pixel(x, y).0.iter().all(|&c| c >= 240), "no neighbour, so white at ({x},{y})");
            }
        }
    }

    #[test]
    fn all_four_edges_behave_identically() {
        // The same scene, turned so that each edge in turn plays the left edge, must give the same
        // result turned back: left, right (mirrored), top (transposed) and bottom (both).
        let side = 40u32;
        let r0 = 12i64;
        let (scene, bands) = left_scene(side, r0);
        let bgc = Some(Rgb([240, 241, 242]));
        let s = side as i64;

        let mut left = scene.clone();
        stretch_edge(&mut left, &EdgeJob { edge: Edge::Left, first: 0, bands: &bands, real: (r0, s) }, bgc);

        // Right: mirror the scene, bands and real span.
        let mut right = flip_h(&scene);
        let right_bands: Vec<(i64, i64)> = bands.iter().map(|&(lo, hi)| (s - hi, s - lo)).collect();
        stretch_edge(&mut right, &EdgeJob { edge: Edge::Right, first: 0, bands: &right_bands, real: (0, s - r0) }, bgc);
        assert_eq!(flip_h(&right), left, "right edge differs from the left edge");

        // Top: transpose, so rows become columns.
        let mut top = transposed(&scene);
        stretch_edge(&mut top, &EdgeJob { edge: Edge::Top, first: 0, bands: &bands, real: (r0, s) }, bgc);
        assert_eq!(transposed(&top), left, "top edge differs from the left edge");

        // Bottom: transpose, then mirror the line axis.
        let mut bottom = flip_v(&transposed(&scene));
        stretch_edge(&mut bottom, &EdgeJob { edge: Edge::Bottom, first: 0, bands: &right_bands, real: (0, s - r0) }, bgc);
        assert_eq!(transposed(&flip_v(&bottom)), left, "bottom edge differs from the left edge");
    }

    fn flip_v(img: &RgbImage) -> RgbImage {
        image::imageops::flip_vertical(img)
    }
}
