//! Integration tests for the imaging core.
//!
//! The detection/geometry tests that used to live here were retired with the classical detector.
//! What survives is the export invariant, which is independent of routing: every JPEG Pproni writes
//! carries an embedded sRGB ICC profile and decodes back at the size it was written.

use pproni::core::config::WORKING_SIZE;
use pproni::core::exporter::save;
use pproni::core::options::Options;
use pproni::core::preprocessor as load;
use image::{Rgb, Rgba, RgbaImage};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn saved_jpeg_embeds_icc_and_reads_back_at_size() {
    for name in ["packshot_white.jpg", "packshot_tan.jpg", "hardshadow.jpg"] {
        let prep = load::prepare(&fixture(name), &Options::default()).unwrap();
        let tmp = std::env::temp_dir().join(format!("pproni_test_icc_{name}"));
        save::save_jpeg_srgb(&prep.original, &tmp, 95).unwrap();

        let bytes = std::fs::read(&tmp).unwrap();
        assert!(
            has_app2_icc(&bytes),
            "{name}: output JPEG must carry an APP2 ICC profile"
        );

        let back = image::open(&tmp).unwrap();
        assert_eq!(back.width(), prep.original.width(), "{name}: width drifted");
        assert_eq!(back.height(), prep.original.height(), "{name}: height drifted");
        let _ = std::fs::remove_file(&tmp);
    }
}

#[test]
fn working_copy_is_bounded_and_keeps_aspect_ratio() {
    for name in ["packshot_white.jpg", "packshot_tan.jpg", "hardshadow.jpg"] {
        let prep = load::prepare(&fixture(name), &Options::default()).unwrap();
        let (ow, oh) = (prep.original.width(), prep.original.height());
        let (ww, wh) = (prep.working.width(), prep.working.height());

        assert!(
            ww.max(wh) <= WORKING_SIZE,
            "{name}: working longest side {} exceeds {WORKING_SIZE}",
            ww.max(wh)
        );
        // Never upscaled: an image already inside the envelope comes through untouched.
        assert!(ww <= ow && wh <= oh, "{name}: working copy was upscaled");

        let original_ar = ow as f64 / oh as f64;
        let working_ar = ww as f64 / wh as f64;
        assert!(
            (original_ar - working_ar).abs() < 0.01,
            "{name}: aspect ratio drifted {original_ar:.4} -> {working_ar:.4}"
        );
    }
}

#[test]
fn transparent_pixels_become_white() {
    // Transparent frame with an opaque red box. After preprocessing there is no alpha channel and
    // the transparent region reads as white.
    let (w, h) = (120u32, 90u32);
    let mut img = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    for y in 30..60 {
        for x in 40..80 {
            img.put_pixel(x, y, Rgba([200, 30, 30, 255]));
        }
    }
    let tmp = std::env::temp_dir().join("pproni_test_alpha_flatten.png");
    img.save(&tmp).unwrap();

    let prep = load::prepare(&tmp, &Options::default()).unwrap();
    assert_eq!(*prep.original.get_pixel(5, 5), Rgb([255, 255, 255]));
    assert_eq!(*prep.original.get_pixel(50, 40), Rgb([200, 30, 30]));
    let _ = std::fs::remove_file(&tmp);
}

fn has_app2_icc(jpeg: &[u8]) -> bool {
    let mut i = 2usize; // skip SOI
    while i + 4 <= jpeg.len() {
        if jpeg[i] != 0xFF {
            break;
        }
        let marker = jpeg[i + 1];
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        let len = ((jpeg[i + 2] as usize) << 8) | jpeg[i + 3] as usize;
        let seg = &jpeg[i + 4..(i + 2 + len).min(jpeg.len())];
        if marker == 0xE2 && seg.starts_with(b"ICC_PROFILE") {
            return true;
        }
        i += 2 + len;
    }
    false
}
