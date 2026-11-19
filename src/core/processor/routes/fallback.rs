//! R3 — no verdict (EIX not set). Fits the *whole* image, uncropped and unscaled, centred on a
//! square canvas sized to the original's longest side, the remainder filled white.
//! See docs/architecture/modules/core-processor-routes.md.

use super::super::super::preprocessor::Prepared;
use image::{Rgb, RgbImage};

const CANVAS_FILL: Rgb<u8> = Rgb([255, 255, 255]);

pub fn apply(prep: &Prepared) -> RgbImage {
    let src = &prep.original;
    let (w, h) = (src.width(), src.height());
    let side = w.max(h);
    if w == h {
        return src.clone();
    }

    let mut canvas = RgbImage::from_pixel(side, side, CANVAS_FILL);
    image::imageops::replace(&mut canvas, src, ((side - w) / 2) as i64, ((side - h) / 2) as i64);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::preprocessor::{Framing, Trims};
    use crate::core::shot_classifier::geometry::Rect;

    fn prepared(w: u32, h: u32) -> Prepared {
        let img = RgbImage::from_pixel(w, h, Rgb([10, 20, 30]));
        Prepared { original: img.clone(), working: img.clone(), model_input: img.clone(), crop: Rect { x: 0, y: 0, w: img.width(), h: img.height() }, trims: Trims::NONE, framing: Framing::Free, letterbox: None }
    }

    #[test]
    fn output_is_square_at_the_longest_side() {
        for (w, h) in [(400u32, 300u32), (300, 400), (250, 250)] {
            let out = apply(&prepared(w, h));
            assert_eq!(out.width(), out.height(), "{w}x{h}: not square");
            assert_eq!(out.width(), w.max(h), "{w}x{h}: wrong side");
        }
    }

    #[test]
    fn image_is_centred_and_the_remainder_is_white() {
        // 400x300 → 400x400, so 50 rows of fill above and below.
        let out = apply(&prepared(400, 300));
        assert_eq!(*out.get_pixel(200, 10), CANVAS_FILL, "top band should be fill");
        assert_eq!(*out.get_pixel(200, 390), CANVAS_FILL, "bottom band should be fill");
        assert_eq!(*out.get_pixel(200, 200), Rgb([10, 20, 30]), "centre should be the image");
    }

    #[test]
    fn a_square_image_is_untouched() {
        let out = apply(&prepared(256, 256));
        assert_eq!(out.dimensions(), (256, 256));
        assert_eq!(*out.get_pixel(0, 0), Rgb([10, 20, 30]));
    }
}
