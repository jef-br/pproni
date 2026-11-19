//! Pass 2: pixel-perfect edge refinement. Call [`refine_edge`] only for edges
//! [`super::gate::gate`] flagged: this is the cost pass 1 exists to avoid.
//! Steps 1-5 (band slice, full-res crop, trimap, colour matting, verdict):
//! See docs/specs/03-shot-classifier.md §3.1. Step 6, [`graze_ratio`], rejects
//! a contact where the silhouette runs along the edge rather than off it.

use super::geometry::{Edge, EdgeView, Grid, Rect};
use super::sampling::{sample_background_color, ColorStats};
use super::segmentation::{Instance, Mask};
use image::{Rgb, RgbImage};

/// Everything pass 2 needs for one instance.
pub struct RefinementInput<'a> {
    /// Preprocessor working-resolution image (what the segmentation model runs on)
    pub working_image: &'a RgbImage,
    /// Full res OG image, prior to the preprocessor's resize.
    pub original_image: &'a RgbImage,
    /// Detected instance; `mask`/`bbox` are in `working_image`'s coordinate space.
    pub instance: &'a Instance,
}

/// Tuning knobs, expressed in working-resolution pixels so they stay meaningful regardless of the original image's size.
#[derive(Debug, Clone, Copy)]
pub struct RefineParams {
    /// Width of the border band pass 1 already gated on.
    pub band_px: u32,
    /// Safety margin excluded around the mask when sampling background color.
    pub safety_px: u32,
    /// Extra context grown around the band-intersection region before
    /// cropping, so the samplers have enough pixels to work with.
    pub context_px: u32,
    /// Narrow band of the graze test, compared against `band_px`.
    pub narrow_band_px: u32,
    /// Below this narrow-to-wide coverage ratio, the contact is a graze
    /// and the edge reports as not touching.
    pub graze_ratio_max: f32,
}

impl Default for RefineParams {
    fn default() -> Self {
        RefineParams {
            band_px: 20,
            safety_px: 10,
            context_px: 30,
            narrow_band_px: 5,
            graze_ratio_max: 0.80,
        }
    }
}

/// Result of refining one edge for one instance.
#[derive(Debug, Clone)]
pub struct EdgeRefinement {
    pub edge: Edge,
    /// Pixel-perfect verdict: does the refined subject boundary reach the actual image border on this edge?
    pub touching: bool,
    /// The region refinement worked in, in original-image coordinates.
    /// Not used in `touching`; exposed for callers that export or visualize the refined area.
    pub crop_region: Rect,
    /// Narrow/wide coverage ratio from [`graze_ratio`]; `None` when the mask never reaches the
    /// band or the measurement is degenerate (so no touching decision was made from a ratio).
    /// Below [`RefineParams::graze_ratio_max`] counts as a graze.
    pub coverage: Option<f32>,
}

pub fn refine_edge(input: &RefinementInput, edge: Edge, params: RefineParams) -> EdgeRefinement {
    let mask = &input.instance.mask;
    let scale_x = input.original_image.width() as f32 / input.working_image.width() as f32;
    let scale_y = input.original_image.height() as f32 / input.working_image.height() as f32;
    let scale = scale_x.max(scale_y);

    // Step 1: intersect the mask with the border band, in edge-canonical working-resolution space.
    let mask_view = EdgeView::new(mask, edge);
    let band_h = params.band_px.min(mask_view.height());
    let mut min_cx: Option<u32> = None;
    let mut max_cx = 0u32;
    let mut max_cy = 0u32;
    for cy in 0..band_h {
        for cx in 0..mask_view.width() {
            if mask_view.get(cx, cy) {
                min_cx = Some(min_cx.map_or(cx, |m| m.min(cx)));
                max_cx = max_cx.max(cx);
                max_cy = max_cy.max(cy);
            }
        }
    }

    let Some(min_cx) = min_cx else {
        // Pass 1 flagged the mask's bbox as near this edge, but the mask itself doesn't actually reach the band - nothing to refine.
        return EdgeRefinement {
            edge,
            touching: false,
            crop_region: Rect { x: 0, y: 0, w: 0, h: 0 },
            coverage: None,
        };
    };

    let roi_canonical = Rect::from_bounds(min_cx, 0, max_cx, max_cy);

    // Step 2a: map back to working-image coordinates.
    let (ox0, oy0) = mask_view.to_original(roi_canonical.x, roi_canonical.y);
    let (ox1, oy1) = mask_view.to_original(
        roi_canonical.x + roi_canonical.w.saturating_sub(1),
        roi_canonical.y + roi_canonical.h.saturating_sub(1),
    );
    let working_roi = Rect::from_bounds(ox0.min(ox1), oy0.min(oy1), ox0.max(ox1), oy0.max(oy1))
        .expanded(params.context_px, input.working_image.width(), input.working_image.height());

    // Step 2b: rescale to full original resolution and crop there.
    let original_roi = working_roi.scaled(scale_x, scale_y).expanded(
        0,
        input.original_image.width(),
        input.original_image.height(),
    );
    let crop = crop_image(input.original_image, original_roi);

    // Step 3: trimap from the working-resolution mask, upsampled into crop coordinates.
    let upsampled_mask = mask.upsample_region_nearest(working_roi, crop.width(), crop.height());
    let ring_px = ((params.safety_px as f32) * scale).round().max(2.0) as u32;
    let trimap = Trimap::from_mask(&upsampled_mask, ring_px);

    // Step 4: sample fg/bg color, then classify the unknown ring.
    let band_px_full = ((params.band_px as f32) * scale).round().max(1.0) as u32;
    let safety_px_full = ((params.safety_px as f32) * scale).round().max(1.0) as u32;
    let bg = sample_background_color(&crop, &upsampled_mask, edge, band_px_full, safety_px_full);
    let fg = sample_foreground_color(&crop, &trimap);

    let refined = match (fg, bg) {
        (Some(fg), Some(bg)) => trimap.resolve(&crop, fg, bg),
        // Not enough samples to refine confidently - fall back to the
        // upsampled mask rather than guessing at a verdict.
        _ => upsampled_mask,
    };

    // Step 5: the refined mask says whether the boundary reaches the
    // actual border (canonical row/col 0) anywhere along this edge...
    let refined_view = EdgeView::new(&refined, edge);
    let reaches_border = (0..refined_view.width()).any(|cx| refined_view.get(cx, 0));

    // ...and step 6 asks whether that contact is a real bleed-off or only a graze.
    // See docs/architecture/modules/core-shot-classifier-refine.md.
    let narrow_px_full = ((params.narrow_band_px as f32) * scale).round().max(1.0) as u32;
    let coverage = graze_ratio(&refined, edge, band_px_full, narrow_px_full);
    let is_graze = coverage.is_some_and(|r| r < params.graze_ratio_max);
    let touching = reaches_border && !is_graze;

    EdgeRefinement { edge, touching, crop_region: original_roi, coverage }
}

/// Narrow-band over wide-band mean coverage: about 0.61 for a graze, 0.93 and up for a bleed-off.
/// Takes the *refined* mask in crop coordinates, bands already at full resolution.
/// See docs/architecture/modules/core-shot-classifier-refine.md.
fn graze_ratio(mask: &Mask, edge: Edge, wide_px: u32, narrow_px: u32) -> Option<f32> {
    let view = EdgeView::new(mask, edge);
    let wide = wide_px.min(view.height());
    let narrow = narrow_px.min(wide);
    // Degenerate bands can't express a ratio; the caller treats `None` as "not a graze" so the
    // border contact stands on its own.
    if narrow == 0 || narrow == wide {
        return None;
    }

    let per_row: Vec<f32> = (0..wide)
        .map(|cy| (0..view.width()).filter(|&cx| view.get(cx, cy)).count() as f32)
        .collect();
    let mean = |rows: &[f32]| rows.iter().sum::<f32>() / rows.len() as f32;

    let wide_mean = mean(&per_row);
    if wide_mean <= 0.0 {
        return None;
    }
    Some(mean(&per_row[..narrow as usize]) / wide_mean)
}

fn crop_image(image: &RgbImage, rect: Rect) -> RgbImage {
    image::imageops::crop_imm(image, rect.x, rect.y, rect.w.max(1), rect.h.max(1)).to_image()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrimapLabel {
    Foreground,
    Background,
    Unknown,
}

struct Trimap {
    width: u32,
    height: u32,
    labels: Vec<TrimapLabel>,
}

impl Trimap {
    /// `ring_px`: width of the "unknown" ring straddling the mask boundary, in crop pixels; it covers
    /// where the upsampled working mask is inaccurate. Found by walking runs, one pass per axis.
    /// See docs/architecture/modules/core-shot-classifier-refine.md.
    fn from_mask(mask: &Mask, ring_px: u32) -> Trimap {
        let (w, h) = (mask.width, mask.height);
        let at = |x: u32, y: u32| mask.data[(y * w + x) as usize] != 0;

        let mut labels = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                labels.push(if at(x, y) {
                    TrimapLabel::Foreground
                } else {
                    TrimapLabel::Background
                });
            }
        }

        // `ring_px == 0` leaves the two-label mask as it stands: with no
        // ring there is nothing for the matting step to re-decide.
        if ring_px > 0 && w > 0 && h > 0 {
            let mut mark = |x: u32, y: u32| labels[(y * w + x) as usize] = TrimapLabel::Unknown;

            for y in 0..h {
                let mut start = 0;
                while start < w {
                    let value = at(start, y);
                    let mut end = start;
                    while end + 1 < w && at(end + 1, y) == value {
                        end += 1;
                    }
                    if start > 0 {
                        for x in start..=(start + ring_px - 1).min(end) {
                            mark(x, y);
                        }
                    }
                    if end + 1 < w {
                        for x in (end + 1).saturating_sub(ring_px).max(start)..=end {
                            mark(x, y);
                        }
                    }
                    start = end + 1;
                }
            }

            for x in 0..w {
                let mut start = 0;
                while start < h {
                    let value = at(x, start);
                    let mut end = start;
                    while end + 1 < h && at(x, end + 1) == value {
                        end += 1;
                    }
                    if start > 0 {
                        for y in start..=(start + ring_px - 1).min(end) {
                            mark(x, y);
                        }
                    }
                    if end + 1 < h {
                        for y in (end + 1).saturating_sub(ring_px).max(start)..=end {
                            mark(x, y);
                        }
                    }
                    start = end + 1;
                }
            }
        }

        Trimap { width: w, height: h, labels }
    }

    fn get(&self, x: u32, y: u32) -> TrimapLabel {
        self.labels[(y * self.width + x) as usize]
    }

    /// Labels each "unknown" pixel by colour distance to the sampled foreground/background colours;
    /// returns the refined binary mask at crop resolution.
    fn resolve(&self, image: &RgbImage, fg: ColorStats, bg: ColorStats) -> Mask {
        let mut data = vec![0u8; (self.width * self.height) as usize];
        for y in 0..self.height {
            for x in 0..self.width {
                let set = match self.get(x, y) {
                    TrimapLabel::Foreground => true,
                    TrimapLabel::Background => false,
                    TrimapLabel::Unknown => {
                        let px = *image.get_pixel(x, y);
                        fg.distance_sq(px) <= bg.distance_sq(px)
                    }
                };
                if set {
                    data[(y * self.width + x) as usize] = 255;
                }
            }
        }
        Mask { width: self.width, height: self.height, data }
    }
}

/// Like `sample_background_color`, on the trimap's `Foreground` pixels; they already exclude the
/// ring, so no extra margin.
fn sample_foreground_color(image: &RgbImage, trimap: &Trimap) -> Option<ColorStats> {
    let mut sum = [0f64; 3];
    let mut count = 0u32;
    for y in 0..trimap.height {
        for x in 0..trimap.width {
            if trimap.get(x, y) == TrimapLabel::Foreground {
                let Rgb(px) = *image.get_pixel(x, y);
                sum[0] += px[0] as f64;
                sum[1] += px[1] as f64;
                sum[2] += px[2] as f64;
                count += 1;
            }
        }
    }
    if count == 0 {
        return None;
    }
    Some(ColorStats {
        mean: [
            (sum[0] / count as f64) as f32,
            (sum[1] / count as f64) as f32,
            (sum[2] / count as f64) as f32,
        ],
        sample_count: count,
    })
}

#[cfg(test)]
mod trimap_tests {
    use super::*;

    /// The definition `from_mask` used to compute directly: within
    /// `ring_px` along a row or column, does any pixel hold the opposite
    /// value? Kept as the reference the fast version is checked against.
    fn brute_force(mask: &Mask, ring_px: u32) -> Vec<TrimapLabel> {
        let (w, h) = (mask.width, mask.height);
        let mut labels = vec![TrimapLabel::Background; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let inside = mask.get(x, y);
                let near_boundary = (0..=ring_px).any(|d| {
                    [
                        x.checked_sub(d).map(|nx| mask.get(nx, y)),
                        Some(mask.get((x + d).min(w - 1), y)),
                        y.checked_sub(d).map(|ny| mask.get(x, ny)),
                        Some(mask.get(x, (y + d).min(h - 1))),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|v| v != inside)
                });
                labels[(y * w + x) as usize] = if near_boundary {
                    TrimapLabel::Unknown
                } else if inside {
                    TrimapLabel::Foreground
                } else {
                    TrimapLabel::Background
                };
            }
        }
        labels
    }

    /// A cheap deterministic bit source - enough to shake out run
    /// boundaries at every offset without pulling in a rng dependency.
    fn noisy_mask(w: u32, h: u32, seed: u64, density: u64) -> Mask {
        let mut state = seed | 1;
        let mut data = Vec::with_capacity((w * h) as usize);
        for _ in 0..w * h {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            data.push(if (state >> 33) % 100 < density { 255 } else { 0 });
        }
        Mask { width: w, height: h, data }
    }

    #[test]
    fn matches_the_brute_force_definition() {
        for (w, h) in [(1u32, 1u32), (1, 9), (9, 1), (13, 11), (32, 24)] {
            for density in [0, 3, 50, 97, 100] {
                for ring in [0u32, 1, 2, 5, 40] {
                    let mask = noisy_mask(w, h, (w * 31 + h) as u64 + density, density as u64);
                    assert_eq!(
                        Trimap::from_mask(&mask, ring).labels,
                        brute_force(&mask, ring),
                        "{w}x{h}, density {density}, ring {ring}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_uniform_mask_has_no_ring() {
        for fill in [0u8, 255] {
            let mask = Mask { width: 16, height: 16, data: vec![fill; 256] };
            let expected =
                if fill == 0 { TrimapLabel::Background } else { TrimapLabel::Foreground };
            assert!(Trimap::from_mask(&mask, 4).labels.iter().all(|&l| l == expected));
        }
    }
}
