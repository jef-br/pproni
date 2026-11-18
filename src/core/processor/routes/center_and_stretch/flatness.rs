//! R1 step 0 — is the background flat? One answer for two jobs: on a flat background the stretch
//! starts from the subject's box ([`super::compose`]) and the edge gradients are painted
//! ([`super::gradients`]); on a textured one neither happens.
//!
//! The measure: take the background only (no mask pixel within `R1_FLAT_BLOCK_GAP_PX`), cut it into
//! blocks, and count the share whose pixels vary by more than `R1_FLAT_BLOCK_STD` once the slow changes
//! (a Gaussian of `R1_FLAT_SLOW_SIGMA`) are taken out. A share, not an average, so texture over part
//! of the photo is not watered down by flat wall.
//! See docs/architecture/modules/core-processor-routes-center-and-stretch.md.

use image::{imageops, imageops::FilterType, RgbImage};

use crate::core::config;
use crate::core::shot_classifier::Mask;

/// Whether the background is flat: at most `R1_FLAT_ROUGH_SHARE_MAX` of its blocks are rough. With no
/// block to measure there is no proof of flatness, so no.
pub fn is_flat(original: &RgbImage, mask: &Mask) -> bool {
    rough_share(original, mask).is_some_and(|share| share <= config::R1_FLAT_ROUGH_SHARE_MAX)
}

/// The share of background blocks that are rough, measured at the mask's own size, or `None` when no
/// block lies clear of the mask.
pub fn rough_share(original: &RgbImage, mask: &Mask) -> Option<f32> {
    let (w, h) = (mask.width as usize, mask.height as usize);
    let bs = config::R1_FLAT_BLOCK_PX as usize;
    if bs == 0 || w < bs || h < bs || original.width() == 0 || original.height() == 0 {
        return None;
    }
    let luma = luma_at(original, mask.width, mask.height);
    let slow = gaussian(&luma, w, h, config::R1_FLAT_SLOW_SIGMA);
    let mask_count = integral_of(mask);

    let gap = config::R1_FLAT_BLOCK_GAP_PX as usize;
    let (mut clean, mut rough) = (0u32, 0u32);
    for by in 0..h / bs {
        for bx in 0..w / bs {
            let (x0, y0) = (bx * bs, by * bs);
            let (x1, y1) = (x0 + bs, y0 + bs);
            let near = (x0.saturating_sub(gap), y0.saturating_sub(gap), (x1 + gap).min(w), (y1 + gap).min(h));
            if rect_sum(&mask_count, w, near) != 0 {
                continue;
            }
            clean += 1;
            let (mut sum, mut sum_sq) = (0f64, 0f64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let d = (luma[y * w + x] - slow[y * w + x]) as f64;
                    sum += d;
                    sum_sq += d * d;
                }
            }
            let n = (bs * bs) as f64;
            let var = (sum_sq / n - (sum / n) * (sum / n)).max(0.0);
            if var.sqrt() > config::R1_FLAT_BLOCK_STD as f64 {
                rough += 1;
            }
        }
    }
    (clean > 0).then(|| rough as f32 / clean as f32)
}

/// Luma of `img` resized to `w x h` (not resized when it already is that size).
fn luma_at(img: &RgbImage, w: u32, h: u32) -> Vec<f32> {
    let resized;
    let img = if img.dimensions() == (w, h) {
        img
    } else {
        resized = imageops::resize(img, w, h, FilterType::Triangle);
        &resized
    };
    img.pixels().map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect()
}

/// Separable Gaussian blur, the edge pixel repeated past the border.
fn gaussian(src: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    let radius = (3.0 * sigma).ceil() as i64;
    let mut kernel: Vec<f32> = (-radius..=radius).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let total: f32 = kernel.iter().sum();
    kernel.iter_mut().for_each(|k| *k /= total);

    let mut across = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (i, k) in kernel.iter().enumerate() {
                let xx = (x as i64 + i as i64 - radius).clamp(0, w as i64 - 1) as usize;
                acc += k * src[y * w + xx];
            }
            across[y * w + x] = acc;
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (i, k) in kernel.iter().enumerate() {
                let yy = (y as i64 + i as i64 - radius).clamp(0, h as i64 - 1) as usize;
                acc += k * across[yy * w + x];
            }
            out[y * w + x] = acc;
        }
    }
    out
}

/// Summed-area table of the mask: entry `(y + 1) * (w + 1) + (x + 1)` counts the set pixels in `[0, x] x [0, y]`.
fn integral_of(mask: &Mask) -> Vec<u32> {
    let (w, h) = (mask.width as usize, mask.height as usize);
    let mut table = vec![0u32; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0u32;
        for x in 0..w {
            row += (mask.data[y * w + x] != 0) as u32;
            table[(y + 1) * (w + 1) + x + 1] = table[y * (w + 1) + x + 1] + row;
        }
    }
    table
}

/// Set pixels inside `(x0, y0, x1, y1)`, the end exclusive.
fn rect_sum(table: &[u32], w: usize, (x0, y0, x1, y1): (usize, usize, usize, usize)) -> u32 {
    let at = |x: usize, y: usize| table[y * (w + 1) + x];
    at(x1, y1) + at(x0, y0) - at(x1, y0) - at(x0, y1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::shot_classifier::geometry::Rect;
    use image::Rgb;

    /// A `w x h` picture of `bg(x, y)` with a dark `subject`, and its exact mask.
    fn scene(w: u32, h: u32, subject: Rect, bg: impl Fn(u32, u32) -> u8) -> (RgbImage, Mask) {
        let mut img = RgbImage::from_fn(w, h, |x, y| Rgb([bg(x, y); 3]));
        let mut mask = Mask { width: w, height: h, data: vec![0u8; (w * h) as usize] };
        for y in subject.y..subject.y + subject.h {
            for x in subject.x..subject.x + subject.w {
                img.put_pixel(x, y, Rgb([10; 3]));
                mask.data[(y * w + x) as usize] = 255;
            }
        }
        (img, mask)
    }

    const SUBJECT: Rect = Rect { x: 400, y: 100, w: 200, h: 600 };

    fn noise(x: u32, y: u32) -> u8 {
        (x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503)).wrapping_mul(2246822519) as u8
    }

    #[test]
    fn a_plain_background_is_flat_and_so_is_a_slow_ramp() {
        let (img, mask) = scene(1000, 800, SUBJECT, |_, _| 200);
        assert_eq!(rough_share(&img, &mask), Some(0.0));
        assert!(is_flat(&img, &mask));
        let (img, mask) = scene(1000, 800, SUBJECT, |x, _| (60 + x / 8) as u8);
        assert!(is_flat(&img, &mask), "a slow ramp is not texture");
    }

    #[test]
    fn a_noisy_background_is_not_flat() {
        let (img, mask) = scene(1000, 800, SUBJECT, noise);
        assert!(rough_share(&img, &mask).expect("blocks") > 0.9);
        assert!(!is_flat(&img, &mask));
    }

    #[test]
    fn texture_over_a_part_of_the_photo_counts_by_its_share_of_the_background() {
        // The lower third is noise, the rest plain: far above the limit, though the average is low.
        let (img, mask) = scene(1000, 800, SUBJECT, |x, y| if y >= 560 { noise(x, y) } else { 200 });
        let share = rough_share(&img, &mask).expect("blocks");
        assert!(share > config::R1_FLAT_ROUGH_SHARE_MAX, "share {share}");
        assert!(!is_flat(&img, &mask));
    }

    #[test]
    fn the_products_own_edge_is_not_counted() {
        // A plain background with a hard-edged dark product: only the product's edge is rough, and
        // that lies inside the gap, so it must not count.
        let (img, mask) = scene(1000, 800, SUBJECT, |_, _| 240);
        assert_eq!(rough_share(&img, &mask), Some(0.0));
    }

    #[test]
    fn with_no_block_clear_of_the_mask_there_is_no_proof_of_flatness() {
        let (img, mask) = scene(100, 100, Rect { x: 10, y: 10, w: 80, h: 80 }, |_, _| 200);
        assert_eq!(rough_share(&img, &mask), None);
        assert!(!is_flat(&img, &mask));
    }

    #[test]
    fn a_mask_smaller_than_the_original_is_measured_at_the_masks_size() {
        let (img, _) = scene(1000, 800, SUBJECT, noise);
        let mut small = Mask { width: 500, height: 400, data: vec![0u8; 500 * 400] };
        for y in 50..350u32 {
            for x in 200..300u32 {
                small.data[(y * 500 + x) as usize] = 255;
            }
        }
        assert!(!is_flat(&img, &small));
    }
}
