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
//! The background is taken as trustworthy from the grown silhouette (see [`Silhouette`]) all the
//! way to the border. Two validations of it were designed and then *deferred*, not forgotten:
//! trimming a flat, artificial background run off the outer end, and advancing the inner end past a
//! cast shadow the mask does not cover. Neither is worth its complexity until an image argues for
//! it. The argument will look like a smeared shadow, or a stretched band of the wrong colour along
//! one side.

pub mod compose;
pub mod extend;
pub mod inpaint;
pub mod plan;

use super::super::super::config;
use super::super::super::preprocessor::{Framing, Prepared};
use super::{fallback, Shot};
use crate::core::shot_classifier::{geometry::Rect, Edge, Mask};
use crate::core::trace;
use image::RgbImage;
use plan::{Pins, SquarePlan};

/// The product silhouette, grown outward, as the span it covers on every row and every column of
/// the *original* image. The grown silhouette is the product band: copied untouched, never a stretch
/// source. Growing it is what keeps a cuff, an edge or a tiny shadow hugging the product out of the
/// stretch (plan A.2).
///
/// Built once from the working-resolution mask, scaled up, then grown by `grow` pixels in every
/// direction (a disc, not a square). Rows or columns the grown shape does not reach take the span of
/// the nearest one that has it, so the product band changes smoothly from one line to the next
/// instead of jumping between the silhouette and the bounding box.
pub struct Silhouette {
    /// Indexed by original-image `y`: the columns `[lo, hi)` the grown silhouette covers.
    pub row_x: Vec<(i64, i64)>,
    /// Indexed by original-image `x`: the rows `[lo, hi)` the grown silhouette covers.
    pub col_y: Vec<(i64, i64)>,
}

impl Silhouette {
    pub fn from_mask(mask: &Mask, orig_w: u32, orig_h: u32, grow: u32) -> Silhouette {
        let (rows, cols) = spans_of(mask, orig_w, orig_h);
        let slope = config::R1_BAND_SLOPE_PX_PER_LINE as i64;
        Silhouette { row_x: grow_spans(&rows, grow as i64, slope), col_y: grow_spans(&cols, grow as i64, slope) }
    }
}

/// Per row and per column of the original, the span `(lo, hi)` (inclusive) the mask covers, or `None`
/// where it covers nothing.
///
/// The mask is at most `WORKING_SIZE` on its longest side and the original can be many times that, so
/// the mask is read once, in its own pixels, and the answer is carried over to the original's lines.
/// An original pixel `ox` reads mask column `mx_of(ox)`, which never goes down as `ox` goes up, so the
/// originals that read one mask column are one unbroken run: a mask row's span is the first original
/// of the run of its lowest set column and the last original of the run of its highest.
fn spans_of(mask: &Mask, orig_w: u32, orig_h: u32) -> (Vec<Option<(i64, i64)>>, Vec<Option<(i64, i64)>>) {
    let (mw, mh) = (mask.width as usize, mask.height as usize);
    if mw == 0 || mh == 0 {
        return (vec![None; orig_h as usize], vec![None; orig_w as usize]);
    }
    let fx = mask.width as f32 / orig_w.max(1) as f32;
    let fy = mask.height as f32 / orig_h.max(1) as f32;
    let mask_x = |ox: u32| (((ox as f32 + 0.5) * fx) as u32).min(mask.width.saturating_sub(1)) as usize;
    let mask_y = |oy: u32| (((oy as f32 + 0.5) * fy) as u32).min(mask.height.saturating_sub(1)) as usize;

    // The run of original columns (rows) that reads each mask column (row): first and last.
    let mut x_run = vec![(i64::MAX, i64::MIN); mw];
    for ox in 0..orig_w {
        let run = &mut x_run[mask_x(ox)];
        *run = (run.0.min(ox as i64), run.1.max(ox as i64));
    }
    let mut y_run = vec![(i64::MAX, i64::MIN); mh];
    for oy in 0..orig_h {
        let run = &mut y_run[mask_y(oy)];
        *run = (run.0.min(oy as i64), run.1.max(oy as i64));
    }

    // Lowest and highest set mask column of every mask row, and set row of every mask column. Only
    // mask pixels some original pixel reads count (all of them, unless the mask is the larger one).
    let mut row_set: Vec<Option<(usize, usize)>> = vec![None; mh];
    let mut col_set: Vec<Option<(usize, usize)>> = vec![None; mw];
    for (y, line) in mask.data.chunks_exact(mw).enumerate().take(mh) {
        if y_run[y].0 == i64::MAX {
            continue;
        }
        for (x, &v) in line.iter().enumerate() {
            if v != 0 && x_run[x].0 != i64::MAX {
                let r = &mut row_set[y];
                *r = Some(r.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x))));
                let c = &mut col_set[x];
                *c = Some(c.map_or((y, y), |(lo, hi)| (lo.min(y), hi.max(y))));
            }
        }
    }

    let rows = (0..orig_h).map(|oy| row_set[mask_y(oy)].map(|(lo, hi)| (x_run[lo].0, x_run[hi].1))).collect();
    let cols = (0..orig_w).map(|ox| col_set[mask_x(ox)].map(|(lo, hi)| (y_run[lo].0, y_run[hi].1))).collect();
    (rows, cols)
}

/// Grow per-line spans `(lo, hi)` (inclusive) by a disc of radius `r` and return them as `[lo, hi)`.
///
/// A line's grown span is the union of the spans of the lines within `r` of it, each widened by the
/// disc's reach at that distance. Lines the disc does not reach take the span of the nearest line
/// that has one (the earlier one on a tie); with no span anywhere they come out empty. Last, no edge
/// is allowed to move faster than `slope` pixels per line: an edge that would is pulled outward from
/// the line where it jumps, so the span only ever widens.
fn grow_spans(spans: &[Option<(i64, i64)>], r: i64, slope: i64) -> Vec<(i64, i64)> {
    let n = spans.len() as i64;
    let reach: Vec<i64> = (0..=r).map(|d| ((r * r - d * d) as f64).sqrt().floor() as i64).collect();
    let mut grown: Vec<Option<(i64, i64)>> = vec![None; spans.len()];
    for y in 0..n {
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        for d in -r..=r {
            let yy = y + d;
            if yy < 0 || yy >= n {
                continue;
            }
            if let Some((l, h)) = spans[yy as usize] {
                let e = reach[d.unsigned_abs() as usize];
                lo = lo.min(l - e);
                hi = hi.max(h + e);
            }
        }
        if lo <= hi {
            grown[y as usize] = Some((lo, hi + 1));
        }
    }
    // Nearest line that has a span, from both directions.
    let mut before: Vec<Option<usize>> = vec![None; spans.len()];
    let mut last = None;
    for (i, g) in grown.iter().enumerate() {
        if g.is_some() {
            last = Some(i);
        }
        before[i] = last;
    }
    let mut after: Vec<Option<usize>> = vec![None; spans.len()];
    let mut next = None;
    for (i, g) in grown.iter().enumerate().rev() {
        if g.is_some() {
            next = Some(i);
        }
        after[i] = next;
    }
    let mut out: Vec<(i64, i64)> = (0..spans.len())
        .map(|i| {
            let pick = match (before[i], after[i]) {
                (Some(b), Some(a)) => if i - b <= a - i { b } else { a },
                (Some(b), None) => b,
                (None, Some(a)) => a,
                (None, None) => return (0, 0),
            };
            grown[pick].unwrap_or((0, 0))
        })
        .collect();
    // Slope limit: a cone along the lines, `lo` pulled down and `hi` pushed up (forward, then back).
    // Lines with no span anywhere (all `(0, 0)`) are left alone.
    if grown.iter().any(|g| g.is_some()) {
        for i in 1..out.len() {
            out[i].0 = out[i].0.min(out[i - 1].0 + slope);
            out[i].1 = out[i].1.max(out[i - 1].1 - slope);
        }
        for i in (0..out.len().saturating_sub(1)).rev() {
            out[i].0 = out[i].0.min(out[i + 1].0 + slope);
            out[i].1 = out[i].1.max(out[i + 1].1 - slope);
        }
    }
    out
}

/// How many full-resolution pixels the silhouette is grown by: `R1_FILL_SOURCE_CLEARANCE_WORKING_PX`
/// scaled up to original pixels (plan A.2).
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
    // A textured scene with flat letterbox/pillarbox bars must be re-framed, not stretched: R1's
    // stretch invents a smeared band across a textured overhang (measured on 268/46). The
    // preprocessor's detector (`prep.letterbox`) fires only on genuine bars-plus-textured-interior —
    // a freestanding product on a plain backdrop has flat edges on both axes, so it never fires and
    // `EIX0000` cannot flip. Runs after the subject guard so the mask gives the real subject box.
    if let Some(keep) = prep.letterbox {
        trace::note(|| format!("R1 → Part C: textured scene, reframing on the trimmed {keep:?} rather than stretching"));
        let trimmed = image::imageops::crop_imm(&prep.original, keep.x, keep.y, keep.w, keep.h).to_image();
        return extend::reframe_to_square(&trimmed, subject_into(subject, keep));
    }
    // `None` means no square can respect the bled edges — R3 frames the image safely rather than R1
    // guessing.
    let Some(plan) = plan_square(prep, subject, &shot.class.touches_edges) else {
        trace::note(|| "R1: no square can respect the bled edges: using R3 framing".into());
        return fallback::apply(prep);
    };
    let silhouette =
        Silhouette::from_mask(shot.mask, prep.original.width(), prep.original.height(), clearance_px(prep));
    compose::compose(&prep.original, &plan, &silhouette, shot.bgc)
}

/// Where R1's square goes.
///
/// Normally it is planned from the subject: the edges the subject bleeds off are the ones the square
/// may not move ([`SquarePlan::new`]). When `core::framing` found white bars on one axis
/// ([`Framing::CentreContent`]) it is cut from the content box instead: as wide as the box's longer
/// side, no margin, centred on the box — equal padding either side of the shorter axis, aligned on
/// nothing. Whatever of the square falls outside the picture is filled as always.
pub fn plan_square(prep: &Prepared, subject: Rect, touches: &[Edge]) -> Option<SquarePlan> {
    if let Framing::CentreContent(content) = prep.framing {
        let (fx, fy) = (
            prep.original.width() as f32 / prep.working.width().max(1) as f32,
            prep.original.height() as f32 / prep.working.height().max(1) as f32,
        );
        let (x0, y0) = ((content.x as f32 * fx).round() as i64, (content.y as f32 * fy).round() as i64);
        let (w, h) = ((content.w as f32 * fx).round() as i64, (content.h as f32 * fy).round() as i64);
        let side = w.max(h).max(1);
        return Some(SquarePlan {
            side: side as u32,
            x: x0 + (w - side).div_euclid(2),
            y: y0 + (h - side).div_euclid(2),
        });
    }
    SquarePlan::new(subject, config::R1_MARGIN, Pins::from_edges(touches))
}

/// Re-express `subject` (original coordinates) relative to the trimmed `keep` sub-rect, clamped to
/// it. The trimmed bars are background, so the subject normally lies wholly inside `keep`; the
/// clamp only guards a subject box that grazes a trimmed band.
fn subject_into(subject: Rect, keep: Rect) -> Rect {
    let x0 = subject.x.max(keep.x);
    let y0 = subject.y.max(keep.y);
    let x1 = (subject.x + subject.w).min(keep.x + keep.w);
    let y1 = (subject.y + subject.h).min(keep.y + keep.h);
    Rect { x: x0 - keep.x, y: y0 - keep.y, w: x1.saturating_sub(x0), h: y1.saturating_sub(y0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::preprocessor::{Framing, Trims};
    use crate::core::shot_classifier::{geometry::Rect, Mask, ShotClassification};
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
        let prep = Prepared { original: img.clone(), working: img.clone(), model_input: img.clone(), crop: Rect { x: 0, y: 0, w: img.width(), h: img.height() }, trims: Trims::NONE, framing: Framing::Free, letterbox: None };
        let class = ShotClassification::default();
        apply(&prep, Shot { class: &class, mask, bgc: None })
    }

    /// Build the square for `img` straight through `compose`, growing the silhouette by only 2 px so
    /// that small synthetic scenes keep some background to stretch.
    fn compose_scene(img: &RgbImage, mask: &Mask, bgc: Option<Rgb<u8>>) -> RgbImage {
        let class = ShotClassification::default();
        let subject = Shot { class: &class, mask, bgc }.subject_bbox().expect("a subject");
        let plan = SquarePlan::new(subject, config::R1_MARGIN, Pins::NONE).expect("a legal square");
        let silhouette = Silhouette::from_mask(mask, img.width(), img.height(), 2);
        compose::compose(img, &plan, &silhouette, bgc)
    }

    #[test]
    fn silhouette_scales_row_extents_from_working_to_original() {
        // 10x10 working mask, subject columns 2..=5 on rows 3..=6. Original is 20x20 (2x), so the
        // subject covers original rows 6..=13.
        let mut mask = Mask { width: 10, height: 10, data: vec![0u8; 100] };
        for y in 3..=6 {
            for x in 2..=5 {
                mask.data[(y * 10 + x) as usize] = 255;
            }
        }
        let s = Silhouette::from_mask(&mask, 20, 20, 0);
        // Original row 8 maps to working row 4; columns 2..=5 at 2x cover ~4..=11, so [4, 12).
        let (lo, hi) = s.row_x[8];
        assert!(lo <= 5 && hi >= 11, "expected ~[4, 12), got [{lo}, {hi})");
        // Rows above and below the subject take the nearest subject row's span.
        assert_eq!(s.row_x[0], s.row_x[6]);
        assert_eq!(s.row_x[19], s.row_x[13]);
        // Columns the same way.
        assert_eq!(s.col_y[0], s.col_y[4]);
    }

    /// A slope so steep it never limits anything, for the tests that are not about it.
    const NO_SLOPE: i64 = 1_000_000;

    #[test]
    fn an_edge_that_jumps_between_two_lines_is_eased_in_at_the_set_slope() {
        // Two shoes side by side: line 9 holds only the left one (span 0..=10), line 10 on also the
        // right one (span 0..=500). Without a slope limit the right edge jumps 490 px in one line.
        let mut spans: Vec<Option<(i64, i64)>> = vec![Some((0, 10)); 10];
        spans.extend(vec![Some((0, 500)); 10]);
        let jumpy = grow_spans(&spans, 0, NO_SLOPE);
        assert_eq!((jumpy[9].1, jumpy[10].1), (11, 501), "a jump of 490");
        let eased = grow_spans(&spans, 0, 20);
        assert!(eased.windows(2).all(|w| (w[0].1 - w[1].1).abs() <= 20), "no edge moves faster than 20 per line");
        assert!(eased.iter().zip(&jumpy).all(|(e, j)| e.0 <= j.0 && e.1 >= j.1), "the span only ever widens");
        assert_eq!(eased[19].1, 501, "and the far end is untouched");
    }

    #[test]
    fn growing_widens_a_span_by_the_discs_reach_and_far_lines_take_the_nearest_span() {
        let g = grow_spans(&[None, None, Some((10, 12)), None, None], 2, NO_SLOPE);
        // On the line itself the disc reaches 2 each way; one line away floor(sqrt(3)) = 1; two away 0.
        assert_eq!(g[2], (8, 15));
        assert_eq!(g[1], (9, 14));
        assert_eq!(g[3], (9, 14));
        assert_eq!(g[0], (10, 13));
        assert_eq!(g[4], (10, 13));

        // Lines the disc does not reach take the nearest span.
        let g = grow_spans(&[None, Some((5, 6)), None, None, None, None, None, None], 1, NO_SLOPE);
        assert_eq!(g[1], (4, 8));
        assert_eq!(g[2], (5, 7));
        assert_eq!(g[7], g[2], "a far line borrows the nearest grown line");
        assert_eq!(g[4], g[2]);
    }

    #[test]
    fn white_bars_are_centred_with_equal_padding_and_not_aligned_on_the_subject() {
        // A 300x400 picture with 100 px bars each side of a 100 px wide content box, full height.
        let img = RgbImage::from_pixel(300, 400, Rgb([255, 255, 255]));
        let mut prep = Prepared {
            original: img.clone(),
            working: img.clone(),
            model_input: img.clone(),
            crop: Rect { x: 0, y: 0, w: 300, h: 400 },
            trims: Trims::NONE,
            framing: Framing::CentreContent(Rect { x: 100, y: 0, w: 100, h: 400 }),
            letterbox: None,
        };
        // The subject hugs the right of the content: a subject-aligned plan would not be centred.
        let subject = Rect { x: 150, y: 100, w: 50, h: 300 };
        let plan = plan_square(&prep, subject, &[Edge::Bottom]).unwrap();
        assert_eq!((plan.side, plan.x, plan.y), (400, -50, 0), "400 square, 150 px of padding split 75/75 about the box");
        // Scaled the same way when the working copy is smaller than the original.
        prep.working = RgbImage::from_pixel(150, 200, Rgb([255, 255, 255]));
        prep.framing = Framing::CentreContent(Rect { x: 50, y: 0, w: 50, h: 200 });
        let plan = plan_square(&prep, subject, &[Edge::Bottom]).unwrap();
        assert_eq!((plan.side, plan.x, plan.y), (400, -50, 0));
        // Without the framing the subject decides, as before.
        prep.framing = Framing::Free;
        let plan = plan_square(&prep, subject, &[Edge::Bottom]).unwrap();
        assert_ne!((plan.side, plan.x), (400, -50));
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

        // A square subject already in the middle: every side lands on real pixels, so it is a pure
        // crop, nothing resampled. A uniform background stays exactly its own value.
        let (img, mask) = scene(400, 400, Rect { x: 150, y: 150, w: 100, h: 100 }, [240, 240, 240], [0, 0, 0]);
        let out = run(img, &mask);
        assert_eq!(out.dimensions(), (108, 108));
        assert_eq!(*out.get_pixel(52, 52), Rgb([0, 0, 0]));
        assert_eq!(*out.get_pixel(0, 0), Rgb([240, 240, 240]));
    }

    #[test]
    fn a_subject_wider_than_the_frame_is_filled_on_the_narrow_axis() {
        // 200x100 frame, subject nearly filling it: the square is taller than the image, so the
        // top and bottom overhangs are filled. Corners come out background — the proof that the
        // two-pass order fills them, since they are outside the image on both axes.
        let (img, mask) = scene(200, 100, Rect { x: 40, y: 20, w: 120, h: 60 }, [180, 190, 200], [30, 40, 50]);
        let out = compose_scene(&img, &mask, None);
        assert_eq!(out.dimensions(), (130, 130));
        assert_eq!(*out.get_pixel(2, 2), Rgb([180, 190, 200]), "top-left corner");
        assert_eq!(*out.get_pixel(122, 122), Rgb([180, 190, 200]), "bottom-right corner");
        assert_eq!(*out.get_pixel(62, 62), Rgb([30, 40, 50]), "centre is still subject");
    }

    #[test]
    fn a_gradient_background_keeps_its_gradient_next_to_the_product() {
        // The case a flat fill loses. The ramp runs left-to-right and the fill is horizontal, so the
        // left overhang must continue the ramp rather than settle at one value.
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
        let out = compose_scene(&img, &mask, None);
        assert_eq!(out.dimensions(), (217, 217), "200 * 1.084 = 216.8");

        // The square starts 49 columns left of the image and 91 rows down. The product band is the
        // silhouette grown by 2: source columns 18..102, output columns 67.. — an untouched copy.
        let row = out.height() / 2;
        for x in 67..140u32 {
            assert_eq!(out.get_pixel(x, row), img.get_pixel(x - 49, row + 91), "band altered at {x}");
        }
        // The pixel next to the band continues the ramp from source column 17 (value 36).
        let next_to_band = out.get_pixel(66, row)[0];
        assert!((30..=40).contains(&next_to_band), "ramp not continued next to the product: {next_to_band}");
        // And nothing in the left overhang leaves the source's own range, 0..=36.
        for x in 0..67u32 {
            let v = out.get_pixel(x, row)[0];
            assert!(v <= 36, "overhang pixel {x} outside the source range: {v}");
        }
    }

    #[test]
    fn a_textured_pillarbox_scene_reframes_instead_of_stretching() {
        // A white 10px bar on the left, a black one on the right (two bars of one colour would read
        // as the backdrop and not fire), a pseudo-random textured interior between — so the
        // preprocessor's letterbox detector fires and R1 must reframe (crop | seam-carve |
        // white-pad) rather than stretch the textured overhang.
        let mut img = RgbImage::from_pixel(60, 60, Rgb([255, 255, 255]));
        for y in 0..60u32 {
            for x in 10..60u32 {
                let v = if x >= 50 { 0 } else { ((x * 37 + y * 101) % 256) as u8 };
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        // A tall subject in the interior, so a square crop cannot preserve it.
        let subject = Rect { x: 20, y: 3, w: 12, h: 54 };
        let mut mask = Mask { width: 60, height: 60, data: vec![0u8; 60 * 60] };
        for y in subject.y..subject.y + subject.h {
            for x in subject.x..subject.x + subject.w {
                mask.data[(y * 60 + x) as usize] = 255;
            }
        }
        let keep = crate::core::preprocessor::optimizer::trim_letterbox(&img).expect("pillarbox fires");
        let prep = Prepared {
            original: img.clone(),
            working: img.clone(),
            model_input: img.clone(),
            crop: Rect { x: 0, y: 0, w: 60, h: 60 },
            trims: Trims::NONE, framing: Framing::Free,
            letterbox: Some(keep),
        };
        let class = ShotClassification::default();
        let out = apply(&prep, Shot { class: &class, mask: &mask, bgc: None });

        // The route output is exactly the reframe of the trimmed interior — proof it took the Part C
        // path, not R1's stretch, whichever reframe priority won.
        let trimmed = image::imageops::crop_imm(&img, keep.x, keep.y, keep.w, keep.h).to_image();
        let bbox = Shot { class: &class, mask: &mask, bgc: None }.subject_bbox_in(&prep).unwrap();
        let expected = extend::reframe_to_square(&trimmed, subject_into(bbox, keep));
        assert_eq!(out, expected, "a textured scene must reframe, not R1-stretch");
        assert_eq!(out.width(), out.height(), "reframed output is square");
    }

    /// What `spans_of` replaced: ask the mask about every original pixel. Kept as the reference.
    fn spans_by_asking_every_pixel(mask: &Mask, orig_w: u32, orig_h: u32) -> (Vec<Option<(i64, i64)>>, Vec<Option<(i64, i64)>>) {
        let fx = mask.width as f32 / orig_w.max(1) as f32;
        let fy = mask.height as f32 / orig_h.max(1) as f32;
        let mut rows: Vec<Option<(i64, i64)>> = vec![None; orig_h as usize];
        let mut cols: Vec<Option<(i64, i64)>> = vec![None; orig_w as usize];
        for oy in 0..orig_h {
            let my = (((oy as f32 + 0.5) * fy) as u32).min(mask.height.saturating_sub(1));
            for ox in 0..orig_w {
                let mx = (((ox as f32 + 0.5) * fx) as u32).min(mask.width.saturating_sub(1));
                if mask.get(mx, my) {
                    let (x, y) = (ox as i64, oy as i64);
                    let r = &mut rows[oy as usize];
                    *r = Some(r.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x))));
                    let c = &mut cols[ox as usize];
                    *c = Some(c.map_or((y, y), |(lo, hi)| (lo.min(y), hi.max(y))));
                }
            }
        }
        (rows, cols)
    }

    #[test]
    fn reading_the_mask_once_gives_the_spans_of_asking_every_pixel() {
        // Masks of every kind against originals of every ratio, including a mask larger than the
        // original and one with no pixel set.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for (mw, mh, ow, oh) in [(10u32, 10u32, 20u32, 20u32), (13, 7, 100, 61), (64, 48, 64, 48), (1, 1, 9, 9), (40, 30, 17, 11), (50, 80, 333, 517), (7, 9, 7, 300)] {
            for density in [0u64, 2, 30, 100] {
                let data = (0..mw * mh).map(|_| if next() % 100 < density { 255 } else { 0 }).collect();
                let mask = Mask { width: mw, height: mh, data };
                assert_eq!(spans_of(&mask, ow, oh), spans_by_asking_every_pixel(&mask, ow, oh), "mask {mw}x{mh}, original {ow}x{oh}, density {density}");
            }
        }
        let empty = Mask { width: 0, height: 0, data: Vec::new() };
        assert_eq!(spans_of(&empty, 5, 4), spans_by_asking_every_pixel(&empty, 5, 4));
    }
}
