//! R1 fallback: fill a segment of a line that has no background to stretch from.
//!
//! A line (a row or a column) is *fully bled* on one side when the product, grown by the clearance,
//! runs to the border there: there is nothing to stretch and nothing to invent from. Those segments
//! land here and get a flat BGC-or-white fill; the caller may then replace them with a clean
//! neighbouring line's background. Every line that does have a source stays with
//! [`super::compose`]; this module never touches them.
//!
//! R1 does not use [`fill_segment`]'s `left_ctx`/`right_ctx` arms. A wider inpainter, if ever
//! needed, slots in behind the same signature. See docs/work-state/open-r1-stretch-follow-ups.md.

use image::Rgb;

use super::super::super::super::config;

/// Fill `out`, one unstretchable sub-range of a line, from whichever neighbour context is present.
///
/// * `left_ctx` / `right_ctx` — the end pixels of the stretched runs either side of the sub-range
///   ("left" = lower output index, whatever image edge that is), or `None` at the segment's end.
/// * `bgc` — the global or per-edge background colour, if found; `None` gives flat white (R3's canvas).
///
/// Noise at [`config::R1_INPAINT_NOISE_SIGMA`] (a standard deviation) keeps a flat fill from reading
/// as a dead-flat seam against the JPEG-noisy stretched pixels next to it.
pub fn fill_segment(
    out: &mut [Rgb<u8>],
    left_ctx: Option<Rgb<u8>>,
    right_ctx: Option<Rgb<u8>>,
    bgc: Option<Rgb<u8>>,
) {
    if out.is_empty() {
        return;
    }
    let noise_sigma = config::R1_INPAINT_NOISE_SIGMA;
    match (left_ctx, right_ctx) {
        (Some(a), Some(b)) => lerp_fill(out, a, b, noise_sigma),
        (Some(a), None) => flat_fill(out, a, noise_sigma),
        (None, Some(b)) => flat_fill(out, b, noise_sigma),
        (None, None) => flat_fill(out, bgc.unwrap_or(Rgb([255, 255, 255])), noise_sigma),
    }
}

fn lerp_fill(out: &mut [Rgb<u8>], a: Rgb<u8>, b: Rgb<u8>, sigma: f32) {
    let m = out.len();
    let steps = (m + 1) as f32;
    let mut rng = TinyRng::seeded(a, b, m);
    for (k, dst) in out.iter_mut().enumerate() {
        let t = (k + 1) as f32 / steps;
        let r = lerp_u8(a.0[0], b.0[0], t);
        let g = lerp_u8(a.0[1], b.0[1], t);
        let bl = lerp_u8(a.0[2], b.0[2], t);
        *dst = Rgb([
            jitter(r, sigma, rng.next_i8()),
            jitter(g, sigma, rng.next_i8()),
            jitter(bl, sigma, rng.next_i8()),
        ]);
    }
}

fn flat_fill(out: &mut [Rgb<u8>], c: Rgb<u8>, sigma: f32) {
    let mut rng = TinyRng::seeded(c, c, out.len());
    for dst in out.iter_mut() {
        *dst = Rgb([
            jitter(c.0[0], sigma, rng.next_i8()),
            jitter(c.0[1], sigma, rng.next_i8()),
            jitter(c.0[2], sigma, rng.next_i8()),
        ]);
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

/// Approximate N(0, sigma) jitter from a signed byte roll in [-128, 127].
fn jitter(v: u8, sigma: f32, roll: i8) -> u8 {
    if sigma <= 0.0 {
        return v;
    }
    let s = roll as f32 / 127.0;
    let d = s * sigma;
    (v as f32 + d).round().clamp(0.0, 255.0) as u8
}

/// A tiny deterministic PRNG seeded by the context colours and segment length — so repeated runs
/// of the same inpaint produce the same output, which keeps the output image deterministic.
struct TinyRng(u64);

impl TinyRng {
    fn seeded(a: Rgb<u8>, b: Rgb<u8>, len: usize) -> Self {
        let seed = (a.0[0] as u64)
            ^ ((a.0[1] as u64) << 8)
            ^ ((a.0[2] as u64) << 16)
            ^ ((b.0[0] as u64) << 24)
            ^ ((b.0[1] as u64) << 32)
            ^ ((b.0[2] as u64) << 40)
            ^ (len as u64).wrapping_mul(0x9E3779B97F4A7C15);
        // Avoid an all-zero state, which xorshift fixed-points at zero.
        TinyRng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn next_i8(&mut self) -> i8 {
        (self.next_u64() & 0xFF) as i8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_target_is_a_noop() {
        let mut out: Vec<Rgb<u8>> = Vec::new();
        fill_segment(&mut out, Some(Rgb([10, 10, 10])), None, None);
        assert!(out.is_empty());
    }

    #[test]
    fn both_contexts_present_lerps_between_them() {
        let mut out = vec![Rgb([0, 0, 0]); 3];
        fill_segment(&mut out, Some(Rgb([0, 0, 0])), Some(Rgb([200, 200, 200])), None);
        // Each channel sits in the gap between the two endpoints, allowing for noise.
        for px in &out {
            for c in 0..3 {
                let v = px.0[c] as i32;
                assert!((0..=210).contains(&v), "channel out of range: {v}");
            }
        }
    }

    #[test]
    fn no_context_and_no_bgc_falls_back_to_white() {
        let mut out = vec![Rgb([0, 0, 0]); 4];
        fill_segment(&mut out, None, None, None);
        // All near-white: 255 ± a handful of noise.
        for px in &out {
            for c in 0..3 {
                assert!(px.0[c] >= 250, "expected near-white, got {}", px.0[c]);
            }
        }
    }

    #[test]
    fn no_context_with_bgc_fills_around_bgc() {
        let bgc = Rgb([128, 64, 200]);
        let mut out = vec![Rgb([0, 0, 0]); 4];
        fill_segment(&mut out, None, None, Some(bgc));
        for px in &out {
            // noise sigma is tiny → every channel stays within a few units of bgc.
            for c in 0..3 {
                let diff = (px.0[c] as i32 - bgc.0[c] as i32).abs();
                assert!(diff <= 5, "channel drifted too far: {diff}");
            }
        }
    }
}
