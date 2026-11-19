//! R1: the solid-background look, for a flat background that is creased (core 51's studio paper). Where
//! the stretch would smear the creases, everything but the product is painted in the background's
//! predominant colour: the product (the mask plus a hair of margin, feathered) and a soft shadow band
//! under it stay as photographed. The band is the mask moved down, minus the mask, blurred, scaled to full
//! strength beside the mask and faded out towards the product's top, so no halo forms up there.
//! See docs/work-state/2026-11-13-flat-edge-gradients.md.

use image::{imageops, imageops::FilterType, Rgb, RgbImage};

use super::super::super::super::config;
use super::{flatness, patch};
use crate::core::shot_classifier::Mask;

/// Fewer background pixels than this and there is no predominant colour to trust.
const MIN_BACKGROUND_PX: usize = 500;

/// Paint the solid-background look on `canvas`. `working` and `mask` share a size; `origin` is where the
/// canvas' (0, 0) lies on the original picture, `orig` that picture's size. Returns whether anything was
/// painted: `false` when there is no background colour to take.
pub fn apply(canvas: &mut RgbImage, working: &RgbImage, mask: &Mask, origin: (i64, i64), orig: (u32, u32)) -> bool {
    let (mw, mh) = (mask.width as usize, mask.height as usize);
    if mw == 0 || mh == 0 || orig.0 == 0 || orig.1 == 0 {
        return false;
    }
    let Some(colour) = background_colour(working, mask) else {
        return false;
    };
    let side = canvas.width() as usize;
    // The work happens on a grid the size of the mask's pixels: the canvas in mask pixels.
    let (kx, ky) = (orig.0 as f32 / mw as f32, orig.1 as f32 / mh as f32);
    let (gw, gh) = (((side as f32 / kx).ceil() as usize).max(1), ((side as f32 / ky).ceil() as usize).max(1));
    let mut on = vec![false; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let mx = (gx as f32 + 0.5 + origin.0 as f32 / kx).floor() as i64;
            let my = (gy as f32 + 0.5 + origin.1 as f32 / ky).floor() as i64;
            if mx >= 0 && my >= 0 && (mx as usize) < mw && (my as usize) < mh {
                on[gy * gw + gx] = mask.data[my as usize * mw + mx as usize] != 0;
            }
        }
    }
    let keep = keep_map(&on, gw, gh);

    for y in 0..side {
        let fy = ((y as f32 + 0.5) / ky - 0.5).clamp(0.0, gh as f32 - 1.0);
        for x in 0..side {
            let fx = ((x as f32 + 0.5) / kx - 0.5).clamp(0.0, gw as f32 - 1.0);
            let k = sample(&keep, gw, gh, fx, fy);
            if k >= 0.999 {
                continue;
            }
            let p = canvas.get_pixel_mut(x as u32, y as u32);
            *p = Rgb([0, 1, 2].map(|c| (p[c] as f32 * k + colour[c] * (1.0 - k)).round().clamp(0.0, 255.0) as u8));
        }
    }
    true
}

/// How much of every pixel of the grid stays as photographed: 1 on the product and beside it, a soft band
/// under it, 0 elsewhere.
fn keep_map(on: &[bool], w: usize, h: usize) -> Vec<f32> {
    // The product: the mask plus a margin, feathered.
    let near = patch::dilate(on, w, h, config::R1_SOLID_MARGIN_PX);
    let product = flatness::gaussian(&near.iter().map(|&b| b as u8 as f32).collect::<Vec<_>>(), w, h, config::R1_SOLID_FEATHER_SIGMA);

    // The shadow band: the mask moved down, minus the mask.
    let longest = w.max(h) as f32;
    let shift = ((config::R1_SOLID_SHADOW_SHARE * longest).round() as usize).max(1);
    let mut band = vec![0f32; w * h];
    for y in shift..h {
        for x in 0..w {
            band[y * w + x] = (on[(y - shift) * w + x] as u8 as f32 - on[y * w + x] as u8 as f32).max(0.0);
        }
    }
    // Blurred along the line of the shift, then all round.
    let band = blur_down(&band, w, h, shift);
    let mut band = flatness::gaussian(&band, w, h, (config::R1_SOLID_BLUR_SIGMA_SHARE * longest).max(1.0));
    // Full strength beside the mask: the 90th percentile of what lies in a thin ring round it.
    let ring = patch::dilate(on, w, h, 4);
    let mut beside: Vec<f32> = (0..w * h).filter(|&i| ring[i] && !on[i] && band[i] > 0.01).map(|i| band[i]).collect();
    let peak = if beside.is_empty() {
        band.iter().cloned().fold(0.0, f32::max)
    } else {
        let q = (beside.len() * 9 / 10).min(beside.len() - 1);
        beside.select_nth_unstable_by(q, |a, b| a.total_cmp(b));
        beside[q]
    };
    // No halo up at the product's top: the band is off over the first third of the mask's height and
    // on again from two thirds.
    let rows: Vec<usize> = (0..h).filter(|&y| on[y * w..(y + 1) * w].iter().any(|&v| v)).collect();
    let (top, span) = match (rows.first(), rows.last()) {
        (Some(&a), Some(&b)) => (a as f32, (b - a).max(1) as f32),
        _ => (0.0, 1.0),
    };
    let from = top + config::R1_SOLID_RAMP_START * span;
    let to = top + config::R1_SOLID_RAMP_END * span;
    for y in 0..h {
        let ramp = ((y as f32 - from) / (to - from).max(1.0)).clamp(0.0, 1.0);
        for x in 0..w {
            let i = y * w + x;
            band[i] = if peak > 1e-6 && !on[i] { (band[i] / peak).min(1.0) * ramp } else { 0.0 };
        }
    }
    (0..w * h).map(|i| product[i].max(band[i])).collect()
}

/// A box blur of `len` pixels down the columns, centred, the edge pixel repeated past the border.
fn blur_down(src: &[f32], w: usize, h: usize, len: usize) -> Vec<f32> {
    let mut out = vec![0f32; w * h];
    let (before, after) = (len / 2, len - len / 2 - 1);
    for x in 0..w {
        for y in 0..h {
            let mut sum = 0.0;
            for k in 0..len {
                let yy = (y as i64 + k as i64 - before as i64).clamp(0, h as i64 - 1) as usize;
                sum += src[yy * w + x];
            }
            out[y * w + x] = sum / len as f32;
        }
    }
    let _ = after;
    out
}

/// Bilinear read of `map` at the fractional position `(x, y)`.
fn sample(map: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let top = map[y0 * w + x0] * (1.0 - tx) + map[y0 * w + x1] * tx;
    let bottom = map[y1 * w + x0] * (1.0 - tx) + map[y1 * w + x1] * tx;
    top * (1.0 - ty) + bottom * ty
}

/// The background's predominant colour: per channel, the median of the working picture's pixels that are
/// not within `R1_FLAT_BLOCK_GAP_PX` of the mask. `None` with fewer than `MIN_BACKGROUND_PX` of them.
fn background_colour(working: &RgbImage, mask: &Mask) -> Option<[f32; 3]> {
    let (w, h) = (mask.width as usize, mask.height as usize);
    let resized;
    let img = if working.dimensions() == (mask.width, mask.height) {
        working
    } else {
        resized = imageops::resize(working, mask.width, mask.height, FilterType::Triangle);
        &resized
    };
    let on: Vec<bool> = mask.data.iter().map(|&v| v != 0).collect();
    let near = patch::dilate(&on, w, h, config::R1_FLAT_BLOCK_GAP_PX as usize);
    let mut hist = [[0u64; 256]; 3];
    let mut n = 0usize;
    for (i, p) in img.pixels().enumerate() {
        if near[i] {
            continue;
        }
        for c in 0..3 {
            hist[c][p[c] as usize] += 1;
        }
        n += 1;
    }
    if n < MIN_BACKGROUND_PX {
        return None;
    }
    Some([0, 1, 2].map(|c| {
        let mut seen = 0u64;
        for (v, &count) in hist[c].iter().enumerate() {
            seen += count;
            if seen as usize * 2 > n {
                return v as f32;
            }
        }
        255.0
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `side x side` picture of wrinkled paper (columns alternate 230 and 250 in bands of 7), with a dark
    /// product block in the middle, and its mask.
    fn scene(side: u32) -> (RgbImage, Mask) {
        let mut img = RgbImage::from_fn(side, side, |x, _| {
            let v = if (x / 7) % 2 == 0 { 230 } else { 250 };
            Rgb([v, v, v])
        });
        let mut mask = Mask { width: side, height: side, data: vec![0u8; (side * side) as usize] };
        for y in side / 4..side * 3 / 4 {
            for x in side / 3..side * 2 / 3 {
                img.put_pixel(x, y, Rgb([90, 50, 20]));
                mask.data[(y * side + x) as usize] = 255;
            }
        }
        (img, mask)
    }

    #[test]
    fn the_background_becomes_one_colour_and_the_product_stays() {
        let side = 300;
        let (img, mask) = scene(side);
        let mut canvas = img.clone();
        assert!(apply(&mut canvas, &img, &mask, (0, 0), (side, side)));
        // Far from the product: one colour, the median of the paper.
        let far = *canvas.get_pixel(10, 10);
        assert!(far[0] == 230 || far[0] == 250, "a colour of the paper, got {far:?}");
        for (x, y) in [(10u32, 10u32), (40, 250), (280, 20), (150, 5), (290, 290)] {
            assert_eq!(*canvas.get_pixel(x, y), far, "({x}, {y}) is one flat colour");
        }
        // The product itself, to the pixel.
        for y in side / 4 + 3..side * 3 / 4 - 3 {
            for x in side / 3 + 3..side * 2 / 3 - 3 {
                assert_eq!(canvas.get_pixel(x, y), img.get_pixel(x, y), "product changed at ({x}, {y})");
            }
        }
    }

    #[test]
    fn the_paper_just_under_the_product_stays_but_not_the_paper_above_it() {
        let side = 300;
        let (img, mask) = scene(side);
        let mut canvas = img.clone();
        apply(&mut canvas, &img, &mask, (0, 0), (side, side));
        // Two pixels under the product's lower edge (row 225): photographed, so it still shows the bands.
        let under: Vec<u8> = (110..190).map(|x| canvas.get_pixel(x, 228)[0]).collect();
        let under_original: Vec<u8> = (110..190).map(|x| img.get_pixel(x, 228)[0]).collect();
        assert!(
            under.iter().zip(&under_original).all(|(a, b)| a.abs_diff(*b) <= 4),
            "the shadow zone right under the product is kept"
        );
        // Above the product's top edge (row 75) is replaced, no halo.
        let above = canvas.get_pixel(150, 70)[0];
        assert!(above == 230 || above == 250);
        assert_eq!(*canvas.get_pixel(150, 70), *canvas.get_pixel(10, 10));
    }

    #[test]
    fn too_little_background_means_nothing_is_painted() {
        let side = 40;
        let (img, mask) = scene(side);
        let mut canvas = img.clone();
        assert!(!apply(&mut canvas, &img, &mask, (0, 0), (side, side)));
        assert_eq!(canvas, img);
    }
}
