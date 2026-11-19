//! Background colour sampling for one edge. No edge-specific branching: [`EdgeView`] does it.
//! See docs/architecture/modules/core-shot-classifier-sampling.md.

use super::geometry::{BoolSummedArea, Edge, EdgeView, Grid};
use super::segmentation::Mask;
use image::{Rgb, RgbImage};

#[derive(Debug, Clone, Copy)]
pub struct ColorStats {
    pub mean: [f32; 3],
    pub sample_count: u32,
}

impl ColorStats {
    /// Squared Euclidean distance in RGB space: cheap, enough for foreground/background. See
    /// docs/architecture/modules/core-shot-classifier-sampling.md.
    pub fn distance_sq(&self, px: Rgb<u8>) -> f32 {
        let dr = px[0] as f32 - self.mean[0];
        let dg = px[1] as f32 - self.mean[1];
        let db = px[2] as f32 - self.mean[2];
        dr * dr + dg * dg + db * db
    }
}

/// Thin adapter so `RgbImage` satisfies [`Grid`].
pub(crate) struct ImageGrid<'a>(pub &'a RgbImage);

impl<'a> Grid for ImageGrid<'a> {
    type Item = Rgb<u8>;
    fn width(&self) -> u32 {
        self.0.width()
    }
    fn height(&self) -> u32 {
        self.0.height()
    }
    fn get(&self, x: u32, y: u32) -> Rgb<u8> {
        *self.0.get_pixel(x, y)
    }
}

/// Background colour of a `strip_px`-wide band at `edge`'s border, leaving out the mask plus a
/// `safety_px` margin so no subject-edge pixels leak in. `None` when nothing is left to sample.
pub fn sample_background_color(
    image: &RgbImage,
    mask: &Mask,
    edge: Edge,
    strip_px: u32,
    safety_px: u32,
) -> Option<ColorStats> {
    let img_grid = ImageGrid(image);
    let img_view = EdgeView::new(&img_grid, edge);
    let mask_view = EdgeView::new(mask, edge);
    let strip_h = strip_px.min(img_view.height());

    // The deepest row a query can reach is `strip_h - 1 + safety_px`, so the
    // table stops there rather than covering the whole mask.
    let mask_sums = BoolSummedArea::new(&mask_view, strip_h.saturating_add(safety_px));

    let mut sum = [0f64; 3];
    let mut count = 0u32;
    for cy in 0..strip_h {
        for cx in 0..img_view.width() {
            if mask_is_near(&mask_sums, cx, cy, safety_px) {
                continue;
            }
            let Rgb(px) = img_view.get(cx, cy);
            sum[0] += px[0] as f64;
            sum[1] += px[1] as f64;
            sum[2] += px[2] as f64;
            count += 1;
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

/// True if any mask pixel is within `margin` (Chebyshev distance) of `(cx, cy)`, edge-canonical
/// coordinates. `margin` is in full-resolution pixels, so the window can be huge. See
/// docs/architecture/modules/core-shot-classifier-sampling.md.
fn mask_is_near(mask_sums: &BoolSummedArea, cx: u32, cy: u32, margin: u32) -> bool {
    mask_sums.any_in(
        cx.saturating_sub(margin),
        cy.saturating_sub(margin),
        cx.saturating_add(margin),
        cy.saturating_add(margin),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::geometry::Edge;

    fn noisy_mask(w: u32, h: u32, seed: u64, density: u64) -> Mask {
        let mut state = seed | 1;
        let mut data = Vec::with_capacity((w * h) as usize);
        for _ in 0..w * h {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            data.push(if (state >> 33) % 100 < density { 255 } else { 0 });
        }
        Mask { width: w, height: h, data }
    }

    /// The window scan `mask_is_near` replaced, kept as the reference.
    fn brute_force(mask_view: &EdgeView<Mask>, cx: u32, cy: u32, margin: u32) -> bool {
        let x0 = cx.saturating_sub(margin);
        let y0 = cy.saturating_sub(margin);
        let x1 = (cx + margin).min(mask_view.width().saturating_sub(1));
        let y1 = (cy + margin).min(mask_view.height().saturating_sub(1));
        (y0..=y1).any(|y| (x0..=x1).any(|x| mask_view.get(x, y)))
    }

    #[test]
    fn summed_area_matches_the_window_scan() {
        for (w, h) in [(1u32, 1u32), (7, 3), (3, 7), (21, 17)] {
            for density in [0, 5, 50, 100] {
                let mask = noisy_mask(w, h, (w * 7 + h) as u64 + density, density as u64);
                for edge in Edge::ALL {
                    let view = EdgeView::new(&mask, edge);
                    for strip_h in [1u32, 3, view.height()] {
                        let strip_h = strip_h.min(view.height());
                        for margin in [0u32, 1, 4, 30] {
                            let sums = BoolSummedArea::new(
                                &view,
                                strip_h.saturating_add(margin),
                            );
                            for cy in 0..strip_h {
                                for cx in 0..view.width() {
                                    assert_eq!(
                                        mask_is_near(&sums, cx, cy, margin),
                                        brute_force(&view, cx, cy, margin),
                                        "{w}x{h} {edge:?} density {density} \
                                         strip {strip_h} margin {margin} at ({cx},{cy})"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
