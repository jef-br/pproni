//! BGC / FGC: the background and foreground colours the debug tags report, measured once, here,
//! from the true-colour working copy split by the mask (background = inverse mask, foreground =
//! mask). The colour optimiser never touches that copy, so the colours describe the original image
//! at working resolution. See docs/architecture/02-debug-tags.md.

use super::super::shot_classifier::Mask;
use image::{Rgb, RgbImage};
use std::collections::HashSet;

/// A colour must occupy at least this fraction of the background region (inverse mask) to be reported as BGC.
const BGC_PRESENCE: f64 = 0.9;

/// A region smaller than this fraction of the frame is too small to sample a colour from.
const MIN_REGION_FRAC: f64 = 0.01;

/// The two sampled colours, each as `rrggbb` when there is one to report.
pub struct ShotColors {
    /// Background colour, when one colour dominates the background region.
    pub bgc: Option<String>,
    /// Foreground colour, when the subject is big enough to sample.
    pub fgc: Option<String>,
}

/// BGC and FGC, derived from the working image partitioned by the mask
/// (bg = inverse mask, fg = mask).
///
/// Working resolution, not the original: `image` and `mask` have to share one
/// coordinate space for the loop below to index both off the same `x`/`y`, and
/// the `min` against the mask's dimensions would quietly clip a full-res image
/// to its top-left corner rather than fail.
pub fn sample(image: &RgbImage, mask: &Mask) -> ShotColors {
    let (mut fg_hist, mut bg_hist) = (Hist::new(), Hist::new());
    let w = image.width().min(mask.width);
    let h = image.height().min(mask.height);
    for y in 0..h {
        for x in 0..w {
            let p = image.get_pixel(x, y).0;
            if mask.get(x, y) {
                fg_hist.add(p);
            } else {
                bg_hist.add(p);
            }
        }
    }
    let frame = (w as u64 * h as u64).max(1) as f64;

    // BGC: a single colour must cover most of the inverse-mask region.
    let bgc = (bg_hist.total() as f64 >= frame * MIN_REGION_FRAC)
        .then(|| dominant_cluster(&bg_hist))
        .flatten()
        .filter(|(frac, _)| *frac >= BGC_PRESENCE)
        .map(|(_, c)| hex(c));

    // FGC: the largest colour blob inside the mask -> its weighted-median colour.
    // "Blob" = fullest histogram cluster (the winning 5-bit bin plus its ±1 neighbours)
    // "Blob" =/= a spatially-connected region.
    let fgc = (fg_hist.total() as f64 >= frame * MIN_REGION_FRAC)
        .then(|| dominant_bin(&fg_hist))
        .flatten()
        .map(|top| {
            // Per-channel value histograms over the blob's pixels, for a
            // frequency-weighted median that shrugs off folds/shadows.
            let mut chan = [[0u64; 256]; 3];
            for y in 0..h {
                for x in 0..w {
                    if !mask.get(x, y) {
                        continue;
                    }
                    let p = image.get_pixel(x, y).0;
                    if in_cluster(top, p) {
                        for c in 0..3 {
                            chan[c][p[c] as usize] += 1;
                        }
                    }
                }
            }
            hex([median(&chan[0]), median(&chan[1]), median(&chan[2])])
        });

    ShotColors { bgc, fgc }
}

/// Frequency-weighted median of a 256-bin value histogram.
fn median(hist: &[u64; 256]) -> u8 {
    let half = hist.iter().sum::<u64>().div_ceil(2);
    let mut cum = 0u64;
    for (v, &c) in hist.iter().enumerate() {
        cum += c;
        if cum >= half {
            return v as u8;
        }
    }
    0
}

/// 5-bit-per-channel RGB histogram (32³ bins) that also carries the running
/// colour sum per bin, so a bin's mean colour is recoverable.
struct Hist {
    count: Vec<u32>,
    sum: Vec<[u64; 3]>,
}

impl Hist {
    fn new() -> Self {
        Hist { count: vec![0; 32 * 32 * 32], sum: vec![[0; 3]; 32 * 32 * 32] }
    }
    fn add(&mut self, p: [u8; 3]) {
        let i = bin(p[0] >> 3, p[1] >> 3, p[2] >> 3);
        self.count[i] += 1;
        self.sum[i][0] += p[0] as u64;
        self.sum[i][1] += p[1] as u64;
        self.sum[i][2] += p[2] as u64;
    }
    fn total(&self) -> u64 {
        self.count.iter().map(|&c| c as u64).sum()
    }
}

fn bin(r: u8, g: u8, b: u8) -> usize {
    ((r as usize) << 10) | ((g as usize) << 5) | (b as usize)
}

/// Index of the fullest histogram bin, or `None` if the region is empty.
fn dominant_bin(h: &Hist) -> Option<usize> {
    if h.total() == 0 {
        return None;
    }
    h.count.iter().enumerate().max_by_key(|(_, &c)| c).map(|(i, _)| i)
}

/// The bin set forming one colour "blob": `top` plus its ±1 neighbours per
/// channel, so JPEG dither around a boundary stays one colour.
fn cluster_bins(top: usize) -> HashSet<usize> {
    let (tr, tg, tb) = ((top >> 10) & 31, (top >> 5) & 31, top & 31);
    let mut set = HashSet::new();
    for dr in -1..=1i32 {
        for dg in -1..=1i32 {
            for db in -1..=1i32 {
                set.insert(bin(
                    (tr as i32 + dr).clamp(0, 31) as u8,
                    (tg as i32 + dg).clamp(0, 31) as u8,
                    (tb as i32 + db).clamp(0, 31) as u8,
                ));
            }
        }
    }
    set
}

/// Is colour `p` in the blob around bin `top`? The same question as `cluster_bins(top)` containing
/// `p`'s bin, answered without building the set: a bin is in the blob when each of its three channels
/// is within one step of `top`'s (the clamp at 0 and 31 only drops bins that do not exist).
fn in_cluster(top: usize, p: [u8; 3]) -> bool {
    let (tr, tg, tb) = ((top >> 10) & 31, (top >> 5) & 31, top & 31);
    let near = |t: usize, v: u8| t.abs_diff((v >> 3) as usize) <= 1;
    near(tr, p[0]) && near(tg, p[1]) && near(tb, p[2])
}

/// Fraction of the region occupied by the dominant colour blob, and that
/// blob's mean RGB. `None` if the region is empty.
fn dominant_cluster(h: &Hist) -> Option<(f64, [u8; 3])> {
    let region = h.total();
    let top = dominant_bin(h)?;
    let (mut n, mut s) = (0u64, [0u64; 3]);
    for i in cluster_bins(top) {
        n += h.count[i] as u64;
        s[0] += h.sum[i][0];
        s[1] += h.sum[i][1];
        s[2] += h.sum[i][2];
    }
    (n > 0).then(|| (n as f64 / region as f64, [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]))
}

fn hex(c: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Parse a six-digit lowercase `rrggbb` string back into an [`Rgb`]. `None` on any other shape.
/// The pair of this function and [`hex`] is lossless.
pub fn parse_hex(s: &str) -> Option<Rgb<u8>> {
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Rgb([r, g, b]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_background_reports_bgc() {
        // Solid white frame, empty mask: the background is one colour, so BGC is reported.
        let img = RgbImage::from_pixel(40, 40, image::Rgb([255, 255, 255]));
        let mask = Mask { width: 40, height: 40, data: vec![0; 40 * 40] };
        let colors = sample(&img, &mask);

        assert_eq!(colors.bgc.as_deref(), Some("ffffff"));
        assert_eq!(colors.fgc, None);
    }

    /// The distance test is the set lookup it replaced, for bins on the edges of the cube as well as the
    /// middle.
    #[test]
    fn in_cluster_matches_the_cluster_set() {
        let edges = [0usize, 1, 2, 15, 30, 31];
        for &tr in &edges {
            for &tg in &edges {
                for &tb in &edges {
                    let top = bin(tr as u8, tg as u8, tb as u8);
                    let set = cluster_bins(top);
                    for r in 0..32u8 {
                        for g in 0..32u8 {
                            for b in 0..32u8 {
                                let p = [r << 3 | 5, g << 3 | 2, b << 3 | 7];
                                assert_eq!(in_cluster(top, p), set.contains(&bin(r, g, b)), "top {tr},{tg},{tb} colour {r},{g},{b}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_hex_round_trips_and_rejects_garbage() {
        assert_eq!(parse_hex("ffffff"), Some(Rgb([255, 255, 255])));
        assert_eq!(parse_hex("000000"), Some(Rgb([0, 0, 0])));
        assert_eq!(parse_hex("abcdef"), Some(Rgb([0xab, 0xcd, 0xef])));
        assert_eq!(parse_hex("abcde"), None, "wrong length");
        assert_eq!(parse_hex("gggggg"), None, "non-hex");
    }
}
