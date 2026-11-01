//! Output size envelope, [MIN_SIZE, MAX_SIZE] on the longest side. The exporter starts here, so
//! this is the one place it is enforced. See docs/architecture/modules/core-exporter.md.

use super::super::config::{MAX_SIZE, MIN_SIZE};
use image::{imageops, RgbImage};

/// Scale `img` uniformly (a square stays square) so its longest side lands inside the envelope.
/// One already inside is returned as is: no generation loss.
pub fn to_envelope(img: &RgbImage) -> RgbImage {
    let (w, h) = (img.width(), img.height());
    let longest = w.max(h);

    let scale = if longest > MAX_SIZE {
        MAX_SIZE as f64 / longest as f64
    } else if longest < MIN_SIZE {
        MIN_SIZE as f64 / longest as f64
    } else {
        return img.clone();
    };

    let tw = ((w as f64 * scale).round() as u32).max(1);
    let th = ((h as f64 * scale).round() as u32).max(1);
    imageops::resize(img, tw, th, imageops::FilterType::Lanczos3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn img(w: u32, h: u32) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb([7, 8, 9]))
    }

    #[test]
    fn oversized_is_scaled_down_to_max() {
        let out = to_envelope(&img(5314, 3000));
        assert_eq!(out.width().max(out.height()), MAX_SIZE);
    }

    #[test]
    fn undersized_is_scaled_up_to_min() {
        let out = to_envelope(&img(300, 200));
        assert_eq!(out.width().max(out.height()), MIN_SIZE);
    }

    #[test]
    fn in_range_is_untouched() {
        let src = img(1000, 1500);
        let out = to_envelope(&src);
        assert_eq!(out.dimensions(), src.dimensions());
    }

    #[test]
    fn boundaries_are_inclusive() {
        for side in [MIN_SIZE, MAX_SIZE] {
            let out = to_envelope(&img(side, side));
            assert_eq!(out.dimensions(), (side, side), "side {side} should be left alone");
        }
    }

    #[test]
    fn aspect_ratio_is_preserved() {
        for (w, h) in [(5314u32, 3000u32), (300, 200), (400, 3000)] {
            let out = to_envelope(&img(w, h));
            let src_ar = w as f64 / h as f64;
            let out_ar = out.width() as f64 / out.height() as f64;
            assert!(
                (src_ar - out_ar).abs() < 0.01,
                "{w}x{h}: aspect drifted {src_ar:.4} -> {out_ar:.4}"
            );
        }
    }

    #[test]
    fn a_square_stays_square() {
        for side in [5314u32, 300, 1200] {
            let out = to_envelope(&img(side, side));
            assert_eq!(out.width(), out.height(), "side {side} lost squareness");
        }
    }
}
