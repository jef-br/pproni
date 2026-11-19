//! R2 — crop a square out of the frame: the largest square the image already contains, centred on
//! the *frame*, not on the mask. It adds nothing.
//! Why its three verdicts land here: see docs/architecture/modules/core-processor-routes.md.

use super::super::super::preprocessor::Prepared;
use crate::core::shot_classifier::route::Shot;
use image::RgbImage;

pub fn apply(prep: &Prepared, _shot: Shot<'_>) -> RgbImage {
    crop_centred_square(&prep.original)
}

/// The largest square the image contains, centred. A square image is returned untouched.
fn crop_centred_square(src: &RgbImage) -> RgbImage {
    let (w, h) = (src.width(), src.height());
    let side = w.min(h);
    if w == h {
        return src.clone();
    }
    image::imageops::crop_imm(src, (w - side) / 2, (h - side) / 2, side, side).to_image()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// A gradient along `x`, so a crop's horizontal offset is readable off the pixels.
    fn ramp(w: u32, h: u32) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = (x * 255 / w.max(2).saturating_sub(1)) as u8;
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        img
    }

    #[test]
    fn the_crop_is_the_shorter_side_and_nothing_is_invented() {
        for (w, h) in [(800u32, 1200u32), (1385, 2000), (1500, 1125)] {
            let out = crop_centred_square(&ramp(w, h));
            assert_eq!(out.dimensions(), (w.min(h), w.min(h)), "{w}x{h}");
        }
    }

    #[test]
    fn a_square_image_is_returned_untouched() {
        let img = ramp(300, 300);
        let out = crop_centred_square(&img);
        assert_eq!(out.dimensions(), (300, 300));
        assert_eq!(*out.get_pixel(0, 0), *img.get_pixel(0, 0));
    }

    #[test]
    fn the_crop_is_centred_on_the_frame() {
        // 400x200 -> a 200 square starting at x=100, so the output's first column is the
        // source's column 100 and not its column 0.
        let img = ramp(400, 200);
        let out = crop_centred_square(&img);
        assert_eq!(*out.get_pixel(0, 0), *img.get_pixel(100, 0));
        assert_eq!(*out.get_pixel(199, 0), *img.get_pixel(299, 0));
    }

    #[test]
    fn an_odd_remainder_loses_the_extra_pixel_from_the_far_side() {
        // 101 wide, 100 tall: one column has to go. Integer division drops it on the right, so
        // the crop starts at 0 — a fixed, testable choice rather than an accident.
        let img = ramp(101, 100);
        let out = crop_centred_square(&img);
        assert_eq!(out.dimensions(), (100, 100));
        assert_eq!(*out.get_pixel(0, 0), *img.get_pixel(0, 0));
    }
}
