//! R1 step 3 — flat-edge gradients: where the square had to invent background, fade the stretched
//! background into a flat colour, so shadows and reflections leave no streaks at the border. The colour
//! is sampled at the edge the zone belongs to (its edge band and its outermost line), not taken from the
//! whole photo. Five zones (left, right, bottom, two bottom corners), each only on a side the square
//! reaches past the image. Called only on a flat background (`flatness`). The product box is never touched.
//! See docs/architecture/modules/core-processor-routes-center-and-stretch.md.

use image::{Rgb, RgbImage};

use crate::core::config;
use crate::core::shot_classifier::plan::{Overhang, SquarePlan};
use crate::core::shot_classifier::{geometry::Rect, Edge, Mask};

/// What the gradients need to know about one composed square.
pub struct Square<'a> {
    pub plan: SquarePlan,
    /// The subject's box in the original image's pixels.
    pub subject: Rect,
    /// The original image's size.
    pub original: (u32, u32),
    /// The working-resolution subject mask.
    pub mask: &'a Mask,
    /// The lines the product grazes ([`super::grazes`]): they are left out when an edge is sampled.
    pub blocked: Option<&'a super::grazes::Blocked>,
}

/// Fewer clean background pixels than this and there is no colour to trust: no gradients.
const MIN_BACKGROUND_PX: u64 = 100;

/// Zone order is paint order: later zones are painted over earlier ones.
const RED: usize = 0;
const GREEN: usize = 1;
const YELLOW: usize = 2;
const BLUE: usize = 3;
const CYAN: usize = 4;

/// A fade along a slanted line: 1 at `(sx, sy)`, 0 at `(sx + vx, sy + vy)`.
struct Diagonal {
    sx: f32,
    sy: f32,
    vx: f32,
    vy: f32,
}

/// Where each zone's fade starts and ends, in canvas pixels. `None`: the zone is not painted.
struct Layout {
    /// Bottom: 0 at `.0`, 1 at `.1` (rows).
    red: Option<(f32, f32)>,
    green: Option<Diagonal>,
    yellow: Option<Diagonal>,
    /// Left: 1 at `.0`, 0 at `.1` (columns).
    blue: Option<(f32, f32)>,
    /// Right: 0 at `.0`, 1 at `.1` (columns).
    cyan: Option<(f32, f32)>,
}

impl Layout {
    /// `bbox` is `[left, top, right, bottom]` of the subject in canvas pixels (right and bottom
    /// exclusive). `s` and `g` are `REFINE_NARROW_BAND_PX` and `GATE_MARGIN_PX` in canvas pixels.
    fn new(side: f32, bbox: [f32; 4], over: Overhang, s: f32, g: f32) -> Layout {
        let [bl, bt, br, bb] = bbox;
        let (bw, bh) = (br - bl, bb - bt);
        // A fade ends `s` from the box, or 1 when the box is within `2s` of the border.
        let end_offset = |gap: f32| if gap - s < s { 1.0 } else { s };

        // Bottom: ends `s` below the box; full colour no closer than `g` and `R1_FLAT_EDGE_FADE_SHARE`
        // of the box's height away, or the border when that leaves no room. No room: no bottom zones.
        let start_y = (bb + config::R1_FLAT_EDGE_FADE_SHARE * bh).min(side - g).max(bb + g);
        let red = (over.bottom > 0 && start_y <= side - 1.0).then_some((bb + s, start_y));

        // Corners: from the border, `d` below the box, to `s` below the box beside it.
        let d = 0.5 * config::R1_MARGIN * bw.max(bh);
        let diagonal = |sx: f32, ex: f32, left: bool| {
            let (sy, ey) = (bb + d - s, bb + s);
            let (vx, vy) = (ex - sx, ey - sy);
            let toward_box = if left { vx > 0.0 } else { vx < 0.0 };
            (toward_box && vx * vx + vy * vy > 0.0).then_some(Diagonal { sx, sy, vx, vy })
        };
        let corners = red.is_some();
        let green = (corners && over.left > 0).then(|| diagonal(s, bl - end_offset(bl), true)).flatten();
        let yellow = (corners && over.right > 0)
            .then(|| diagonal(side - 1.0 - s, br + end_offset(side - br), false))
            .flatten();

        let blue_end = bl - end_offset(bl);
        let blue = (over.left > 0 && blue_end > s).then_some((s, blue_end));
        let cyan_end = br + end_offset(side - br);
        let cyan = (over.right > 0 && cyan_end < side - 1.0 - s).then_some((cyan_end, side - 1.0 - s));
        Layout { red, green, yellow, blue, cyan }
    }

    fn is_empty(&self) -> bool {
        self.red.is_none() && self.green.is_none() && self.yellow.is_none() && self.blue.is_none() && self.cyan.is_none()
    }

    /// The weight of every zone at pixel `(x, y)`, in paint order; 0 where a zone does not reach.
    fn alphas(&self, x: f32, y: f32) -> [f32; 5] {
        let mut a = [0.0f32; 5];
        if let Some((end, start)) = self.red {
            a[RED] = ((y - end) / (start - end)).clamp(0.0, 1.0);
        }
        for (i, zone) in [(GREEN, &self.green), (YELLOW, &self.yellow)] {
            if let Some(d) = zone {
                let t = ((x - d.sx) * d.vx + (y - d.sy) * d.vy) / (d.vx * d.vx + d.vy * d.vy);
                a[i] = (1.0 - t).clamp(0.0, 1.0);
            }
        }
        if let Some((start, end)) = self.blue {
            a[BLUE] = ((end - x) / (end - start)).clamp(0.0, 1.0);
        }
        if let Some((end, start)) = self.cyan {
            a[CYAN] = ((x - end) / (start - end)).clamp(0.0, 1.0);
        }
        a
    }
}

/// The colour a zone fades into: `a` at `from`, `b` at `to`, a straight ramp between, along x for the
/// left and right zones and along y for the bottom one. `a` and `b` equal: one flat colour.
#[derive(Clone, Copy)]
struct Target {
    a: [f32; 3],
    b: [f32; 3],
    from: f32,
    to: f32,
    along_x: bool,
}

impl Target {
    fn flat(colour: [f32; 3]) -> Target {
        Target { a: colour, b: colour, from: 0.0, to: 1.0, along_x: false }
    }

    fn at(&self, x: f32, y: f32) -> [f32; 3] {
        let c = if self.along_x { x } else { y };
        let t = ((c - self.from) / (self.to - self.from)).clamp(0.0, 1.0);
        [0, 1, 2].map(|k| self.a[k] + (self.b[k] - self.a[k]) * t)
    }
}

/// Sample one edge of the real photo: `a` is the average colour of its edge band (the `band` lines
/// nearest the border), `b` the average of its single outermost line, both over the lines the product
/// does not graze. The ramp runs from the band's inner edge to the canvas edge. `None` with fewer than 10
/// such lines, or when the photo reaches no deeper than `band` from this edge (its width for Left and
/// Right, its height for Bottom): the band would then read invented pixels, or past the canvas and
/// panic. `edge` is Bottom, Left or Right.
fn edge_target(canvas: &RgbImage, sq: &Square<'_>, edge: Edge, real: [i64; 4], band: i64) -> Option<Target> {
    let side = canvas.width() as i64;
    let sideways = matches!(edge, Edge::Left | Edge::Right);
    let blocked = sq.blocked.map(|b| match edge {
        Edge::Left => &b.left,
        Edge::Right => &b.right,
        _ => &b.bottom,
    });
    // The lines along the edge: rows for the sides, columns for the bottom, on the canvas.
    let (lo, hi) = if sideways { (real[1], real[3]) } else { (real[0], real[2]) };
    let origin = if sideways { sq.plan.y } else { sq.plan.x };
    let safe: Vec<i64> = (lo..hi)
        .filter(|&l| !blocked.is_some_and(|v| v.get((l + origin).max(0) as usize).copied().unwrap_or(false)))
        .collect();
    if safe.len() < 10 {
        return None;
    }
    // The outermost position, and the way into the photo.
    let (border, inward) = match edge {
        Edge::Left => (real[0], 1),
        Edge::Right => (real[2] - 1, -1),
        _ => (real[3] - 1, -1),
    };
    let span = if sideways { real[2] - real[0] } else { real[3] - real[1] };
    if span <= band {
        return None;
    }
    let pixel = |line: i64, depth: i64| {
        let pos = border + inward * depth;
        let (x, y) = if sideways { (pos, line) } else { (line, pos) };
        canvas.get_pixel(x as u32, y as u32).0
    };
    let mean = |depths: std::ops::Range<i64>| -> [f32; 3] {
        let mut sum = [0f64; 3];
        let mut n = 0f64;
        for &l in &safe {
            for d in depths.clone() {
                let p = pixel(l, d);
                for k in 0..3 {
                    sum[k] += p[k] as f64;
                }
                n += 1.0;
            }
        }
        sum.map(|v| (v / n) as f32)
    };
    let (a, b) = (mean(0..band), mean(0..1));
    let inner = (border + inward * (band - 1)) as f32;
    let to = if edge == Edge::Left { 0.0 } else { (side - 1) as f32 };
    Some(Target { a, b, from: inner, to, along_x: sideways })
}

/// Fade the invented edges of `canvas` (the composed square) into the flat colour of the edge they
/// belong to.
/// Returns whether anything was painted: `false` when the square reaches past no border or when too
/// little clean background is left to take a colour from. Only called on a flat background
/// (`flatness::is_flat`).
pub fn blend(canvas: &mut RgbImage, sq: &Square<'_>) -> bool {
    let (w, h) = sq.original;
    let side = canvas.width();
    let over = sq.plan.overhang(w, h);
    let bl = sq.subject.x as i64 - sq.plan.x;
    let bt = sq.subject.y as i64 - sq.plan.y;
    let (br, bb) = (bl + sq.subject.w as i64, bt + sq.subject.h as i64);
    let (s, g) = (config::REFINE_NARROW_BAND_PX as f32, config::GATE_MARGIN_PX as f32);
    let layout = Layout::new(side as f32, [bl as f32, bt as f32, br as f32, bb as f32], over, s, g);
    if layout.is_empty() {
        return false;
    }
    // The real photo, in canvas pixels: the rest of the square is invented.
    let real = [over.left as i64, over.top as i64, (side - over.right) as i64, (side - over.bottom) as i64];
    // The clean-background colour is what a zone falls back on when `edge_target` cannot sample its
    // own edge.
    let Some(colour) = clean_background(canvas, sq, &layout, real, bb) else {
        return false;
    };
    let band = (s as i64).max(1);
    let fallback = Target::flat(colour);
    let bottom = edge_target(canvas, sq, Edge::Bottom, real, band).unwrap_or(fallback);
    let left = edge_target(canvas, sq, Edge::Left, real, band).unwrap_or(fallback);
    let right = edge_target(canvas, sq, Edge::Right, real, band).unwrap_or(fallback);
    // Per zone, in paint order. The corners take the bottom's colour: they are the bottom's corners.
    let targets = [bottom, bottom, bottom, left, right];
    for y in 0..side {
        for x in 0..side {
            let a = layout.alphas(x as f32, y as f32);
            if a.iter().all(|&v| v == 0.0) {
                continue;
            }
            let p = canvas.get_pixel_mut(x, y);
            let mut c = [p[0] as f32, p[1] as f32, p[2] as f32];
            for (zone, &v) in a.iter().enumerate().filter(|(_, &v)| v > 0.0) {
                let target = targets[zone].at(x as f32, y as f32);
                for k in 0..3 {
                    c[k] = c[k] * (1.0 - v) + target[k] * v;
                }
            }
            *p = Rgb([0, 1, 2].map(|k| (c[k] + 0.5).clamp(0.0, 255.0) as u8));
        }
    }
    true
}

/// The one flat colour: per channel, the median of the real photo's pixels that are outside the
/// subject mask, outside every zone, and above the bottom of the subject's box (`bb`). `None` when
/// fewer than `MIN_BACKGROUND_PX` qualify.
fn clean_background(canvas: &RgbImage, sq: &Square<'_>, layout: &Layout, real: [i64; 4], bb: i64) -> Option<[f32; 3]> {
    let (mw, mh) = (sq.mask.width, sq.mask.height);
    let fx = mw as f32 / sq.original.0.max(1) as f32;
    let fy = mh as f32 / sq.original.1.max(1) as f32;
    let mut hist = [[0u64; 256]; 3];
    let mut n = 0u64;
    for y in real[1]..real[3].min(bb) {
        let oy = (y + sq.plan.y) as f32;
        let my = (((oy + 0.5) * fy) as u32).min(mh.saturating_sub(1));
        for x in real[0]..real[2] {
            let mx = (((x + sq.plan.x) as f32 + 0.5) * fx) as u32;
            if sq.mask.get(mx.min(mw.saturating_sub(1)), my) {
                continue;
            }
            if layout.alphas(x as f32, y as f32).iter().any(|&v| v > 0.0) {
                continue;
            }
            let p = canvas.get_pixel(x as u32, y as u32);
            for k in 0..3 {
                hist[k][p[k] as usize] += 1;
            }
            n += 1;
        }
    }
    (n >= MIN_BACKGROUND_PX).then(|| [0, 1, 2].map(|k| median(&hist[k], n)))
}

/// Median of a 256-bin histogram of `n` values; the mean of the two middle values when `n` is even.
fn median(hist: &[u64; 256], n: u64) -> f32 {
    let value_at = |rank: u64| {
        let mut seen = 0;
        for (v, &c) in hist.iter().enumerate() {
            seen += c;
            if seen > rank {
                return v as f32;
            }
        }
        255.0
    };
    if n % 2 == 1 { value_at(n / 2) } else { (value_at(n / 2 - 1) + value_at(n / 2)) / 2.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::processor::routes::center_and_stretch::{compose, Silhouette};
    use crate::core::shot_classifier::plan::Pins;

    const BG: [u8; 3] = [200, 200, 200];
    const FG: [u8; 3] = [10, 10, 10];

    /// A `w x h` picture: `bg(x, y)` background with `subject` painted dark, and its exact mask.
    fn scene(w: u32, h: u32, subject: Rect, bg: impl Fn(u32, u32) -> u8) -> (RgbImage, Mask) {
        let mut img = RgbImage::from_fn(w, h, |x, y| Rgb([bg(x, y); 3]));
        let mut mask = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
        for y in subject.y..subject.y + subject.h {
            for x in subject.x..subject.x + subject.w {
                img.put_pixel(x, y, Rgb(FG));
                mask.data[(y * w + x) as usize] = 255;
            }
        }
        (img, mask)
    }

    /// Compose the square for the scene's subject, then run the gradients on it. Returns the canvas
    /// before and after, whether anything was painted, and the subject's box in canvas pixels.
    fn run(img: &RgbImage, mask: &Mask) -> (RgbImage, RgbImage, bool, Rect) {
        let subject = mask.bounds().expect("a subject");
        let plan = SquarePlan::new(subject, config::R1_MARGIN, Pins::NONE).expect("a legal square");
        let silhouette = Silhouette::from_mask(mask, img.width(), img.height(), 2);
        let before = compose::compose(img, &plan, &silhouette, None, Some(subject), None);
        let mut after = before.clone();
        let sq = Square { plan, subject, original: img.dimensions(), mask, blocked: None };
        let painted = blend(&mut after, &sq);
        let in_canvas = Rect { x: (subject.x as i64 - plan.x) as u32, y: (subject.y as i64 - plan.y) as u32, ..subject };
        (before, after, painted, in_canvas)
    }

    #[test]
    fn a_square_that_reaches_past_no_border_is_left_alone() {
        let (img, mask) = scene(100, 100, Rect { x: 30, y: 30, w: 40, h: 40 }, |_, _| BG[0]);
        let (before, after, painted, _) = run(&img, &mask);
        assert!(!painted);
        assert_eq!(before, after);
    }

    #[test]
    fn the_left_overhang_ends_flat_in_the_clean_background_colour_and_the_product_is_untouched() {
        // Subject 15 px from the left edge: the square reaches 160 px past it. A vertical ramp makes the
        // stretched overhang vary by row, so a flat result can only come from the gradient.
        let (img, mask) = scene(1200, 900, Rect { x: 15, y: 60, w: 300, h: 600 }, |_, y| (100 + y / 8) as u8);
        let (before, after, painted, bx) = run(&img, &mask);
        assert!(painted);
        let edge = after.get_pixel(0, 0);
        assert!((0..after.height()).all(|y| after.get_pixel(0, y) == edge), "the border column is one colour");
        assert!((100..=190).contains(&edge[0]), "taken from the photo's background, got {}", edge[0]);
        // The subject's box, to the pixel.
        for y in bx.y..bx.y + bx.h {
            for x in bx.x..bx.x + bx.w {
                assert_eq!(before.get_pixel(x, y), after.get_pixel(x, y), "box altered at ({x}, {y})");
            }
        }
    }

    #[test]
    fn a_zone_fades_into_the_colour_at_its_own_edge_not_the_whole_photos_median() {
        // A floor that gets lighter towards the bottom: 200 above row 860, 242 in the last rows. The whole
        // photo's median is 200; the bottom edge itself is 242, and that is what the bottom must end in.
        let (img, mask) = scene(1200, 900, Rect { x: 400, y: 30, w: 300, h: 840 }, |_, y| if y >= 860 { 242 } else { 200 });
        let (_, after, painted, _) = run(&img, &mask);
        assert!(painted);
        let last = after.height() - 1;
        let mid = (after.width() / 2, last);
        assert_eq!(after.get_pixel(mid.0, mid.1)[0], 242, "the bottom border row takes the bottom edge's colour");
        assert_eq!(after.get_pixel(mid.0, 0)[0], 200, "rows well above are not touched");
    }

    #[test]
    fn the_colour_comes_from_the_background_not_the_product() {
        // Uniform background: whatever the zones paint must be that background, never the dark product.
        let (img, mask) = scene(1200, 900, Rect { x: 15, y: 60, w: 300, h: 600 }, |_, _| BG[0]);
        let (_, after, painted, _) = run(&img, &mask);
        assert!(painted);
        assert!((0..after.height()).all(|y| *after.get_pixel(0, y) == Rgb(BG)));
    }

    #[test]
    fn the_bottom_fade_ends_5_px_under_the_box_and_starts_at_least_20_px_away() {
        // Core image 22's numbers: the box ends at 986 in a 1027 square, room for 41 px.
        let over = Overhang { top: 0, bottom: 27, left: 171, right: 189 };
        let l = Layout::new(1027.0, [277.0, 0.0, 751.0, 986.0], over, 5.0, 20.0);
        assert_eq!(l.red, Some((991.0, 1007.0)));
        // A taller room: 25% of the box's height (here 400 * 0.25 = 100).
        let l = Layout::new(1054.0, [100.0, 183.0, 900.0, 583.0], over, 5.0, 20.0);
        assert_eq!(l.red, Some((588.0, 683.0)));
        // No room for 20 px under the box: no bottom zones at all, corners included.
        let l = Layout::new(1027.0, [277.0, 0.0, 751.0, 1010.0], over, 5.0, 20.0);
        assert!(l.red.is_none() && l.green.is_none() && l.yellow.is_none());
        assert!(l.blue.is_some() && l.cyan.is_some());
    }

    #[test]
    fn a_zone_is_painted_only_on_a_side_the_square_reaches_past() {
        let none = Overhang::default();
        let l = Layout::new(1000.0, [200.0, 100.0, 800.0, 600.0], none, 5.0, 20.0);
        assert!(l.is_empty());
        let l = Layout::new(1000.0, [200.0, 100.0, 800.0, 600.0], Overhang { left: 40, ..none }, 5.0, 20.0);
        assert!(l.blue.is_some() && l.cyan.is_none() && l.red.is_none() && l.green.is_none());
        let l = Layout::new(1000.0, [200.0, 100.0, 800.0, 600.0], Overhang { bottom: 40, ..none }, 5.0, 20.0);
        assert!(l.red.is_some() && l.green.is_none() && l.yellow.is_none() && l.blue.is_none());
    }

    #[test]
    fn the_median_of_a_histogram_matches_the_sorted_middle() {
        let mut h = [0u64; 256];
        for v in [10u8, 20, 30, 40] {
            h[v as usize] += 1;
        }
        assert_eq!(median(&h, 4), 25.0);
        h[50] += 1;
        assert_eq!(median(&h, 5), 30.0);
    }
}
