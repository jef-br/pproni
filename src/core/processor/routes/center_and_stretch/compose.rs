//! R1 step 2 — build the square.
//!
//! Two passes, horizontal then vertical. The first widens every real row to the square's width;
//! the second stretches the result to the square's height. Splitting it this way is what fills the
//! *corners*: a corner sits outside the image on both axes at once, so no single pass can source
//! it from real pixels — but after the horizontal pass, the top rows are already the full square
//! width, and the vertical pass reads them like any other row. Both passes are the same function:
//! the second runs on the transposed picture, so a column is just a row there.
//!
//! ## One line, one stretch
//!
//! Every line (a row in pass 1, a column in pass 2) is handled on its own, in three parts:
//!
//! * the **product band** — the silhouette grown outward by the clearance — is copied untouched;
//! * the **source** on each side is the background between the band and the image border, read from
//!   the band outward;
//! * the **overhang** is what the square reaches past the border. The source is stretched to cover
//!   it, the least next to the product and the most at the border ([`fill_oriented`]), and when
//!   even the strongest allowed stretch cannot cover it all, the rest is invented from existing
//!   pixels and kept inside the colour range of the source, with the background colour as its
//!   median ([`controlled`]).
//!
//! How hard a line may be stretched is averaged with its neighbouring lines
//! ([`line_amplitudes`]), so adjacent lines move together instead of jumping.

use image::{Rgb, RgbImage};

use super::super::super::super::config;
use super::inpaint;
use super::plan::SquarePlan;

/// Compose the square from `src`, stretching background outward wherever the square reaches past a
/// border. `silhouette` holds, per row and per column of `src`, the span the grown silhouette
/// covers. `bgc` is the one dominant background colour the exporter sampled, when present; it is the
/// reference median for invented pixels. `radius` is the neighbourhood, in pixels, that invented
/// pixels read their local colours from.
pub fn compose(
    src: &RgbImage,
    plan: &SquarePlan,
    silhouette: &super::Silhouette,
    bgc: Option<Rgb<u8>>,
    radius: u32,
) -> RgbImage {
    let (w, h) = (src.width(), src.height());
    let Some(real) = plan.real_region(w, h) else {
        return RgbImage::new(plan.side, plan.side);
    };

    // Pass 1: every real row becomes a full-width square row.
    let rows = image::imageops::crop_imm(src, 0, real.y, w, real.h).to_image();
    let row_bands: Vec<(i64, i64)> = (0..real.h).map(|ry| silhouette.row_x[(real.y + ry) as usize]).collect();
    let (mut mid, fills) = widen_rows(&rows, plan.x, plan.side, &row_bands, bgc, radius);

    // Plan A.5 neighbour-row borrow: replace each fully-bled segment's flat BGC/white fill with the
    // nearest clean neighbour row's background at the same columns, so a run of bled rows reads as
    // the surrounding background rather than a flat bar.
    borrow_neighbour_rows(&mut mid, &fills);

    // Pass 2: those rows become the square's full height. A column of `mid` is a row of its
    // transpose. `mid` column `x` sits over original column `plan.x + x`; past the image's own
    // columns it borrows the nearest real column's span, which is where pass 1 took its background.
    // `mid` row 0 is original row `real.y`, so the square's origin sits at `plan.y - real.y` here.
    let columns = transpose(&mid);
    let col_bands: Vec<(i64, i64)> = (0..plan.side)
        .map(|x| {
            let c = (plan.x + x as i64).clamp(0, w as i64 - 1) as usize;
            let (lo, hi) = silhouette.col_y[c];
            (lo - real.y as i64, hi - real.y as i64)
        })
        .collect();
    let (widened, _) = widen_rows(&columns, plan.y - real.y as i64, plan.side, &col_bands, bgc, radius);
    transpose(&widened)
}

/// What [`widen_rows`] did on one line, for the plan A.5 neighbour-row borrow. `a`/`b` are the
/// output-column boundaries of the product band; a bled lo segment occupies `0..a`, a bled hi
/// segment `b..side`.
#[derive(Clone, Copy)]
struct LineFill {
    lo_bled: bool,
    hi_bled: bool,
    a: usize,
    b: usize,
}

/// Which side of the product band a segment sits on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Lo,
    Hi,
}

/// Widen every row of `input` to `side` columns. `start` is the source column the output's first
/// column maps to — negative when the square begins outside the image. `bands[y]` is the product
/// band of row `y`, `[lo, hi)` in source columns.
fn widen_rows(
    input: &RgbImage,
    start: i64,
    side: u32,
    bands: &[(i64, i64)],
    bgc: Option<Rgb<u8>>,
    radius: u32,
) -> (RgbImage, Vec<LineFill>) {
    let len = input.width() as i64;
    let rows = input.height() as usize;
    let side_i = side as i64;

    // The source range the square actually covers, and the overhang past each end of it.
    let lo_src = start.max(0);
    let hi_src = (start + side_i).min(len).max(lo_src);
    let over_lo = lo_src - start;
    let over_hi = start + side_i - hi_src;

    // Per row: the product band clamped into the covered range.
    let split: Vec<(i64, i64)> = bands
        .iter()
        .map(|&(lo, hi)| {
            let a = lo.max(lo_src).min(hi_src);
            (a, hi.max(a).min(hi_src))
        })
        .collect();

    // Per row: how much background each side has to stretch, and how hard it may be stretched.
    let n_lo: Vec<i64> = split.iter().map(|&(a, _)| a - lo_src).collect();
    let n_hi: Vec<i64> = split.iter().map(|&(_, b)| hi_src - b).collect();
    let window = radius as usize;
    let k_lo = line_amplitudes(&n_lo, over_lo, window);
    let k_hi = line_amplitudes(&n_hi, over_hi, window);

    // The reference colours each line's invented pixels are held to: the background of the lines
    // around it, per side. Only built where there is an overhang to invent into.
    let reach = window * REFERENCE_WINDOW_FACTOR;
    let lo_spans: Vec<(i64, i64)> = split.iter().map(|&(a, _)| (lo_src, a)).collect();
    let hi_spans: Vec<(i64, i64)> = split.iter().map(|&(_, b)| (b, hi_src)).collect();
    let refs_lo = if over_lo > 0 { references(input, &lo_spans, reach, bgc) } else { vec![None; rows] };
    let refs_hi = if over_hi > 0 { references(input, &hi_spans, reach, bgc) } else { vec![None; rows] };
    let neighbourhood = Neighbourhood { input, split: &split, lo_src, hi_src, radius: radius as i64 };

    let mut out = RgbImage::new(side, rows as u32);
    let mut fills = Vec::with_capacity(rows);
    let mut line: Vec<Rgb<u8>> = vec![Rgb([0, 0, 0]); side as usize];
    for y in 0..rows {
        let (a_src, b_src) = split[y];
        let a = (a_src - start) as usize;
        let b = (b_src - start) as usize;

        // Product band: untouched copy.
        for x in a_src..b_src {
            line[(x - start) as usize] = *input.get_pixel(x as u32, y as u32);
        }

        let mut lo_bled = false;
        let mut hi_bled = false;

        // Lo side. Oriented from the product outward: source column `a_src - 1` first.
        if a > 0 {
            if n_lo[y] == 0 {
                // A fully-bled row: the product runs to the border and there is no background left
                // on this side to stretch. Flat BGC, else white — the hard "fill is never black" bound.
                inpaint::fill_segment(&mut line[..a], None, None, bgc);
                lo_bled = true;
            } else {
                let seg: Vec<Rgb<u8>> = (lo_src..a_src).rev().map(|x| *input.get_pixel(x as u32, y as u32)).collect();
                let filled = fill_oriented(&seg, a, k_lo[y], &Invent {
                    nb: &neighbourhood,
                    reference: refs_lo[y].as_ref(),
                    side: Side::Lo,
                    row: y,
                    mask_x: a_src,
                });
                for (d, px) in filled.into_iter().enumerate() {
                    line[a - 1 - d] = px;
                }
            }
        }

        // Hi side. Oriented from the product outward: source column `b_src` first.
        if b < side as usize {
            if n_hi[y] == 0 {
                inpaint::fill_segment(&mut line[b..], None, None, bgc);
                hi_bled = true;
            } else {
                let seg: Vec<Rgb<u8>> = (b_src..hi_src).map(|x| *input.get_pixel(x as u32, y as u32)).collect();
                let filled = fill_oriented(&seg, side as usize - b, k_hi[y], &Invent {
                    nb: &neighbourhood,
                    reference: refs_hi[y].as_ref(),
                    side: Side::Hi,
                    row: y,
                    mask_x: b_src,
                });
                for (d, px) in filled.into_iter().enumerate() {
                    line[b + d] = px;
                }
            }
        }

        for (x, px) in line.iter().enumerate() {
            out.put_pixel(x as u32, y as u32, *px);
        }
        fills.push(LineFill { lo_bled, hi_bled, a, b });
    }
    (out, fills)
}

/// How hard each line's source may be stretched, `0.0..=1.0` (1.0 is the full curve of
/// [`config::R1_STRETCH_CURVE_POWER`]).
///
/// A line needs `overhang * (power + 1) / n` to cover its overhang with stretch alone, `n` being its
/// source length — a long source needs little, a short one more than it may give. Each line is
/// capped at 1.0, then averaged with the lines within `window` of it (those that have a source at
/// all), so neighbouring lines stretch alike. No line ends up above what it needs, because a
/// stretch past the overhang would push the product band.
fn line_amplitudes(n: &[i64], overhang: i64, window: usize) -> Vec<f32> {
    let p1 = config::R1_STRETCH_CURVE_POWER + 1.0;
    let need: Vec<f32> = n
        .iter()
        .map(|&n| if n > 0 && overhang > 0 { overhang as f32 * p1 / n as f32 } else { 0.0 })
        .collect();
    // Prefix sums of the capped need over lines that have a source.
    let mut sum = vec![0.0f64; n.len() + 1];
    let mut cnt = vec![0u32; n.len() + 1];
    for (i, &nd) in need.iter().enumerate() {
        let has = n[i] > 0;
        sum[i + 1] = sum[i] + if has { nd.min(1.0) as f64 } else { 0.0 };
        cnt[i + 1] = cnt[i] + has as u32;
    }
    (0..n.len())
        .map(|i| {
            if n[i] <= 0 || overhang <= 0 {
                return 0.0;
            }
            let lo = i.saturating_sub(window);
            let hi = (i + window + 1).min(n.len());
            let c = (cnt[hi] - cnt[lo]).max(1);
            let avg = ((sum[hi] - sum[lo]) / c as f64) as f32;
            avg.min(need[i]).min(1.0)
        })
        .collect()
}

/// What the invented pixels of one segment need to look around them.
struct Invent<'a> {
    nb: &'a Neighbourhood<'a>,
    reference: Option<&'a Reference>,
    side: Side,
    row: usize,
    /// Source column of the product band's edge on this side (`a_src` for lo, `b_src` for hi).
    mask_x: i64,
}

/// Fill one segment of `total` pixels, ordered from the product outward, from `seg` (the source,
/// in the same order).
///
/// The source is stretched first. Writing `u` for the position along the source (0 at the product,
/// 1 at the border), the stretch at `u` is `k * u^power` — nothing next to the product, `k` times
/// the full curve at the border — so the output position is
/// `n * (u + k * u^(power+1) / (power+1))`. Each output pixel is read from the source position that
/// lands on it, so the pixel next to the product is the source pixel next to the product. `k` is
/// the line's amplitude from [`line_amplitudes`].
///
/// If that covers fewer than `total` pixels, the rest is invented beyond the border: each new
/// pixel proposes the value mirrored from the stretched part and [`controlled`] keeps it inside
/// the colours the source really has.
fn fill_oriented(seg: &[Rgb<u8>], total: usize, k: f32, invent: &Invent<'_>) -> Vec<Rgb<u8>> {
    let n = seg.len();
    if total <= n {
        return seg[..total].to_vec();
    }
    let p1 = config::R1_STRETCH_CURVE_POWER + 1.0;
    let nf = n as f32;
    let stretched_len = ((nf + k * nf / p1).round() as usize).clamp(n, total);

    // Output position of each source position, tabulated, then walked in step with the output.
    let grid = 2 * stretched_len + 2;
    let pos: Vec<f32> = (0..=grid)
        .map(|j| {
            let u = j as f32 / grid as f32;
            nf * (u + k * u.powf(p1) / p1)
        })
        .collect();
    let mut stretched: Vec<Rgb<u8>> = Vec::with_capacity(total);
    let mut source_at: Vec<f32> = Vec::with_capacity(stretched_len);
    let mut j = 0usize;
    for d in 0..stretched_len {
        let target = d as f32 + 0.5;
        while j + 1 < grid && pos[j + 1] < target {
            j += 1;
        }
        let span = pos[j + 1] - pos[j];
        let f = if span > 0.0 { ((target - pos[j]) / span).clamp(0.0, 1.0) } else { 0.0 };
        let u = (j as f32 + f) / grid as f32;
        let s = (u * nf - 0.5).clamp(0.0, nf - 1.0);
        source_at.push(s);
        stretched.push(sample_at(seg, s));
    }

    // Invent the rest. Each new pixel is held to the reference after looking at the background
    // around the source spot it mirrors. Its proposal is the mirrored stretched pixel, which carries
    // the scene's own texture on. A source shorter than the neighbourhood would repeat in visible
    // tiles when mirrored, so there the proposal is the colour of one of the neighbours instead.
    let mirror = stretched_len as i64 >= invent.nb.radius.max(1);
    for i in 0..(total - stretched_len) {
        let cycle = 2 * stretched_len;
        let r = i % cycle;
        let idx = if r < stretched_len { stretched_len - 1 - r } else { r - stretched_len };
        let off = source_at[idx].round() as i64;
        let x = match invent.side {
            Side::Lo => invent.mask_x - 1 - off,
            Side::Hi => invent.mask_x + off,
        };
        let around = invent.nb.samples(invent.side, x, invent.row);
        let mut proposal = if mirror || around.is_empty() {
            stretched[idx]
        } else {
            let pick = splitmix64(((invent.row as u64) << 32) ^ i as u64) as usize % around.len();
            Rgb(around[pick])
        };
        // Mirrored detail reads as a reflected copy of the scene, more so the further it goes. So the
        // detail fades into the average colour of its neighbours with distance from the seam: the
        // first invented pixel is the mirror, the last is that average.
        if !around.is_empty() {
            let fade = (i + 1) as f32 / (total - stretched_len) as f32;
            for c in 0..3 {
                let mean = around.iter().map(|s| s[c] as f32).sum::<f32>() / around.len() as f32;
                proposal[c] = (proposal[c] as f32 * (1.0 - fade) + mean * fade).round() as u8;
            }
        }
        let px = match invent.reference {
            Some(reference) => controlled(proposal, &around, reference),
            None => proposal,
        };
        stretched.push(px);
    }
    stretched
}

/// A small deterministic hash, so the same picture always gives the same invented pixels.
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Cubic (Catmull-Rom) read of `seg` at fractional index `s`. Where the stretch magnifies the most a
/// linear read goes soft and smeared; the cubic keeps the edges of the background's texture. It
/// passes exactly through the source pixels, and the result is clamped to a valid colour.
fn sample_at(seg: &[Rgb<u8>], s: f32) -> Rgb<u8> {
    let n = seg.len();
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

/// The colours the background of one side really has, as a boxplot per channel: lowest value,
/// first quartile, median, third quartile, highest value.
///
/// When the one background colour (BGC) lies inside the lowest–highest range on every channel it is
/// "close enough" to be believed, and it replaces the median — the colour the rest is judged
/// around. Otherwise the data's own median stands.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Reference {
    five: [[f32; 5]; 3],
    /// The BGC lay inside the range on every channel and is the median above. Without it there is
    /// no background colour to control against, and invented pixels are only held to the range.
    bgc_close: bool,
}

/// How many times the neighbour radius, in lines either way, the reference looks. Wide enough to
/// know the background's own spread, narrow enough that a gradient across the picture does not
/// widen it far beyond what the invented pixel's neighbours show — a reference much wider than the
/// neighbourhood would stretch their noise out to its own range.
const REFERENCE_WINDOW_FACTOR: usize = 3;

/// Per-channel value counts of the background pixels in a window of lines.
struct Counts {
    hist: [[u32; 256]; 3],
    total: u64,
}

impl Counts {
    fn new() -> Counts {
        Counts { hist: [[0; 256]; 3], total: 0 }
    }

    /// Add (`add`) or remove the pixels `x0..x1` of line `y`.
    fn line(&mut self, input: &RgbImage, y: usize, (x0, x1): (i64, i64), add: bool) {
        for x in x0..x1 {
            let p = input.get_pixel(x as u32, y as u32).0;
            for c in 0..3 {
                if add {
                    self.hist[c][p[c] as usize] += 1;
                } else {
                    self.hist[c][p[c] as usize] -= 1;
                }
            }
        }
        let n = (x1 - x0).max(0) as u64;
        if add {
            self.total += n;
        } else {
            self.total -= n;
        }
    }

    fn reference(&self, bgc: Option<Rgb<u8>>) -> Option<Reference> {
        Reference::from_counts(&self.hist, self.total, bgc)
    }
}

/// One reference per line: the background of the lines within `reach` of it (`spans[y]` is line
/// `y`'s background, `[x0, x1)`), by a window that slides down the lines.
fn references(input: &RgbImage, spans: &[(i64, i64)], reach: usize, bgc: Option<Rgb<u8>>) -> Vec<Option<Reference>> {
    let rows = spans.len();
    let mut counts = Counts::new();
    let mut out = Vec::with_capacity(rows);
    let mut added = 0usize; // lines 0..added are in the window
    let mut removed = 0usize; // lines 0..removed have left it
    for y in 0..rows {
        let hi = (y + reach + 1).min(rows);
        while added < hi {
            counts.line(input, added, spans[added], true);
            added += 1;
        }
        let lo = y.saturating_sub(reach);
        while removed < lo {
            counts.line(input, removed, spans[removed], false);
            removed += 1;
        }
        out.push(counts.reference(bgc));
    }
    out
}

impl Reference {
    /// From the safe pixels `(row, x0, x1)` of one side, `x1` exclusive. `None` when there are none.
    #[cfg(test)]
    fn build(input: &RgbImage, spans: impl Iterator<Item = (usize, i64, i64)>, bgc: Option<Rgb<u8>>) -> Option<Reference> {
        let mut counts = Counts::new();
        for (y, x0, x1) in spans {
            counts.line(input, y, (x0, x1), true);
        }
        counts.reference(bgc)
    }

    fn from_counts(hist: &[[u32; 256]; 3], total: u64, bgc: Option<Rgb<u8>>) -> Option<Reference> {
        if total == 0 {
            return None;
        }
        let mut five = [[0.0f32; 5]; 3];
        for c in 0..3 {
            let lowest = hist[c].iter().position(|&n| n > 0).unwrap_or(0);
            let highest = 255 - hist[c].iter().rev().position(|&n| n > 0).unwrap_or(0);
            let quantile = |frac: f64| -> f32 {
                let target = ((total as f64) * frac).ceil().max(1.0) as u64;
                let mut cum = 0u64;
                for v in 0..256 {
                    cum += hist[c][v] as u64;
                    if cum >= target {
                        return v as f32;
                    }
                }
                255.0
            };
            five[c] = [lowest as f32, quantile(0.25), quantile(0.5), quantile(0.75), highest as f32];
        }
        let mut bgc_close = false;
        if let Some(b) = bgc {
            if (0..3).all(|c| five[c][0] <= b[c] as f32 && b[c] as f32 <= five[c][4]) {
                bgc_close = true;
                for c in 0..3 {
                    five[c][2] = b[c] as f32;
                    five[c][1] = five[c][1].min(five[c][2]);
                    five[c][3] = five[c][3].max(five[c][2]);
                }
            }
        }
        Some(Reference { five, bgc_close })
    }

    /// The reference value of channel `c` at quantile `q` (0 = lowest, 0.5 = median, 1 = highest),
    /// read along the boxplot.
    fn at(&self, c: usize, q: f32) -> f32 {
        let pos = q.clamp(0.0, 1.0) * 4.0;
        let i = (pos.floor() as usize).min(3);
        let t = pos - i as f32;
        self.five[c][i] * (1.0 - t) + self.five[c][i + 1] * t
    }
}

/// The safe background around one spot, read from the source.
struct Neighbourhood<'a> {
    input: &'a RgbImage,
    /// Per row, the product band clamped into the covered range, `[a, b)`.
    split: &'a [(i64, i64)],
    lo_src: i64,
    hi_src: i64,
    radius: i64,
}

impl Neighbourhood<'_> {
    /// The colours of the background pixels within `radius` of source column `x` on `row` — the
    /// pixels of this side, outside the product band, on this row and the rows around it. Read on a
    /// coarse grid (a third of the radius apart) to keep the cost down.
    fn samples(&self, side: Side, x: i64, row: usize) -> Vec<[u8; 3]> {
        let r = self.radius;
        let step = (r / 3).max(1);
        let reach_steps = r / step;
        let mut out = Vec::new();
        for j in -reach_steps..=reach_steps {
            let dy = j * step;
            let yy = row as i64 + dy;
            if yy < 0 || yy as usize >= self.split.len() {
                continue;
            }
            let (a, b) = self.split[yy as usize];
            let (x0, x1) = match side {
                Side::Lo => (self.lo_src, a),
                Side::Hi => (b, self.hi_src),
            };
            let reach = (((r * r - dy * dy).max(0)) as f64).sqrt() as i64;
            for i in -(reach / step)..=(reach / step) {
                let xx = x + i * step;
                if xx >= x0 && xx < x1 {
                    out.push(self.input.get_pixel(xx as u32, yy as u32).0);
                }
            }
        }
        out
    }
}

/// Hold an invented pixel to the background it is made from.
///
/// Per channel, the proposal is first brought inside the reference's lowest–highest range. With no
/// believable BGC that is all. With one, the boxplot of the pixels around it (`around`) says where
/// the proposal sits among its neighbours — its quantile, taken on brightness so all three channels
/// share it and the hue holds — and the reference's value at that same quantile is averaged with
/// it. A proposal on its neighbours' first quartile ends up halfway to the reference's first
/// quartile.
fn controlled(proposal: Rgb<u8>, around: &[[u8; 3]], reference: &Reference) -> Rgb<u8> {
    let mut v = [0.0f32; 3];
    for c in 0..3 {
        v[c] = (proposal[c] as f32).clamp(reference.five[c][0], reference.five[c][4]);
    }
    if !reference.bgc_close {
        return Rgb([v[0].round() as u8, v[1].round() as u8, v[2].round() as u8]);
    }
    let luma = |p: [f32; 3]| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
    let q = if around.is_empty() {
        0.5
    } else {
        let mine = luma(v);
        let below = around.iter().filter(|s| luma([s[0] as f32, s[1] as f32, s[2] as f32]) < mine).count() as f32;
        let equal = around.iter().filter(|s| luma([s[0] as f32, s[1] as f32, s[2] as f32]) == mine).count() as f32;
        (below + 0.5 * equal) / around.len() as f32
    };
    let mut out = [0u8; 3];
    for c in 0..3 {
        out[c] = ((v[c] + reference.at(c, q)) / 2.0).round().clamp(0.0, 255.0) as u8;
    }
    Rgb(out)
}

/// Swap rows and columns, so the vertical pass can reuse the horizontal one.
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

/// Plan A.5 neighbour-row borrow. For each output column that any fully-bled row touched, replace
/// those bled pixels with the nearest clean neighbour row's value at the same column. A column that
/// is bled on every row (the subject runs the full height against that border) has no clean
/// neighbour and keeps its BGC/white fill. Only bled pixels are ever written, so every stretched or
/// copied pixel is byte-for-byte unchanged.
fn borrow_neighbour_rows(mid: &mut RgbImage, fills: &[LineFill]) {
    let h = mid.height() as usize;
    let w = mid.width();
    if h == 0 {
        return;
    }
    let is_bled = |ry: usize, x: u32| {
        let f = fills[ry];
        (f.lo_bled && (x as usize) < f.a) || (f.hi_bled && (x as usize) >= f.b)
    };
    for x in 0..w {
        if !(0..h).any(|ry| is_bled(ry, x)) {
            continue;
        }
        // Nearest clean (non-bled) row at or above / at or below each row, by two linear sweeps.
        let mut above = vec![None; h];
        let mut last = None;
        for ry in 0..h {
            if !is_bled(ry, x) {
                last = Some(ry);
            }
            above[ry] = last;
        }
        let mut below = vec![None; h];
        let mut next = None;
        for ry in (0..h).rev() {
            if !is_bled(ry, x) {
                next = Some(ry);
            }
            below[ry] = next;
        }
        for ry in 0..h {
            if !is_bled(ry, x) {
                continue;
            }
            let src_row = match (above[ry], below[ry]) {
                (Some(a), Some(b)) => if ry - a <= b - ry { a } else { b },
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => continue, // whole column bled — keep the BGC/white fill
            };
            let px = *mid.get_pixel(x, src_row as u32);
            mid.put_pixel(x, ry as u32, px);
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

    fn no_invent<'a>(nb: &'a Neighbourhood<'a>) -> Invent<'a> {
        Invent { nb, reference: None, side: Side::Lo, row: 0, mask_x: 0 }
    }

    fn empty_nb(input: &RgbImage) -> Neighbourhood<'_> {
        Neighbourhood { input, split: &[], lo_src: 0, hi_src: 0, radius: 0 }
    }

    #[test]
    fn no_overhang_is_a_pure_copy() {
        // The square lands exactly on the image: nothing to stretch, every pixel is its own.
        let img = ramp(30, 2);
        let (out, fills) = widen_rows(&img, 0, 30, &[(10, 20), (10, 20)], None, 4);
        assert_eq!(out, img);
        assert!(fills.iter().all(|f| !f.lo_bled && !f.hi_bled));
    }

    #[test]
    fn a_crop_into_the_image_is_a_pure_copy() {
        // The square starts inside the image on the left, so that side crops, never stretches.
        let img = ramp(30, 1);
        let (out, _) = widen_rows(&img, 5, 20, &[(12, 18)], None, 4);
        for x in 0..20u32 {
            assert_eq!(out.get_pixel(x, 0)[0], (x + 5) as u8);
        }
    }

    #[test]
    fn the_stretch_is_gentle_at_the_product_and_strongest_at_the_border() {
        // Source value = distance from the product (0..60). Stretching 60 pixels by the full curve
        // adds ~11, so the output is 71 long; how fast the source advances per output pixel must
        // grow outward: ~1 next to the product, ~0.5 at the border.
        let seg: Vec<Rgb<u8>> = (0..60).map(|i| Rgb([i as u8, 0, 0])).collect();
        let img = RgbImage::new(1, 1);
        let nb = empty_nb(&img);
        let out = fill_oriented(&seg, 71, 1.0, &no_invent(&nb));
        assert_eq!(out.len(), 71);
        assert_eq!(out[0][0], 0, "the pixel next to the product is the source pixel next to it");
        let near = out[10][0] as i32 - out[0][0] as i32;
        let far = out[69][0] as i32 - out[59][0] as i32;
        assert!(near >= 9, "next to the product the source barely slows: advanced {near} in 10");
        assert!(far <= 6, "at the border the source advances slowly: advanced {far} in 10");
        assert!(out.windows(2).all(|p| p[0][0] <= p[1][0]), "the stretch never reverses");
        assert_eq!(out[70][0], 59, "the border pixel ends the stretched part");
    }

    #[test]
    fn a_stretch_that_exactly_covers_the_overhang_invents_nothing() {
        // n = 100, k chosen so growth = 10: 100 -> 110 exactly.
        let p1 = config::R1_STRETCH_CURVE_POWER + 1.0;
        let k = 10.0 * p1 / 100.0;
        let seg: Vec<Rgb<u8>> = (0..100).map(|i| Rgb([i as u8, 0, 0])).collect();
        let img = RgbImage::new(1, 1);
        let nb = empty_nb(&img);
        let out = fill_oriented(&seg, 110, k, &no_invent(&nb));
        assert_eq!(out.len(), 110);
        assert_eq!(out[109][0], 99, "the last pixel is the border pixel, so nothing was invented");
    }

    #[test]
    fn amplitudes_are_capped_at_the_full_curve_and_zero_without_overhang() {
        // A long source needs a gentle stretch, a short one more than the curve gives.
        let k = line_amplitudes(&[1000, 10], 50, 0);
        assert!(k[0] > 0.0 && k[0] < 0.3, "long source stretches gently: {}", k[0]);
        assert_eq!(k[1], 1.0, "a short source is capped at the full curve");
        assert_eq!(line_amplitudes(&[100, 100], 0, 2), vec![0.0, 0.0]);
        assert_eq!(line_amplitudes(&[0], 50, 2), vec![0.0], "no source, no stretch");
    }

    #[test]
    fn neighbouring_lines_are_averaged_but_never_exceed_their_own_need() {
        // Middle line has a short source (wants the full curve), its neighbours long ones.
        let k = line_amplitudes(&[1000, 10, 1000], 50, 1);
        let alone = line_amplitudes(&[1000, 10, 1000], 50, 0);
        assert!(k[1] < alone[1], "the short-source line is pulled toward its neighbours");
        let need = |n: f32| 50.0 * (config::R1_STRETCH_CURVE_POWER + 1.0) / n;
        assert!(k[0] <= need(1000.0) + 1e-6, "a line never stretches past what it needs");
        assert_eq!(k[0], alone[0], "so a long-source neighbour is not pulled up");
    }

    #[test]
    fn a_line_with_no_background_fills_bgc_never_black() {
        // det3/det4: the product runs to the border, so the left side has no source at all. The old
        // code left that overhang black.
        let img = RgbImage::from_pixel(10, 1, Rgb([200, 20, 20]));
        let white = Rgb([255, 255, 255]);
        let (out, fills) = widen_rows(&img, -5, 20, &[(0, 10)], Some(white), 4);
        assert!(fills[0].lo_bled && fills[0].hi_bled);
        for &x in &[0u32, 1, 2, 3, 4, 15, 16, 17, 18, 19] {
            let p = *out.get_pixel(x, 0);
            assert!(p.0 != [0, 0, 0], "overhang left black at {x}");
            assert!(p.0.iter().all(|&c| c >= 240), "overhang not BGC-white at {x}: {p:?}");
        }
        for x in 5..15u32 {
            assert_eq!(*out.get_pixel(x, 0), Rgb([200, 20, 20]), "product band altered at {x}");
        }
    }

    #[test]
    fn a_line_with_no_background_and_no_bgc_fills_white() {
        let img = RgbImage::from_pixel(10, 1, Rgb([10, 90, 10]));
        let (out, _) = widen_rows(&img, -5, 20, &[(0, 10)], None, 4);
        for &x in &[0u32, 2, 4, 15, 17, 19] {
            assert!(out.get_pixel(x, 0).0.iter().all(|&c| c >= 240), "not white at {x}");
        }
    }

    #[test]
    fn invented_pixels_stay_inside_the_colour_range_of_the_source() {
        // A short source (values 100..=120) under a big overhang: most of the fill is invented.
        let mut img = RgbImage::new(60, 12);
        for y in 0..12 {
            for x in 0..60 {
                let v = if x < 6 { 100 + (x as u8 * 4) } else { 10 };
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        let bands = vec![(6i64, 60i64); 12];
        let (out, _) = widen_rows(&img, -30, 90, &bands, None, 6);
        for y in 0..12 {
            for x in 0..36u32 {
                let p = out.get_pixel(x, y)[0];
                assert!((100..=120).contains(&p), "invented pixel outside the source range: {p} at ({x},{y})");
            }
        }
    }

    #[test]
    fn the_reference_takes_bgc_as_its_median_only_when_it_is_inside_the_range() {
        let img = RgbImage::from_fn(10, 1, |x, _| Rgb([100 + x as u8, 100 + x as u8, 100 + x as u8]));
        let spans = || std::iter::once((0usize, 0i64, 10i64));
        let inside = Reference::build(&img, spans(), Some(Rgb([102, 102, 102]))).unwrap();
        assert_eq!(inside.five[0], [100.0, 102.0, 102.0, 107.0, 109.0], "BGC replaces the median");
        let outside = Reference::build(&img, spans(), Some(Rgb([250, 250, 250]))).unwrap();
        assert_eq!(outside.five[0][2], 104.0, "a BGC outside the range is not believed");
        assert!(Reference::build(&img, std::iter::empty(), None).is_none());
    }

    #[test]
    fn each_line_is_held_to_the_background_of_the_lines_around_it() {
        // Top half 10, bottom half 200: a line's reference sees only the lines within reach.
        let img = RgbImage::from_fn(4, 20, |_, y| if y < 10 { Rgb([10, 10, 10]) } else { Rgb([200, 200, 200]) });
        let spans = vec![(0i64, 4i64); 20];
        let refs = references(&img, &spans, 2, None);
        assert_eq!(refs[0].unwrap().five[0], [10.0; 5], "top lines see only the dark");
        assert_eq!(refs[19].unwrap().five[0], [200.0; 5], "bottom lines see only the light");
        let mid = refs[9].unwrap().five[0];
        assert_eq!((mid[0], mid[4]), (10.0, 200.0), "lines at the join see both");
    }

    #[test]
    fn invented_pixels_do_not_amplify_the_noise_of_a_gradient_backdrop() {
        // A tall vertical gradient (190..210) with ±1 noise, and a BGC of 200 that the lines around
        // the middle rows believe. The whole side spans 20 levels while any neighbourhood spans a
        // few. Held to the whole side's spread, the invented pixels there were several times
        // rougher than their source; held to the lines around them they are not.
        let (w, h) = (60u32, 300u32);
        let img = RgbImage::from_fn(w, h, |x, y| {
            let v = 190 + (y * 20 / h) as i32 + ((x * 7 + y * 13) % 3) as i32 - 1;
            Rgb([v as u8; 3])
        });
        let bands = vec![(50i64, 60i64); h as usize];
        let (out, _) = widen_rows(&img, -40, 100, &bands, Some(Rgb([200, 200, 200])), 6);
        let mut diff = 0.0f64;
        let mut n = 0u32;
        for y in 140..160 {
            for x in 0..30u32 {
                diff += (out.get_pixel(x, y)[0] as f64 - out.get_pixel(x + 1, y)[0] as f64).abs();
                n += 1;
            }
        }
        let rough = diff / n as f64;
        assert!(rough < 1.5, "invented background is too rough: mean neighbour difference {rough:.2}");
    }

    #[test]
    fn a_proposal_is_clamped_then_pulled_halfway_to_the_reference_at_its_quantile() {
        let reference = Reference { five: [[100.0, 110.0, 120.0, 130.0, 140.0]; 3], bgc_close: true };
        // Above the range: brought to 140. Neighbours all 90 (below the proposal): quantile 1 ->
        // reference 140; (140 + 140) / 2.
        let above = controlled(Rgb([200, 200, 200]), &[[90, 90, 90]; 5], &reference);
        assert_eq!(above.0, [140, 140, 140]);
        // Inside the range, on its neighbours' first quartile: averaged with the reference's Q1.
        let around: Vec<[u8; 3]> = vec![[100, 100, 100], [105, 105, 105], [110, 110, 110], [115, 115, 115]];
        // 105 has 1 of 4 below and 1 equal: quantile 0.375 -> between Q1 (110) and the median (120).
        let mid = controlled(Rgb([105, 105, 105]), &around, &reference);
        let expected = ((105.0 + reference.at(0, 0.375)) / 2.0f32).round() as u8;
        assert_eq!(mid[0], expected);
        // No neighbours: judged against the median.
        let alone = controlled(Rgb([110, 110, 110]), &[], &reference);
        assert_eq!(alone[0], 115);
        // Hue holds: all three channels share one quantile, so a coloured pixel is not shifted
        // channel by channel.
        let coloured = controlled(Rgb([105, 135, 105]), &around, &reference);
        assert!(coloured[1] > coloured[0], "green stays above red and blue: {coloured:?}");
    }

    #[test]
    fn without_a_believable_bgc_an_invented_pixel_is_only_held_to_the_range() {
        let reference = Reference { five: [[100.0, 110.0, 120.0, 130.0, 140.0]; 3], bgc_close: false };
        let around: Vec<[u8; 3]> = vec![[100, 100, 100], [105, 105, 105], [110, 110, 110]];
        assert_eq!(controlled(Rgb([105, 105, 105]), &around, &reference).0, [105, 105, 105], "inside: untouched");
        assert_eq!(controlled(Rgb([200, 20, 120]), &around, &reference).0, [140, 100, 120], "outside: clamped");
    }

    #[test]
    fn neighbour_row_borrow_fills_a_bled_row_from_its_clean_neighbour() {
        // 4-wide, 3-row mid. Row 1's lo segment (cols 0..2) is bled (flat white); rows 0 and 2 hold
        // clean background (100) there. After the borrow, row 1 cols 0..2 take the neighbour's 100.
        let mut mid = RgbImage::from_pixel(4, 3, Rgb([100, 100, 100]));
        mid.put_pixel(0, 1, Rgb([255, 255, 255]));
        mid.put_pixel(1, 1, Rgb([255, 255, 255]));
        let fills = vec![
            LineFill { lo_bled: false, hi_bled: false, a: 2, b: 4 },
            LineFill { lo_bled: true, hi_bled: false, a: 2, b: 4 },
            LineFill { lo_bled: false, hi_bled: false, a: 2, b: 4 },
        ];
        borrow_neighbour_rows(&mut mid, &fills);
        assert_eq!(*mid.get_pixel(0, 1), Rgb([100, 100, 100]), "bled col 0 borrowed");
        assert_eq!(*mid.get_pixel(1, 1), Rgb([100, 100, 100]), "bled col 1 borrowed");
        assert_eq!(*mid.get_pixel(3, 1), Rgb([100, 100, 100]), "non-bled col untouched");
    }

    #[test]
    fn neighbour_row_borrow_keeps_the_fill_when_the_whole_column_is_bled() {
        // Column 0 is bled on every row: no clean neighbour exists, so the BGC/white fill stays.
        let mut mid = RgbImage::new(2, 3);
        for y in 0..3 {
            mid.put_pixel(0, y, Rgb([255, 255, 255]));
            mid.put_pixel(1, y, Rgb([100, 100, 100]));
        }
        let fills = vec![LineFill { lo_bled: true, hi_bled: false, a: 1, b: 2 }; 3];
        borrow_neighbour_rows(&mut mid, &fills);
        for y in 0..3 {
            assert_eq!(*mid.get_pixel(0, y), Rgb([255, 255, 255]), "no neighbour → keep fill");
        }
    }

    #[test]
    fn transpose_swaps_rows_and_columns_and_back() {
        let img = ramp(5, 3);
        let t = transpose(&img);
        assert_eq!(t.dimensions(), (3, 5));
        assert_eq!(t.get_pixel(1, 4)[0], 4, "pixel (x=4,y=1), whose red is 4, lands at (x=1,y=4)");
        assert_eq!(transpose(&t), img);
    }
}
