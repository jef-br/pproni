//! Optimiser: the preprocessor's last step before BiRefNet sees the image. [`optimize`] is the one
//! entry point and does two things in order, on a copy of the working image:
//!
//! 1. **Colour balance** (below), then
//! 2. **Trim**: crop away the empty backdrop, so the product fills more of what the model looks at.
//!    The 16 border pixels (`TRIM_SAMPLES_PER_EDGE` per edge) give the backdrop colour range: per
//!    channel, the lowest and highest sample. A pixel inside the range on all three channels is
//!    backdrop. Backdrop gaps are closed (`TRIM_CLOSE_K`), the tightest box around what is left is the
//!    product. That box is squared to its largest side and grown by a margin (`TRIM_MARGIN_MIN_PX`,
//!    `TRIM_MARGIN_SHARE`), and the crop is that square, slid to stay inside the frame.
//!    A frame with no product left is an error: the image is not processed.
//!
//! # Colour balance
//!
//! The segmentation model finds a subject more reliably when the picture uses its full tonal range
//! and its midtones sit where the subject/background contrast is easiest to read. This is a
//! Photoshop "Levels" move, worked out once for the whole image:
//!
//! 1. **Cut points.** For each colour channel, find `lo` (the highest value with at most 0.68% of
//!    the pixels at or below it) and `hi` (the 99.32 percentile).
//! 2. **One shared range.** Take the lowest `lo` and the highest `hi` of the three channels, so all
//!    three are treated alike. The colour balance is kept and clipping stays small.
//! 3. **Gamma.** `1 + lo/(255-lo) - (255-hi)/hi`, kept between 0.01 and 9.99. 1.00 is neutral,
//!    above 1 lightens the midtones, below 1 darkens them.
//! 4. **One table.** Spread `lo`..`hi` over 0-255, bend the midtones by the gamma, and round once,
//!    into a 256-entry table. Every pixel of every channel is then a single table read.
//!
//! If the table changes nothing, the input image is used as it is. The optimiser never changes the
//! image it is given: the full-resolution `original` the exporter composes from, and the
//! true-colour `working` the exporter samples BGC/FGC from, are never touched.

use super::super::config::{
    T_EDGE_FLAT, T_TEXTURE_HIGHPASS, TRIM_CLOSE_K, TRIM_MARGIN_MIN_PX, TRIM_MARGIN_SHARE,
    TRIM_MIN_KEEP, TRIM_OUTLIER_FLOOR, TRIM_OUTLIER_K, TRIM_SAMPLES_PER_EDGE,
};
use super::super::shot_classifier::geometry::Rect;
use image::{Rgb, RgbImage};

/// `lo` is the highest value that keeps at most this percentage of the pixels at or below it.
const LO_PERCENTILE: f64 = 0.68;

/// `hi` is the lowest value that has at least this percentage of the pixels at or below it: the
/// 99.32 percentile.
const HI_PERCENTILE: f64 = 99.32;

/// Count how many pixels hold each value 0-255 in channel `c` (0 = red, 1 = green, 2 = blue).
/// Kept as a single-channel helper for the tests below; `shared_range` builds all three in one pass.
#[cfg(test)]
fn histogram(img: &RgbImage, c: usize) -> [u64; 256] {
    let mut hist = [0u64; 256];
    for px in img.pixels() {
        hist[px[c] as usize] += 1;
    }
    hist
}

/// The two cut points of one channel: `(lo, hi)`.
///
/// `lo` is the highest value `x` such that the pixels at or below `x` are at most `LO_PERCENTILE`
/// percent of all pixels. If even value 0 holds more than that (a black backdrop), `lo` is 0.
/// `hi` is the `HI_PERCENTILE` percentile: the lowest `x` such that the pixels at or below `x` are
/// at least that percent of all pixels. A channel that is all one value `v` gives `(v - 1, v)`.
///
/// Both come straight from histogram positions, so they are already whole numbers. With at least
/// one pixel, `lo` is never 255. For an empty histogram it returns `(255, 0)`, which the caller
/// reads as "no range".
fn cut_points(hist: &[u64; 256]) -> (u8, u8) {
    let total = hist.iter().sum::<u64>() as f64;
    // Whole pixel counts. The 1e-9 keeps an exact tie (say 68 of 10,000 pixels) on the side the
    // rule says, whatever the last bit of the float multiply came out as.
    let most_at_or_below_lo = (LO_PERCENTILE / 100.0 * total + 1e-9).floor() as u64;
    let fewest_at_or_below_hi = (HI_PERCENTILE / 100.0 * total - 1e-9).ceil() as u64;

    // Walk up from black while the running count stays within the budget; the last value that did
    // is `lo`. Values with no pixels add nothing, so `lo` lands on the top of that empty stretch.
    let mut lo = 0u8;
    let mut at_or_below = 0u64;
    for (x, &n) in hist.iter().enumerate() {
        at_or_below += n;
        if at_or_below <= most_at_or_below_lo {
            lo = x as u8;
        } else {
            break;
        }
    }

    // Walk up again until the running count first reaches the 99.32 percentile; that value is `hi`.
    let mut hi = 255u8;
    let mut at_or_below = 0u64;
    for (x, &n) in hist.iter().enumerate() {
        at_or_below += n;
        if at_or_below >= fewest_at_or_below_hi {
            hi = x as u8;
            break;
        }
    }

    (lo, hi)
}

/// The one `(lo, hi)` range all three channels share: the lowest of the three `lo` values and the
/// highest of the three `hi` values. Every channel votes — a channel with no spread still gives its
/// own cut points and can set the shared ones (a pure-red image has `lo` = 0 from its flat green
/// and blue).
///
/// Sharing one range keeps the colour balance: every channel is remapped by the same amounts. It
/// also keeps clipping to a minimum, because the widest range any channel needs is the one used.
///
/// `None` when `hi` is not above `lo`: there is no range to spread, so the image is left alone.
fn shared_range(img: &RgbImage) -> Option<(u8, u8)> {
    let mut hists = [[0u64; 256]; 3];
    for px in img.pixels() {
        hists[0][px[0] as usize] += 1;
        hists[1][px[1] as usize] += 1;
        hists[2][px[2] as usize] += 1;
    }
    let cuts = hists.map(|h| cut_points(&h));
    let lo = cuts.iter().map(|&(lo, _)| lo).min()?;
    let hi = cuts.iter().map(|&(_, hi)| hi).max()?;
    (hi > lo).then_some((lo, hi))
}

/// The gamma is kept between these two. 1.00 is neutral, as in Photoshop's Levels.
const GAMMA_MIN: f64 = 0.01;
const GAMMA_MAX: f64 = 9.99;

/// The midtone gamma that goes with the range `(lo, hi)`: `1 + lo/(255-lo) - (255-hi)/hi`, then kept
/// between `GAMMA_MIN` and `GAMMA_MAX`. 1.00 is neutral, above 1 lightens the midtones and below 1
/// darkens them (the curve itself is built in the table step).
///
/// A higher `lo` pushes it up and a lower `hi` pulls it down: it leans against the brightness
/// shift that the remap to `lo`..`hi` causes. The same limits apply to every image.
///
/// The caller must pass `hi > lo`, which is what `shared_range` returns. That makes `lo` at most
/// 254 and `hi` at least 1, so neither division can be by zero.
fn gamma(lo: u8, hi: u8) -> f64 {
    debug_assert!(hi > lo);
    let (lo, hi) = (lo as f64, hi as f64);
    (1.0 + lo / (255.0 - lo) - (255.0 - hi) / hi).clamp(GAMMA_MIN, GAMMA_MAX)
}

/// The 256-entry table that turns an input value into its output value: spread `lo`..`hi` over
/// 0-255, then bend the midtones by `gamma`. Everything at or below `lo` becomes 0 and everything at
/// or above `hi` becomes 255.
///
/// This is the only place the optimiser rounds. `gamma` stays at full precision and each entry is
/// rounded once, as it is made, so every pixel afterwards costs one table read and nothing else.
///
/// The curve is `255 * t ^ (1 / gamma)` for `t` between 0 and 1, so 0 and 255 stay put, above-1
/// gamma lightens the midtones and below-1 darkens them. The caller passes `hi > lo`.
fn build_table(lo: u8, hi: u8, gamma: f64) -> [u8; 256] {
    debug_assert!(hi > lo);
    let span = (hi - lo) as f64;
    let exponent = 1.0 / gamma;
    let mut table = [0u8; 256];
    for (x, slot) in table.iter_mut().enumerate() {
        let t = ((x as f64 - lo as f64) / span).clamp(0.0, 1.0);
        *slot = (255.0 * t.powf(exponent)).round() as u8;
    }
    table
}

/// Prepare `img` for the segmentation model: colour balance, then trim.
///
/// Returns the cropped image BiRefNet will see and where that crop sits in `img` (so the mask it
/// produces can be put back). `Err` when nothing but backdrop is left: there is no product to find, and
/// the image is not processed.
pub fn optimize(img: &RgbImage) -> Result<(RgbImage, Rect), String> {
    let balanced = colour_balance(img);
    let balanced = balanced.as_ref().unwrap_or(img);

    let product = product_box(balanced).ok_or("nothing but backdrop: no product to find")?;
    let crop = square_crop(product, balanced.width(), balanced.height());
    let mut cropped = image::imageops::crop_imm(balanced, crop.x, crop.y, crop.w, crop.h).to_image();
    counter_balance_for_birefnet(&mut cropped);
    Ok((cropped, crop))
}

/// Pre-compress each channel so BiRefNet's ImageNet normalisation (`(x/255 − mean) / std` with
/// `mean=[0.485, 0.456, 0.406]`, `std=[0.229, 0.224, 0.225]`) ends up at `pixel/255` instead of
/// the usual roughly `[-2, +3]` range. In other words, the model's own first step undoes this.
///
/// Formula (whole-pixel in, whole-pixel out):
/// * R' = pixel · 0.229 + 123.675
/// * G' = pixel · 0.224 + 116.28
/// * B' = pixel · 0.225 + 103.53
///
/// Applied only to the cropped copy BiRefNet reads; `working` and `original` are untouched, so
/// the exporter's BGC/FGC sampling is unaffected.
fn counter_balance_for_birefnet(img: &mut RgbImage) {
    const MEAN_255: [f32; 3] = [123.675, 116.28, 103.53];
    const STD: [f32; 3] = [0.229, 0.224, 0.225];
    let tables: [[u8; 256]; 3] = [0, 1, 2].map(|c| {
        let mut t = [0u8; 256];
        for (x, out) in t.iter_mut().enumerate() {
            *out = (x as f32 * STD[c] + MEAN_255[c]).round().clamp(0.0, 255.0) as u8;
        }
        t
    });
    for px in img.pixels_mut() {
        px[0] = tables[0][px[0] as usize];
        px[1] = tables[1][px[1] as usize];
        px[2] = tables[2][px[2] as usize];
    }
}

/// The border pixels that define the backdrop colour: `TRIM_SAMPLES_PER_EDGE` on each edge, going
/// round the frame clockwise (top left to right, right top to bottom, bottom right to left, left
/// bottom to top). On an edge of length `n`, sample `i` sits `i * n / TRIM_SAMPLES_PER_EDGE` pixels
/// from the edge's start, rounded down. The next edge's start is this edge's 100% point, so every
/// corner is sampled exactly once.
fn border_samples(img: &RgbImage) -> Vec<Rgb<u8>> {
    let (w, h) = img.dimensions();
    let n = TRIM_SAMPLES_PER_EDGE;
    let mut samples = Vec::with_capacity(4 * n as usize);
    for i in 0..n {
        let along_w = i * w / n;
        let along_h = i * h / n;
        samples.push(*img.get_pixel(along_w, 0));
        samples.push(*img.get_pixel(w - 1, along_h));
        samples.push(*img.get_pixel(w - 1 - along_w, h - 1));
        samples.push(*img.get_pixel(0, h - 1 - along_h));
    }
    samples
}

/// Keep only the border samples whose colour sits near the per-channel median of the group.
///
/// A sample's L1 colour distance to the median (`|ΔR|+|ΔG|+|ΔB|`) is kept when it is at or below
/// `TRIM_OUTLIER_FLOOR + TRIM_OUTLIER_K * MAD`, where MAD is the median of those 16 distances.
/// The floor guards near-uniform edges from being shredded; the MAD term lets noisier edges keep a
/// wider spread. `TRIM_MIN_KEEP` samples closest to the median are always kept, so a mostly-product
/// edge can't collapse the backdrop range to one colour.
///
/// Protects `colour_range` on bleed-off shots, where a sampled corner can land on the product.
fn drop_outliers(samples: &[Rgb<u8>]) -> Vec<Rgb<u8>> {
    let median_channel = |c: usize| {
        let mut v: Vec<u8> = samples.iter().map(|p| p[c]).collect();
        v.sort_unstable();
        v[v.len() / 2]
    };
    let med = [median_channel(0), median_channel(1), median_channel(2)];
    let dist = |p: &Rgb<u8>| -> u32 {
        (0..3).map(|c| p[c].abs_diff(med[c]) as u32).sum()
    };
    let dists: Vec<u32> = samples.iter().map(dist).collect();
    let mad = {
        let mut sorted = dists.clone();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    };
    let threshold = TRIM_OUTLIER_FLOOR + (TRIM_OUTLIER_K * mad as f64).round() as u32;

    let keep_rule: Vec<bool> = dists.iter().map(|&d| d <= threshold).collect();
    if keep_rule.iter().filter(|&&k| k).count() >= TRIM_MIN_KEEP {
        return samples.iter().zip(&keep_rule).filter_map(|(s, &k)| k.then_some(*s)).collect();
    }

    // Rule would drop too many: fall back to the TRIM_MIN_KEEP samples closest to the median.
    let mut idx: Vec<usize> = (0..samples.len()).collect();
    idx.sort_by_key(|&i| dists[i]);
    idx.truncate(TRIM_MIN_KEEP);
    idx.into_iter().map(|i| samples[i]).collect()
}

/// Per channel, the lowest and highest value over `samples`: `(lows, highs)`.
fn colour_range(samples: &[Rgb<u8>]) -> ([u8; 3], [u8; 3]) {
    let mut lows = [255u8; 3];
    let mut highs = [0u8; 3];
    for px in samples {
        for c in 0..3 {
            lows[c] = lows[c].min(px[c]);
            highs[c] = highs[c].max(px[c]);
        }
    }
    (lows, highs)
}

/// The tightest box around the product, or `None` when there is none.
///
/// A pixel is product when it is outside the backdrop colour range (see [`border_samples`] and
/// [`colour_range`]) *and* sits inside a `TRIM_CLOSE_K` x `TRIM_CLOSE_K` block of such pixels; that is
/// the same as closing the backdrop with a square window of that size. Anything thinner is treated
/// as backdrop, and so is everything outside the frame. The box spans every such block.
///
/// `TRIM_CLOSE_K` is fixed at 2, so each block is probed as its four corners directly — no
/// summed-area table, no auxiliary allocation.
fn product_box(img: &RgbImage) -> Option<Rect> {
    const _: () = assert!(TRIM_CLOSE_K == 2, "product_box's 2x2 probe assumes TRIM_CLOSE_K == 2");

    let (w, h) = img.dimensions();
    let k = TRIM_CLOSE_K;
    if w < k || h < k {
        return None;
    }
    let (lows, highs) = colour_range(&drop_outliers(&border_samples(img)));
    let is_product = |x: u32, y: u32| {
        let px = img.get_pixel(x, y);
        (0..3).any(|c| px[c] < lows[c] || px[c] > highs[c])
    };
    let block_is_product = |x: u32, y: u32| {
        is_product(x, y) && is_product(x + 1, y) && is_product(x, y + 1) && is_product(x + 1, y + 1)
    };

    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..=h - k {
        for x in 0..=w - k {
            if block_is_product(x, y) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + k - 1);
                y1 = y1.max(y + k - 1);
            }
        }
    }
    (x0 != u32::MAX).then(|| Rect::from_bounds(x0, y0, x1, y1))
}

// ---- Per-edge trim + texture detector (plan Part C; shared with deferred Part D) --------------
//
// A measurement primitive, upstream of the mask. It reports, per edge, whether that border is a
// flat band and how it relates to the frame's backdrop, plus whether the background outside the
// product proxy is textured. The plan's decision table reads this to drive trimming (Part C) or a
// frame peel (deferred Part D). Only the MEASUREMENT lives here for now: wiring the actions into
// `optimize` changes real output and must be tuned against the mountain/268 stems first, so the
// primitive is deliberately not called by the pipeline yet.

// <jbpair goal="Part C per-edge trim/texture detector + trim_letterbox: edge-flatness span, same-colour and bar-depth tolerances, the high-pass texture threshold, and the letterbox/pillarbox-trim decision (incl. the 268 EIX-touch consequence); unwired — build the crop/seam-carve re-framing and tune these on the real 41/46 stems in /pair">
/// How one edge relates to the frame's backdrop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
enum EdgeKind {
    /// Flat, and its colour matches the overall border backdrop — a plain backdrop band.
    Backdrop,
    /// Flat, but a different colour from the backdrop — a coloured pad or frame band.
    FlatOther,
    /// Not flat — varied content reaches this edge.
    Varied,
}

/// `TRIM_SAMPLES_PER_EDGE` colour samples along one edge, both corners included (so neighbouring
/// edges share their shared corner). `edge`: 0 = top, 1 = right, 2 = bottom, 3 = left.
fn edge_samples(img: &RgbImage, edge: u8) -> Vec<Rgb<u8>> {
    let (w, h) = img.dimensions();
    let n = TRIM_SAMPLES_PER_EDGE.max(2);
    // Evenly spaced positions from 0 to len-1 inclusive, so both corners are sampled.
    let at = |len: u32, i: u32| i * (len - 1) / (n - 1);
    (0..n)
        .map(|i| match edge {
            0 => *img.get_pixel(at(w, i), 0),
            1 => *img.get_pixel(w - 1, at(h, i)),
            2 => *img.get_pixel(at(w, i), h - 1),
            _ => *img.get_pixel(0, at(h, i)),
        })
        .collect()
}

/// Per-channel median of a sample set.
fn median_rgb(samples: &[Rgb<u8>]) -> [u8; 3] {
    let mut out = [0u8; 3];
    for (c, slot) in out.iter_mut().enumerate() {
        let mut v: Vec<u8> = samples.iter().map(|p| p[c]).collect();
        v.sort_unstable();
        *slot = v[v.len() / 2];
    }
    out
}

/// One edge's profile: `true` when its kept samples span at most `T_EDGE_FLAT` on every channel,
/// plus their median colour. Outliers (a corner that caught the product) are dropped first, reusing
/// the trim rule so the two statistics agree.
fn edge_flat_median(samples: &[Rgb<u8>]) -> (bool, [u8; 3]) {
    let kept = drop_outliers(samples);
    let (lows, highs) = colour_range(&kept);
    let flat = (0..3).all(|c| (highs[c] as u16).saturating_sub(lows[c] as u16) <= T_EDGE_FLAT);
    (flat, median_rgb(&kept))
}

/// Classify all four edges `[T, R, B, L]` against the frame's backdrop (the median of every border
/// sample, outliers dropped). A flat edge within `T_EDGE_FLAT` of that backdrop on every channel is
/// `Backdrop`; a flat edge of another colour is `FlatOther`; a non-flat edge is `Varied`.
#[cfg_attr(not(test), allow(dead_code))]
fn classify_trim_edges(img: &RgbImage) -> [EdgeKind; 4] {
    let backdrop = median_rgb(&drop_outliers(&border_samples(img)));
    [0u8, 1, 2, 3].map(|edge| {
        let (flat, med) = edge_flat_median(&edge_samples(img, edge));
        if !flat {
            EdgeKind::Varied
        } else if (0..3).all(|c| med[c].abs_diff(backdrop[c]) as u16 <= T_EDGE_FLAT) {
            EdgeKind::Backdrop
        } else {
            EdgeKind::FlatOther
        }
    })
}

/// Mean 3x3-Laplacian magnitude (on luma) over the interior of `region` — a cheap texture measure.
/// A high response means a textured background (a Part C scene); near-zero means a flat region (a
/// frame/backdrop — the deferred Part D double-flat signal). Compared against `T_TEXTURE_HIGHPASS`.
fn highpass_texture(img: &RgbImage, region: Rect) -> f32 {
    let luma = |x: u32, y: u32| {
        let p = img.get_pixel(x, y);
        0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
    };
    if region.w < 3 || region.h < 3 {
        return 0.0;
    }
    let (x0, y0) = (region.x, region.y);
    let (x1, y1) = (region.x + region.w, region.y + region.h);
    let (mut sum, mut n) = (0.0f32, 0u32);
    for y in (y0 + 1)..(y1 - 1) {
        for x in (x0 + 1)..(x1 - 1) {
            let lap = 4.0 * luma(x, y)
                - luma(x - 1, y)
                - luma(x + 1, y)
                - luma(x, y - 1)
                - luma(x, y + 1);
            sum += lap.abs();
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f32
    }
}

/// Whether the background outside the product proxy is textured enough to count as a real scene
/// (Part C) rather than a flat frame/backdrop (Part D). The proxy is `product_box`, since there is
/// no segmask this far upstream. With no product box the whole frame is sampled.
fn background_is_textured(img: &RgbImage) -> bool {
    let frame = Rect { x: 0, y: 0, w: img.width(), h: img.height() };
    // Sampling the whole frame also sweeps the product, but a texture *above* the threshold there
    // only reinforces "textured"; the quiet-background case is the one that must read low, and it
    // does. A tighter exclusion of the product box is a tuning refinement for /pair.
    highpass_texture(img, frame) > T_TEXTURE_HIGHPASS
}

/// Whether ≥99% of row `y`'s pixels are within `T_EDGE_FLAT` of `bar` on every channel.
fn row_is_bar(img: &RgbImage, y: u32, bar: [u8; 3]) -> bool {
    let w = img.width();
    let ok = (0..w)
        .filter(|&x| {
            let p = img.get_pixel(x, y);
            (0..3).all(|c| p[c].abs_diff(bar[c]) as u16 <= T_EDGE_FLAT)
        })
        .count() as u32;
    w > 0 && ok * 100 >= w * 99
}

/// Column counterpart of [`row_is_bar`].
fn col_is_bar(img: &RgbImage, x: u32, bar: [u8; 3]) -> bool {
    let h = img.height();
    let ok = (0..h)
        .filter(|&y| {
            let p = img.get_pixel(x, y);
            (0..3).all(|c| p[c].abs_diff(bar[c]) as u16 <= T_EDGE_FLAT)
        })
        .count() as u32;
    h > 0 && ok * 100 >= h * 99
}

/// How many lines inward from `edge` stay a flat bar of colour `bar`. `edge`: 0 = top, 1 = right,
/// 2 = bottom, 3 = left.
fn bar_depth(img: &RgbImage, edge: u8, bar: [u8; 3]) -> u32 {
    let (w, h) = img.dimensions();
    match edge {
        0 => (0..h).take_while(|&y| row_is_bar(img, y, bar)).count() as u32,
        2 => (0..h).take_while(|&k| row_is_bar(img, h - 1 - k, bar)).count() as u32,
        3 => (0..w).take_while(|&x| col_is_bar(img, x, bar)).count() as u32,
        1 => (0..w).take_while(|&k| col_is_bar(img, w - 1 - k, bar)).count() as u32,
        _ => 0,
    }
}

/// Detect symmetric flat letterbox/pillarbox bars and return the content rectangle to keep, or
/// `None` when there is nothing safe to trim (plan Part C's trim action — the mountain/268 case).
///
/// An axis is trimmed only when BOTH its edges are flat bars of the same colour AND BOTH edges of
/// the *other* axis are `Varied` (textured content reaches them), and the kept interior itself
/// reads as textured. Those conditions are what a genuine letterbox/pillarbox looks like, and they
/// are exactly the hard bound: a freestanding product on a plain backdrop has flat edges on *both*
/// axes, so neither is trimmed and `EIX0000` can never flip. A coloured frame (all four flat — the
/// deferred Part D case) likewise has no `Varied` axis, so it is left alone.
pub fn trim_letterbox(img: &RgbImage) -> Option<Rect> {
    let (w, h) = img.dimensions();
    let prof = [0u8, 1, 2, 3].map(|e| edge_flat_median(&edge_samples(img, e)));
    let flat = |e: usize| prof[e].0;
    let col = |e: usize| prof[e].1;
    let same = |a: usize, b: usize| (0..3).all(|c| col(a)[c].abs_diff(col(b)[c]) as u16 <= T_EDGE_FLAT);

    // Top=0, Right=1, Bottom=2, Left=3.
    let horiz = flat(0) && flat(2) && same(0, 2) && !flat(1) && !flat(3); // top/bottom bars
    let vert = flat(3) && flat(1) && same(3, 1) && !flat(0) && !flat(2); // left/right bars
    if !horiz && !vert {
        return None;
    }

    let (mut x0, mut y0, mut x1, mut y1) = (0u32, 0u32, w, h);
    if horiz {
        y0 = bar_depth(img, 0, col(0));
        y1 = h - bar_depth(img, 2, col(2));
    }
    if vert {
        x0 = bar_depth(img, 3, col(3));
        x1 = w - bar_depth(img, 1, col(1));
    }
    // Refuse a pathological over-trim that would keep less than a quarter of either axis.
    if x1 <= x0 + w / 4 || y1 <= y0 + h / 4 {
        return None;
    }
    let keep = Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
    // Hard-bound confirmation: only trim when what remains is a genuine textured scene.
    let interior = image::imageops::crop_imm(img, keep.x, keep.y, keep.w, keep.h).to_image();
    background_is_textured(&interior).then_some(keep)
}
// </jbpair>

/// The crop BiRefNet is shown: a square around `product`, inside the frame `[0, w) x [0, h)`.
///
/// The product box is squared to its largest side, then grown by a margin on every side: the larger of
/// `TRIM_MARGIN_MIN_PX` and `TRIM_MARGIN_SHARE` of that side (rounded down). The square
/// is centred on the product and slid until it is inside the frame, so the whole product box is always
/// in it.
///
/// If that square is wider than the frame's shorter side but the product still fits, the margin is
/// squeezed so the square is exactly the shorter side. If the product's largest side is itself longer
/// than the frame's shorter side, no square fits: the margin drops to its minimum, `TRIM_MARGIN_MIN_PX`,
/// each axis takes the wanted side or the whole axis, whichever is shorter, and the crop is not square.
fn square_crop(product: Rect, w: u32, h: u32) -> Rect {
    let longest = product.w.max(product.h);
    let fits = longest <= w.min(h);
    let margin = if fits {
        TRIM_MARGIN_MIN_PX.max((TRIM_MARGIN_SHARE * longest as f64).floor() as u32)
    } else {
        TRIM_MARGIN_MIN_PX
    };
    let mut side = longest + 2 * margin;
    if fits {
        side = side.min(w.min(h));
    }
    let (x, crop_w) = span(product.x, product.w, side, w);
    let (y, crop_h) = span(product.y, product.h, side, h);
    Rect { x, y, w: crop_w, h: crop_h }
}

/// One axis of [`square_crop`]: a stretch of `side` pixels (or the whole axis, if that is shorter)
/// centred on the product's stretch `start..start + len`, slid to stay inside `0..axis`. The product's
/// stretch is never longer than the result, so it is always inside it.
fn span(start: u32, len: u32, side: u32, axis: u32) -> (u32, u32) {
    let side = side.min(axis);
    let centred = (start as i64 + (start + len) as i64 - side as i64).div_euclid(2);
    (centred.clamp(0, (axis - side) as i64) as u32, side)
}

/// The colour balance described in the module docs.
///
/// Returns the balanced copy, or `None` when there is nothing to change: the image has no range
/// (for example it is all black) or the table comes out as the identity. `None` means "use `img`
/// itself": no copy is made and no pixel is touched.
///
/// One pass over the pixels: all three channels read the same table.
fn colour_balance(img: &RgbImage) -> Option<RgbImage> {
    let (lo, hi) = shared_range(img)?;
    let g = gamma(lo, hi);
    super::super::trace::note(|| format!("levels: lo={lo} hi={hi} gamma={g:.3}"));
    let table = build_table(lo, hi, g);
    if table.iter().enumerate().all(|(x, &y)| y as usize == x) {
        return None;
    }
    let data = img.as_raw().iter().map(|&b| table[b as usize]).collect();
    Some(RgbImage::from_raw(img.width(), img.height(), data).expect("a table read keeps the buffer length"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// Endpoints are pinned: the darkest pixel comes out 0 and the brightest 255.
    #[test]
    fn extremes_map_to_extremes() {
        let mut img = RgbImage::from_pixel(16, 16, Rgb([120, 120, 120]));
        img.put_pixel(0, 0, Rgb([10, 10, 10]));
        img.put_pixel(1, 0, Rgb([240, 240, 240]));
        let out = colour_balance(&img).expect("this image changes");
        assert_eq!(out.get_pixel(0, 0)[0], 0);
        assert_eq!(out.get_pixel(1, 0)[0], 255);
    }

    #[test]
    fn a_flat_edge_reads_flat_with_its_colour() {
        let img = RgbImage::from_pixel(40, 40, Rgb([255, 255, 255]));
        let (flat, med) = edge_flat_median(&edge_samples(&img, 0));
        assert!(flat);
        assert_eq!(med, [255, 255, 255]);
    }

    #[test]
    fn a_plain_backdrop_reads_all_backdrop() {
        // A product blob floating on a plain backdrop, not touching any edge.
        let mut img = RgbImage::from_pixel(60, 60, Rgb([250, 250, 250]));
        for y in 20..40 {
            for x in 20..40 {
                img.put_pixel(x, y, Rgb([10, 20, 30]));
            }
        }
        assert_eq!(classify_trim_edges(&img), [EdgeKind::Backdrop; 4]);
    }

    #[test]
    fn a_coloured_flat_band_reads_flatother_against_a_white_backdrop() {
        // White backdrop, with a full-height blue band down the left 6 columns (a coloured pad).
        let mut img = RgbImage::from_pixel(60, 60, Rgb([250, 250, 250]));
        for y in 0..60 {
            for x in 0..6 {
                img.put_pixel(x, y, Rgb([40, 60, 200]));
            }
        }
        let kinds = classify_trim_edges(&img);
        assert_eq!(kinds[3], EdgeKind::FlatOther, "left edge is a coloured flat band");
        assert_eq!(kinds[1], EdgeKind::Backdrop, "right edge is plain backdrop");
    }

    #[test]
    fn trim_letterbox_crops_pillarbox_bars_on_a_textured_scene() {
        // White bars down the left and right 10 columns; a textured scene between (a pseudo-random
        // pattern, so the top/bottom edges read as varied at their sample columns rather than
        // aliasing back to white).
        let mut img = RgbImage::from_pixel(60, 60, Rgb([255, 255, 255]));
        for y in 0..60u32 {
            for x in 10..50u32 {
                let v = ((x * 37 + y * 101) % 256) as u8;
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        let keep = trim_letterbox(&img).expect("pillarbox should be trimmed");
        assert_eq!(keep, Rect { x: 10, y: 0, w: 40, h: 60 });
    }

    #[test]
    fn trim_letterbox_leaves_a_freestanding_product_on_a_backdrop() {
        // All four edges are flat backdrop: a product floating on a plain backdrop. Trimming here
        // would risk flipping EIX0000, so it must be left alone (the hard bound).
        let mut img = RgbImage::from_pixel(60, 60, Rgb([250, 250, 250]));
        for y in 24..36 {
            for x in 24..36 {
                img.put_pixel(x, y, Rgb([10, 20, 30]));
            }
        }
        assert!(trim_letterbox(&img).is_none());
    }

    #[test]
    fn highpass_separates_a_flat_region_from_texture() {
        let flat = RgbImage::from_pixel(20, 20, Rgb([128, 128, 128]));
        assert_eq!(highpass_texture(&flat, Rect { x: 0, y: 0, w: 20, h: 20 }), 0.0);
        assert!(!background_is_textured(&flat));

        let mut checker = RgbImage::new(20, 20);
        for y in 0..20 {
            for x in 0..20 {
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                checker.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        assert!(highpass_texture(&checker, Rect { x: 0, y: 0, w: 20, h: 20 }) > T_TEXTURE_HIGHPASS);
        assert!(background_is_textured(&checker));
    }

    /// A full-range image (half black, half white) has an identity table: the input is used as it is.
    #[test]
    fn a_full_range_image_is_used_as_it_is() {
        let mut img = RgbImage::new(10, 10);
        for (i, px) in img.pixels_mut().enumerate() {
            *px = if i % 2 == 0 { Rgb([0, 0, 0]) } else { Rgb([255, 255, 255]) };
        }
        assert_eq!(colour_balance(&img), None);
    }

    /// An all-black image has no range, so it is used as it is too.
    #[test]
    fn an_all_black_image_is_used_as_it_is() {
        assert_eq!(colour_balance(&RgbImage::from_pixel(10, 10, Rgb([0, 0, 0]))), None);
    }

    /// Every pixel comes out as the table says, and all three channels read the same table, so a grey
    /// stays grey: the colour balance is kept.
    #[test]
    fn all_three_channels_share_one_table() {
        let mut img = RgbImage::new(150, 4);
        for (i, px) in img.pixels_mut().enumerate() {
            let x = (50 + (i % 150)) as u8; // 50..199, spread so lo and hi sit well inside
            *px = Rgb([x, x, x]);
        }
        let out = colour_balance(&img).expect("this image changes");
        let (lo, hi) = shared_range(&img).unwrap();
        let table = build_table(lo, hi, gamma(lo, hi));
        for (a, b) in img.pixels().zip(out.pixels()) {
            assert_eq!(b[0], table[a[0] as usize]);
            assert_eq!(b[0], b[1]);
            assert_eq!(b[1], b[2]);
        }
    }

    /// The pure-red example: shared range 0..200, gamma 0.725. Red 200 comes out 255, the zeros stay 0.
    #[test]
    fn red_200_green_0_blue_0_comes_out_255_0_0() {
        let img = RgbImage::from_pixel(10, 10, Rgb([200, 0, 0]));
        let out = colour_balance(&img).expect("this image changes");
        assert_eq!(*out.get_pixel(5, 5), Rgb([255, 0, 0]));
    }

    /// Build a histogram from `(value, pixel count)` pairs.
    fn hist_of(parts: &[(usize, u64)]) -> [u64; 256] {
        let mut hist = [0u64; 256];
        for &(x, n) in parts {
            hist[x] += n;
        }
        hist
    }

    /// The worked example: of 10,000 pixels, 61 sit at 50 (0.61%) and 23 more at 138, so the
    /// running share is 0.61% up to 137 and 0.84% at 138. The highest value still within 0.68% is 137.
    #[test]
    fn lo_is_the_highest_value_within_the_dark_budget() {
        let hist = hist_of(&[(50, 61), (138, 23), (200, 9916)]);
        assert_eq!(cut_points(&hist).0, 137);
    }

    /// The same 10,000 pixels read from the top: 9,916 at 100 (99.16% at or below) and 23 at 117
    /// (99.39% at or below). 117 is the first value to reach 99.32%.
    #[test]
    fn hi_is_the_99_32_percentile() {
        let hist = hist_of(&[(100, 9916), (117, 23), (200, 61)]);
        assert_eq!(cut_points(&hist).1, 117);
    }

    /// An exact tie counts as reaching it: 9,932 of 10,000 pixels at or below 100 is exactly 99.32%.
    #[test]
    fn a_tie_at_the_percentile_counts_as_reaching_it() {
        let hist = hist_of(&[(100, 9932), (200, 68)]);
        assert_eq!(cut_points(&hist).1, 100);
        // And 68 of 10,000 at or below is exactly 0.68%, which the lo budget allows.
        let hist = hist_of(&[(10, 68), (100, 9932)]);
        assert_eq!(cut_points(&hist).0, 99);
    }

    /// A backdrop that is itself over the budget cannot be cut into: lo falls back to 0 and hi to 255.
    #[test]
    fn a_black_and_white_image_keeps_the_full_range() {
        let hist = hist_of(&[(0, 5000), (255, 5000)]);
        assert_eq!(cut_points(&hist), (0, 255));
    }

    /// A channel that is entirely one value v gives (v - 1, v); at 0 there is no room below.
    #[test]
    fn a_flat_channel_sits_just_under_its_value() {
        assert_eq!(cut_points(&hist_of(&[(200, 10_000)])), (199, 200));
        assert_eq!(cut_points(&hist_of(&[(0, 10_000)])), (0, 0));
    }

    /// The pure-red example: R all 200, G and B all 0. The flat green and blue set `lo` = 0, red
    /// sets `hi` = 200.
    #[test]
    fn red_200_green_0_blue_0_shares_lo_0_and_hi_200() {
        let img = RgbImage::from_pixel(10, 10, Rgb([200, 0, 0]));
        assert_eq!(shared_range(&img), Some((0, 200)));
    }

    /// A black image has no range at all: every channel is flat at 0.
    #[test]
    fn an_all_black_image_has_no_range() {
        assert_eq!(shared_range(&RgbImage::from_pixel(10, 10, Rgb([0, 0, 0]))), None);
    }

    /// A pure-red image: red spreads over 100-200, green and blue are flat at 0. The flat channels
    /// vote too, so the shared `lo` is 0 (their `lo`), and the shared `hi` is red's own `hi`.
    #[test]
    fn flat_channels_vote_on_the_shared_range() {
        let mut img = RgbImage::new(100, 100);
        for (i, px) in img.pixels_mut().enumerate() {
            *px = Rgb([100 + (i % 101) as u8, 0, 0]);
        }
        let red_hi = cut_points(&histogram(&img, 0)).1;
        assert_eq!(shared_range(&img), Some((0, red_hi)));
    }

    /// The shared range is the lowest `lo` and the highest `hi` over the three channels.
    #[test]
    fn shared_range_takes_the_lowest_lo_and_the_highest_hi() {
        // Red 50-150, green 100-200, blue 150-250: each spread evenly so the cuts sit near the ends.
        let mut img = RgbImage::new(101, 100);
        for (i, px) in img.pixels_mut().enumerate() {
            let t = (i % 101) as u8;
            *px = Rgb([50 + t, 100 + t, 150 + t]);
        }
        let (lo, hi) = shared_range(&img).expect("a range");
        let lo_r = cut_points(&histogram(&img, 0)).0;
        let hi_b = cut_points(&histogram(&img, 2)).1;
        assert_eq!((lo, hi), (lo_r, hi_b));
    }

    /// The gamma matches the worked examples: the pure-red case, and the two images checked in Photoshop.
    #[test]
    fn gamma_matches_the_worked_examples() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-3;
        assert!(close(gamma(0, 200), 0.725)); // R=200, G=0, B=0
        assert!(close(gamma(41, 247), 1.159)); // 8712345678901.jpg
        assert!(close(gamma(23, 255), 1.1)); // TH12_0N3_M47CH32…
        assert_eq!(gamma(0, 255), 1.0); // full range: neutral
    }

    /// Whatever the formula gives, the same limits apply: below 0.01 it is 0.01, above 9.99 it is 9.99.
    #[test]
    fn gamma_is_kept_between_0_01_and_9_99() {
        assert_eq!(gamma(0, 100), GAMMA_MIN); // formula gives -0.55
        assert_eq!(gamma(0, 1), GAMMA_MIN); // formula gives -253
        assert_eq!(gamma(254, 255), GAMMA_MAX); // formula gives 255
    }

    /// The worked example: lo 41, hi 247, gamma 1.159. 128 maps to 121 (0.422 after the remap, 0.475
    /// after the gamma, times 255). The ends are pinned: at or below lo is 0, at or above hi is 255.
    #[test]
    fn the_table_matches_the_worked_example() {
        let t = build_table(41, 247, 1.159);
        assert_eq!(t[128], 121);
        assert_eq!((t[0], t[41]), (0, 0));
        assert_eq!((t[247], t[255]), (255, 255));
    }

    /// Full range and neutral gamma change nothing.
    #[test]
    fn full_range_and_gamma_one_is_the_identity_table() {
        let t = build_table(0, 255, 1.0);
        for x in 0..256 {
            assert_eq!(t[x], x as u8);
        }
    }

    /// Gamma above 1 lightens the midtones, below 1 darkens them.
    #[test]
    fn gamma_above_one_lightens_and_below_one_darkens() {
        assert!(build_table(0, 255, 2.0)[128] > 128); // 181
        assert!(build_table(0, 255, 0.5)[128] < 128); // 64
    }

    /// A lighter input never comes out darker, whatever the range and gamma.
    #[test]
    fn the_table_never_goes_down() {
        for &(lo, hi, g) in &[(41u8, 247u8, 1.159), (0, 200, 0.725), (178, 246, 3.28), (0, 255, 0.01), (254, 255, 9.99)] {
            let t = build_table(lo, hi, g);
            assert!(t.windows(2).all(|w| w[0] <= w[1]), "not monotonic for {lo},{hi},{g}");
        }
    }

    /// No pixels at all reads as "no range": lo above hi.
    #[test]
    fn an_empty_histogram_has_no_range() {
        let (lo, hi) = cut_points(&[0u64; 256]);
        assert!(hi <= lo);
        assert_eq!(shared_range(&RgbImage::new(0, 0)), None);
    }

    const BACKDROP: Rgb<u8> = Rgb([245, 245, 245]);
    const PRODUCT: Rgb<u8> = Rgb([30, 40, 90]);

    fn paint(img: &mut RgbImage, rect: Rect, colour: Rgb<u8>) {
        for y in rect.y..rect.y + rect.h {
            for x in rect.x..rect.x + rect.w {
                img.put_pixel(x, y, colour);
            }
        }
    }

    fn rect(x: u32, y: u32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    /// 4 samples per edge on an 8x8 frame sit at 0, 2, 4, 6 from each edge's start, going clockwise:
    /// 16 different pixels, every corner once.
    #[test]
    fn sixteen_samples_go_round_the_frame_from_each_edges_start() {
        let mut img = RgbImage::new(8, 8);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgb([x as u8, y as u8, 0]);
        }
        let at: Vec<(u8, u8)> = border_samples(&img).iter().map(|p| (p[0], p[1])).collect();
        let mut expected = vec![
            (0, 0), (2, 0), (4, 0), (6, 0), // top, left to right
            (7, 0), (7, 2), (7, 4), (7, 6), // right, top to bottom
            (7, 7), (5, 7), (3, 7), (1, 7), // bottom, right to left
            (0, 7), (0, 5), (0, 3), (0, 1), // left, bottom to top
        ];
        let mut got = at.clone();
        got.sort();
        expected.sort();
        assert_eq!(got, expected);
    }

    /// Positions are `i * length / 4`, rounded down: on 1000 pixels 0, 250, 500, 750.
    #[test]
    fn samples_on_a_1000_pixel_edge_sit_at_0_250_500_750() {
        let mut img = RgbImage::new(1000, 10);
        for (x, _, px) in img.enumerate_pixels_mut() {
            *px = Rgb([(x / 4) as u8, (x % 4) as u8, 0]);
        }
        let top: Vec<u32> = border_samples(&img).iter().step_by(4).map(|p| p[0] as u32 * 4 + p[1] as u32).collect();
        assert_eq!(top, vec![0, 250, 500, 750]);
    }

    /// The range is the lowest and highest value per channel, over all samples.
    #[test]
    fn the_range_is_the_lowest_and_highest_per_channel() {
        let samples = [Rgb([10, 200, 30]), Rgb([20, 150, 60]), Rgb([15, 180, 40])];
        assert_eq!(colour_range(&samples), ([10, 150, 30], [20, 200, 60]));
    }

    /// A product on a plain backdrop: the box is the product, exactly.
    #[test]
    fn the_box_is_the_product() {
        let mut img = RgbImage::from_pixel(100, 80, BACKDROP);
        paint(&mut img, rect(40, 20, 20, 30), PRODUCT);
        assert_eq!(product_box(&img), Some(rect(40, 20, 20, 30)));
    }

    /// Backdrop noise inside the range does not count, even though it differs from the samples.
    #[test]
    fn a_pixel_inside_the_sampled_range_is_backdrop() {
        let mut img = RgbImage::from_pixel(100, 80, BACKDROP);
        img.put_pixel(0, 0, Rgb([240, 240, 240])); // a corner sample: widens the range to 240..245
        paint(&mut img, rect(40, 20, 20, 30), PRODUCT);
        img.put_pixel(10, 10, Rgb([242, 243, 244])); // inside the range
        assert_eq!(product_box(&img), Some(rect(40, 20, 20, 30)));
    }

    /// Single specks and 1-pixel-thick lines fall away: closing with a 2x2 window.
    #[test]
    fn specks_and_thin_lines_are_backdrop() {
        let mut img = RgbImage::from_pixel(100, 80, BACKDROP);
        paint(&mut img, rect(40, 20, 20, 30), PRODUCT);
        img.put_pixel(5, 5, PRODUCT); // a speck
        paint(&mut img, rect(80, 10, 1, 50), PRODUCT); // a line one pixel thick
        assert_eq!(product_box(&img), Some(rect(40, 20, 20, 30)));
    }

    /// A 2x2 block is the smallest thing that counts.
    #[test]
    fn a_two_by_two_block_counts() {
        let mut img = RgbImage::from_pixel(100, 80, BACKDROP);
        paint(&mut img, rect(70, 40, 2, 2), PRODUCT);
        assert_eq!(product_box(&img), Some(rect(70, 40, 2, 2)));
    }

    /// Two products far apart: one box around both.
    #[test]
    fn two_products_get_one_box() {
        let mut img = RgbImage::from_pixel(100, 80, BACKDROP);
        paint(&mut img, rect(10, 10, 10, 10), PRODUCT);
        paint(&mut img, rect(80, 60, 10, 10), PRODUCT);
        assert_eq!(product_box(&img), Some(rect(10, 10, 80, 60)));
    }

    /// A frame of nothing but backdrop has no product, so `optimize` refuses it.
    #[test]
    fn a_blank_frame_has_no_product() {
        let img = RgbImage::from_pixel(100, 80, BACKDROP);
        assert_eq!(product_box(&img), None);
        assert!(optimize(&img).is_err());
    }

    /// The box is squared to its largest side, then grown by the larger of 10 px and 10% of that side.
    #[test]
    fn the_box_is_squared_to_its_largest_side_then_given_a_margin() {
        // 200 wide x 600 tall: side 600, margin 60, square 720, centred on the product at (500, 500).
        assert_eq!(square_crop(rect(400, 200, 200, 600), 1000, 1000), rect(140, 140, 720, 720));
        // 50 x 55: 10% of 55 is 5.5, rounded to 6, so the 10 px floor wins: 75. Centres land on .5 and round down.
        assert_eq!(square_crop(rect(100, 100, 50, 55), 1000, 1000), rect(87, 90, 75, 75));
        // 10% of 205 is 20.5, rounded down to 20: side 245.
        assert_eq!(square_crop(rect(300, 300, 205, 100), 1000, 1000).w, 245);
        // Exact tenths stay exact despite the float: 10% of every multiple of 10 is that multiple / 10.
        for n in (100..=2000).step_by(10) {
            assert_eq!(square_crop(rect(500, 500, n, n), 3000, 3000).w, n + 2 * (n / 10), "n = {n}");
        }
    }

    /// A square that would stick out of the frame is slid back inside it, with the product still in it.
    #[test]
    fn the_square_slides_to_stay_inside_the_frame() {
        assert_eq!(square_crop(rect(0, 0, 100, 100), 1000, 1000), rect(0, 0, 120, 120));
        assert_eq!(square_crop(rect(900, 900, 100, 100), 1000, 1000), rect(880, 880, 120, 120));
    }

    /// Wider than the frame's shorter side, but the product fits: the margin is squeezed, still square.
    #[test]
    fn the_margin_is_squeezed_when_the_square_is_wider_than_the_frame() {
        // Frame 672 x 1000, product 200 x 600 at (30, 100): the square would be 720, so it is 672.
        assert_eq!(square_crop(rect(30, 100, 200, 600), 672, 1000), rect(0, 64, 672, 672));
    }

    /// No square can hold the product inside the frame: each axis is cut at the frame, not square.
    #[test]
    fn a_product_longer_than_the_frames_short_side_gets_a_crop_cut_at_the_frame() {
        // Frame 672 x 1000, product 300 x 800: the margin drops to 10 px, so the wanted side is 820.
        // Whole width, 820 tall.
        assert_eq!(square_crop(rect(100, 100, 300, 800), 672, 1000), rect(0, 90, 672, 820));
    }

    /// Whatever the placement, the crop holds the whole product box and stays inside the frame.
    #[test]
    fn the_crop_always_holds_the_product_and_stays_in_the_frame() {
        for &(w, h) in &[(1000u32, 1000u32), (672, 1000), (1000, 400)] {
            for &(x, y, bw, bh) in &[(0, 0, 50, 50), (w - 50, h - 50, 50, 50), (10, 5, w - 20, h - 10), (w / 2, h / 3, 30, h / 3)] {
                let product = rect(x, y, bw, bh);
                let c = square_crop(product, w, h);
                assert!(c.x <= product.x && c.y <= product.y, "{product:?} in {c:?} ({w}x{h})");
                assert!(c.x + c.w >= product.x + product.w && c.y + c.h >= product.y + product.h, "{product:?} in {c:?} ({w}x{h})");
                assert!(c.x + c.w <= w && c.y + c.h <= h, "{c:?} outside {w}x{h}");
            }
        }
    }

    /// End to end: the crop is the squared product plus its margin, and it comes back with where it sits.
    #[test]
    fn optimize_crops_to_the_product_plus_margin() {
        let mut img = RgbImage::from_pixel(200, 100, BACKDROP);
        paint(&mut img, rect(70, 30, 60, 40), PRODUCT);
        let (cropped, crop) = optimize(&img).expect("a product");
        // Product 60 x 40 -> side 60, margin 10, square 80, centred on (100, 50), slid down into the frame.
        assert_eq!(crop, rect(60, 10, 80, 80));
        assert_eq!(cropped.dimensions(), (80, 80));
    }
}
