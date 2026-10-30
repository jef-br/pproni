//! R2 — crop a square out of the frame.
//!
//! The route for images where framing *around* the subject is the wrong move, either because
//! there is no usable subject to frame around or because the frame cannot grow to hold one. It
//! adds nothing: it takes the largest square the image already contains, centred.
//!
//! Three verdicts arrive here (see `shot_classifier::route::decide`):
//!
//! * **The mask covers too little of the frame to be a subject.** A close-up where the segmenter
//!   latched onto one high-contrast detail. Cropping to that detail magnifies a speck; padding
//!   around the whole image adds white to a picture that is already all product. Taking a square
//!   out of it is the only move that neither invents nor magnifies.
//! * **Left and right both bled, and nothing else.** The subject spans the full width, so the
//!   square can be at most that wide — and the subject is taller than that. R1 would have to
//!   stretch a blocked axis to fit, which would paint background over a subject running off-frame.
//!   Cropping into the height is the answer instead.
//! * **All four edges bled.** The subject's box *is* the frame. Nowhere to put a margin, no
//!   background band anywhere to stretch from.
//!
//! Centred on the *frame*, not on the mask. Two of the three cases arrive here precisely because
//! the mask is not to be trusted, and the third has a subject spanning the whole width — so the
//! frame is the more reliable anchor in all three.
//!
//! `docs/pproni-spec.md` §6's CoG/MSR square is the refinement this leaves room for: anchor the crop
//! on the saliency-weighted centre rather than the frame's middle. Nothing in the 37-image set
//! needs it yet.

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
