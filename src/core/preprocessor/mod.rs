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
//! Both resolutions are kept because the classifier's pass-2 refinement needs full-resolution
//! pixels. See docs/architecture/01-why-two-resolutions.md.

pub mod maria_antonieta;
pub mod optimizer;

use super::config::WORKING_SIZE;
use super::options::Options;
use super::shot_classifier::geometry::Rect;
use super::trace;
use fast_image_resize as fir;
use image::{DynamicImage, ImageReader, RgbImage};
use std::path::Path;
use tintbox::format::decode::TYPE_RGB_8;
use tintbox::profile::{virtuals::build_srgb_profile, ColorSpace, Profile, RenderingIntent};
use tintbox::sig::Signature;
use tintbox::transform::Transform;

/// A decoded, correctly-oriented image plus whatever colour profile it declared.
pub struct Decoded {
    pub image: DynamicImage,
    pub icc_profile: Option<Vec<u8>>,
}

/// The two trims' tight boxes (`working` coordinates). See [`optimizer::optimize_both`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trims {
    /// Trim 1: the tight box around everything that is not backdrop.
    pub first: Rect,
    /// Trim 2: trim 1's box trimmed again, when that moved it by more than the tolerance.
    pub second: Option<Rect>,
}

impl Trims {
    /// No trims: for pictures built by hand in tests.
    pub const NONE: Trims = Trims { first: Rect { x: 0, y: 0, w: 0, h: 0 }, second: None };
}

/// How R1 frames the picture once `core::framing` has read the trims against the mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// R1 plans the square from the subject, as always.
    Free,
    /// The picture is padded on one axis (white bars): the square is cut from the content box (`working`
    /// coordinates) with equal padding either side of it, and nothing is aligned on the subject.
    CentreContent(Rect),
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
    /// The two trims' tight boxes, in `working` coordinates. `core::framing` reads them against the
    /// segmask once the model has run (once: on `model_input`).
    pub trims: Trims,
    /// What `core::framing` decided about the padding or frame round the picture. Set after the
    /// model has run; `Framing::Free` until then, and for most images after it.
    pub framing: Framing,
    /// The part of `original` left after flat letterbox/pillarbox bars are trimmed (`original`
    /// coordinates), or `None` when no bars were found. Only R1 reads it; the model never does.
    pub letterbox: Option<Rect>,
}

/// Load and homogenize in one call — what the pipeline uses.
pub fn prepare(path: &Path, options: &Options) -> Result<Prepared, String> {
    preprocess(load(path)?, options)
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
pub fn preprocess(decoded: Decoded, options: &Options) -> Result<Prepared, String> {
    let mut original = flatten_onto_white(&decoded.image);
    trace::image(trace::stage::ORIGINAL, "Preprocess ingress: Decode, EXIF rotation, flatten alpha to white", || original.clone());

    // A source tagged with a non-sRGB RGB profile gets colour-managed into sRGB here, before any
    // downstream pixel math runs on it. An untagged image, or one already tagged sRGB, is left
    // untouched.
    if let Some(icc) = decoded.icc_profile.as_deref() {
        if is_srgb_profile(icc) {
            trace::note(|| "ICC profile is already sRGB: colours left as-is".into());
        } else {
            original = convert_to_srgb(original, icc);
            trace::note(|| "ICC profile detected and converted to sRGB".into());
        }
    } else {
        trace::note(|| "No ICC profile in the file: colours left as-is".into());
    }

    // Plan Part C: flat letterbox/pillarbox bars on a textured scene (mountain/268) are found here,
    // on the full-resolution image, before the mask exists. `original` itself is left whole; R1
    // reads `letterbox` and re-frames the trimmed part (`extend::reframe_to_square`).
    let report = optimizer::detect_letterbox(&original);
    let letterbox = report.keep;
    if trace::active() {
        // Stage 02. With no bars found the texture is never measured, so show the proxy on the
        // whole frame (trace only) to let it be judged on any image.
        let (proxy, texture, region) = match report.texture {
            Some(t) => (report.proxy, t, report.bars),
            None => {
                let (p, t) = optimizer::background_texture(&original);
                (p, t, None)
            }
        };
        trace::note(|| format!("letterbox edges [T, R, B, L] = {:?}", report.kinds));
        trace::note(|| {
            let area = region.map_or(original.width() as u64 * original.height() as u64, |r| r.w as u64 * r.h as u64);
            let share = proxy.map_or(0.0, |p| 100.0 * (p.w as u64 * p.h as u64) as f64 / area.max(1) as f64);
            format!(
                "letterbox bars {:?}, keep {:?}; product proxy {:?} covers {share:.1}% of the {} measured, texture outside it {texture:.2} (textured above {})",
                report.bars,
                report.keep,
                proxy,
                if report.texture.is_some() { "interior" } else { "frame, not used: no bars" },
                crate::core::config::T_TEXTURE_HIGHPASS,
            )
        });
        trace::image(
            trace::stage::LETTERBOX,
            "Letterbox check: edge bars green = backdrop, orange = flat other, magenta = varied; dimmed = trimmed bars; cyan = product proxy",
            || trace::letterbox_view(&original, report.kinds, report.bars, proxy),
        );
    }

    let working = downscale_to_working(&original);

    // Colour-balance a copy of the working image and crop it to the product, so the segmentation
    // model finds the subject more reliably. `working` and `original` stay true-colour: the exporter
    // composes from `original` and samples BGC/FGC from `working`.
    let both = optimizer::optimize_both(&working)?;
    let (model_input, crop) = (both.model_input, both.crop);
    let trims = Trims { first: both.first, second: both.second };
    trace::note(|| format!("Optimiser {crop:?}\nworking copy: {}x{}", working.width(), working.height()));

    let _ = options; // Preserved for callers; headcutting is driven post-classify in `core::process_file`.
    Ok(Prepared { original, working, model_input, crop, trims, framing: Framing::Free, letterbox })
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
    // Triangle filter (called "Bilinear" by this crate). The inputs cannot be refused: the buffer is an
    // `RgbImage`'s own, the pixel type matches it, and both sizes are at least 1.
    let source = fir::images::ImageRef::new(w, h, original.as_raw(), fir::PixelType::U8x3)
        .expect("an RgbImage's buffer matches its size");
    let mut target = fir::images::Image::new(target_w, target_h, fir::PixelType::U8x3);
    let options = fir::ResizeOptions::new().resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::Bilinear));
    fir::Resizer::new()
        .resize(&source, &mut target, &options)
        .expect("same pixel type, sizes of at least 1");
    RgbImage::from_raw(target_w, target_h, target.into_vec()).expect("the target buffer holds target_w x target_h pixels")
}

// Whether `icc` is an RGB profile that names itself sRGB. Converting such an image into sRGB changes
// no pixel, and at full resolution it is the slowest step before the model, so it is skipped. Decided
// by the name in the profile's `desc` tag, which comes in two encodings: ASCII, or UTF-16 big-endian.
fn is_srgb_profile(icc: &[u8]) -> bool {
    let Ok(profile) = Profile::open(icc) else {
        return false;
    };
    if profile.header().color_space != ColorSpace::Rgb {
        return false;
    }
    let Ok(desc) = profile.read_tag_raw(Signature::from_bytes(*b"desc")) else {
        return false;
    };
    let ascii = desc.windows(4).any(|w| w.eq_ignore_ascii_case(b"srgb"));
    let utf16 = desc
        .windows(8)
        .any(|w| w[0] == 0 && w[2] == 0 && w[4] == 0 && w[6] == 0 && [w[1], w[3], w[5], w[7]].eq_ignore_ascii_case(b"srgb"));
    ascii || utf16
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

        let prep = preprocess(Decoded { image: DynamicImage::ImageRgb8(img.clone()), icc_profile: None }, &Options::default()).unwrap();

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

    /// A profile that names itself sRGB is skipped; one with another name is not; bytes that are not a
    /// profile are not.
    #[test]
    fn a_profile_is_srgb_by_its_name() {
        let srgb = crate::core::exporter::icc::srgb_profile();
        assert!(is_srgb_profile(&srgb));

        let at = srgb.windows(4).position(|w| w == b"sRGB").expect("the profile names itself");
        let mut renamed = srgb.clone();
        renamed[at..at + 4].copy_from_slice(b"Adob");
        assert!(!is_srgb_profile(&renamed));

        assert!(!is_srgb_profile(b"not a profile"));
    }

    /// A frame with nothing but backdrop is refused, so the batch leaves the file where it is.
    #[test]
    fn a_blank_image_is_an_error() {
        let img = RgbImage::from_pixel(64, 64, Rgb([240, 240, 240]));
        assert!(preprocess(Decoded { image: DynamicImage::ImageRgb8(img), icc_profile: None }, &Options::default()).is_err());
    }
}
