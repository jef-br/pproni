//! Preprocessor: load, then homogenize.
//!
//! **load** — decode, apply EXIF orientation, read the embedded ICC profile.
//!
//! **preprocess (homogenize)** — flatten any alpha onto white, colour-manage into sRGB, and produce
//! the working-resolution copy. Everything downstream sees one shape: opaque sRGB pixels at two
//! resolutions, no alpha channel and no unknown colour space. A third image, `model_input`, is the
//! working copy run through the optimiser (colour balance, then a crop around the product) — it
//! exists only to be shown to the model, and `crop` says where it was cut from `working`.
//!
//! Both resolutions are kept because the shot classifier needs both: the segmentation model runs on
//! `working`, and its pass-2 refinement re-decides the boundary against full-resolution pixels in
//! `original`. Hand it the same image twice and the scale between them collapses to 1.0, which
//! reduces the refinement to a no-op.

pub mod optimizer;

use super::config::WORKING_SIZE;
use super::shot_classifier::geometry::Rect;
use super::trace;
use image::{DynamicImage, ImageReader, RgbImage};
use std::path::Path;
use tintbox::format::decode::TYPE_RGB_8;
use tintbox::profile::{virtuals::build_srgb_profile, ColorSpace, Profile, RenderingIntent};
use tintbox::transform::Transform;

/// A decoded, correctly-oriented image plus whatever colour profile it declared.
pub struct Decoded {
    pub image: DynamicImage,
    pub icc_profile: Option<Vec<u8>>,
}

/// The homogenized image, at the two resolutions the pipeline works in.
pub struct Prepared {
    /// Full resolution, sRGB, EXIF-applied, alpha flattened onto white.
    pub original: RgbImage,
    /// The same image with its longest side reduced to `config::WORKING_SIZE` (never upscaled).
    /// True colour — the optimiser never touches it.
    pub working: RgbImage,
    /// The image BiRefNet sees: `working` after the optimiser (colour balance, then cropped to the
    /// product and its margin).
    pub model_input: RgbImage,
    /// Where `model_input` was cut from, in `working` coordinates.
    pub crop: Rect,
}

/// Load and homogenize in one call — what the pipeline uses.
pub fn prepare(path: &Path) -> Result<Prepared, String> {
    preprocess(load(path)?)
}

/// Decode `path`, apply its EXIF orientation, and keep its ICC profile for the sRGB conversion.
pub fn load(path: &Path) -> Result<Decoded, String> {
    let reader = ImageReader::open(path)
        .map_err(|e| format!("open: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("format: {e}"))?;

    let mut decoder = reader.into_decoder().map_err(|e| format!("decode: {e}"))?;
    let orientation = image::ImageDecoder::orientation(&mut decoder)
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let icc_profile = image::ImageDecoder::icc_profile(&mut decoder).unwrap_or(None);

    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| format!("decode: {e}"))?;
    image.apply_orientation(orientation);

    Ok(Decoded { image, icc_profile })
}

/// Flatten alpha onto white, colour-manage into sRGB, and derive the working-resolution copy and the
/// optimised crop the model sees.
///
/// `Err` when the optimiser finds nothing but backdrop: the image is not processed.
pub fn preprocess(decoded: Decoded) -> Result<Prepared, String> {
    let mut original = flatten_onto_white(&decoded.image);
    trace::image(trace::stage::ORIGINAL, "Preprocess ingress: Decode, EXIF rotation, flatten alpha to white", || original.clone());

    // A source tagged with a non-sRGB RGB profile gets colour-managed into sRGB here, before any
    // downstream pixel math runs on it. An untagged image, or one already tagged sRGB, is left
    // untouched.
    if let Some(icc) = decoded.icc_profile.as_deref() {
        original = convert_to_srgb(original, icc);
        trace::note(|| "ICC profile detected and converted to sRGB".into());
    } else {
        trace::note(|| "No ICC profile in the file: colours left as-is".into());
    }

    // NOTE (plan Part C): `optimizer::trim_letterbox` detects baked-in letterbox/pillarbox bars on
    // textured scenes (mountain/268) and is unit-tested, but it is deliberately NOT wired in here
    // yet. Trimming the bars alone regresses: on 268 it leaves a short frame that R1 then stretches
    // to square, inventing a large smear (and shifting the edge-touch verdict). The trim is only
    // safe once paired with Part C's re-framing (crop when a square crop preserves the mask; else
    // the seam-carver, which needs its perceptual-quality gate tuned on the real stems in `/pair`).
    // Until that path exists, wiring the trim would ship a worse output, so it stays a primitive.

    let working = downscale_to_working(&original);

    // Colour-balance a copy of the working image and crop it to the product, so the segmentation
    // model finds the subject more reliably. `working` and `original` stay true-colour: the exporter
    // composes from `original` and samples BGC/FGC from `working`.
    let (model_input, crop) = optimizer::optimize(&working)?;
    trace::note(|| format!("Optimiser {crop:?}\nworking copy: {}x{}", working.width(), working.height()));
    Ok(Prepared { original, working, model_input, crop })
}

// Composite onto white so transparent pixels become white pixels. The alpha channel is not carried
// forward — nothing downstream reads it.
fn flatten_onto_white(img: &DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut rgb = RgbImage::new(rgba.width(), rgba.height());
    for (dst, src) in rgb.pixels_mut().zip(rgba.pixels()) {
        let a = src[3] as f32 / 255.0;
        for c in 0..3 {
            dst[c] = (src[c] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
        }
    }
    rgb
}

// Reduce the longest side to WORKING_SIZE. An image already at or below that size is copied as-is —
// upscaling would invent detail the segmentation model would then read as real.
fn downscale_to_working(original: &RgbImage) -> RgbImage {
    let (w, h) = (original.width(), original.height());
    let longest = w.max(h);
    if longest <= WORKING_SIZE {
        return original.clone();
    }
    let scale = WORKING_SIZE as f64 / longest as f64;
    let target_w = ((w as f64 * scale).round() as u32).max(1);
    let target_h = ((h as f64 * scale).round() as u32).max(1);
    image::imageops::resize(
        original,
        target_w,
        target_h,
        image::imageops::FilterType::Triangle,
    )
}

// Colour-manage `rgb` from its embedded ICC profile into sRGB via tintbox (a pure-Rust,
// bit-identical-to-lcms2 CMM). Anything that doesn't parse as an RGB profile, or that tintbox
// can't build a transform for, leaves `rgb` untouched — same fallback as no profile at all.
fn convert_to_srgb(rgb: RgbImage, icc: &[u8]) -> RgbImage {
    let (w, h) = (rgb.width(), rgb.height());
    let Ok(source) = Profile::open(icc) else {
        return rgb;
    };
    if source.header().color_space != ColorSpace::Rgb {
        return rgb;
    }
    let Ok(target) = Profile::from_writable(&build_srgb_profile()) else {
        return rgb;
    };
    let Ok(xform) = Transform::new_simple_with_formats(
        &source,
        &target,
        RenderingIntent::RelativeColorimetric,
        true, // black-point compensation
        TYPE_RGB_8,
        TYPE_RGB_8,
    ) else {
        return rgb;
    };

    let n_pixels = w as usize * h as usize;
    let src = rgb.into_raw();
    let mut out = vec![0u8; src.len()];
    xform.do_transform(&src, &mut out, n_pixels);
    RgbImage::from_raw(w, h, out).expect("transform preserves buffer size")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb};

    /// The optimiser feeds the model only: `working` keeps the image's true colours, so BGC/FGC
    /// sampled from it describe the original. The model sees the balanced crop around the product.
    #[test]
    fn working_stays_true_colour_and_only_model_input_is_optimised() {
        let mut img = RgbImage::from_pixel(100, 100, Rgb([200, 200, 200]));
        for y in 35..65 {
            for x in 35..65 {
                img.put_pixel(x, y, Rgb([60, 60, 60]));
            }
        }

        let prep = preprocess(Decoded { image: DynamicImage::ImageRgb8(img.clone()), icc_profile: None }).unwrap();

        assert_eq!(prep.working, img, "working must be the untouched (here: not even resized) image");
        // A 30x30 product: the margin is the 10 px floor on every side.
        assert_eq!(prep.crop, Rect { x: 25, y: 25, w: 50, h: 50 });
        assert_eq!(prep.model_input.dimensions(), (50, 50));
        assert_ne!(
            prep.model_input.get_pixel(0, 0),
            prep.working.get_pixel(25, 25),
            "model_input must be the colour-balanced copy"
        );
    }

    /// A frame with nothing but backdrop is refused, so the batch leaves the file where it is.
    #[test]
    fn a_blank_image_is_an_error() {
        let img = RgbImage::from_pixel(64, 64, Rgb([240, 240, 240]));
        assert!(preprocess(Decoded { image: DynamicImage::ImageRgb8(img), icc_profile: None }).is_err());
    }
}
