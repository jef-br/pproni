//! Segmentation types. [`SegmentationModel`] hides the BiRefNet ONNX call; everything else depends
//! only on the [`Mask`]/[`Instance`] shapes. See docs/architecture/02-modules-shot-classifier.md.

use super::geometry::{Grid, Rect};

/// A binary instance mask in *working-resolution* pixels (longest side `config::WORKING_SIZE`).
/// Its boundary is not pixel-accurate, which is why [`super::refine`] exists. See
/// docs/architecture/modules/core-shot-classifier-segmentation.md.
#[derive(Debug, Clone)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    /// Row-major, one byte per pixel: 0 = background, non-zero = foreground.
    pub data: Vec<u8>,
}

impl Mask {
    pub fn get(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.data[(y * self.width + x) as usize] != 0
    }

    /// The tight box around every foreground pixel, or `None` for an empty mask.
    pub fn bounds(&self) -> Option<Rect> {
        let (mut x0, mut y0) = (u32::MAX, u32::MAX);
        let (mut x1, mut y1) = (0u32, 0u32);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != u32::MAX).then(|| Rect::from_bounds(x0, y0, x1, y1))
    }

    /// This mask laid on a blank `canvas_w x canvas_h` canvas with its top left corner at `(x, y)`.
    /// Everything outside it is background. Anything that would fall off the canvas is dropped.
    pub fn pasted_into(&self, canvas_w: u32, canvas_h: u32, x: u32, y: u32) -> Mask {
        let mut data = vec![0u8; canvas_w as usize * canvas_h as usize];
        let run = self.width.min(canvas_w.saturating_sub(x)) as usize;
        for row in 0..self.height.min(canvas_h.saturating_sub(y)) {
            let from = row as usize * self.width as usize;
            let to = (y + row) as usize * canvas_w as usize + x as usize;
            data[to..to + run].copy_from_slice(&self.data[from..from + run]);
        }
        Mask { width: canvas_w, height: canvas_h, data }
    }

    /// The part of this mask inside `region`, as a mask of its own with its top left corner at `(0, 0)`.
    /// Whatever of `region` falls outside this mask counts as background.
    pub fn cropped(&self, region: Rect) -> Mask {
        let (w, h) = (region.w.max(1), region.h.max(1));
        let mut data = vec![0u8; w as usize * h as usize];
        for y in 0..h {
            for x in 0..w {
                if self.get(region.x + x, region.y + y) {
                    data[(y * w + x) as usize] = 255;
                }
            }
        }
        Mask { width: w, height: h, data }
    }

    /// Nearest-neighbour upsample of `region` (this mask's coordinates) to `target_w x target_h`:
    /// the trimap's starting point in a full-resolution crop, not pixel-accurate on its own.
    pub fn upsample_region_nearest(&self, region: Rect, target_w: u32, target_h: u32) -> Mask {
        let target_w = target_w.max(1);
        let target_h = target_h.max(1);
        let mut data = vec![0u8; (target_w * target_h) as usize];
        for ty in 0..target_h {
            let sy = region.y + (ty as u64 * region.h.max(1) as u64 / target_h as u64) as u32;
            for tx in 0..target_w {
                let sx = region.x + (tx as u64 * region.w.max(1) as u64 / target_w as u64) as u32;
                let sx = sx.min(self.width.saturating_sub(1));
                let sy = sy.min(self.height.saturating_sub(1));
                if self.get(sx, sy) {
                    data[(ty * target_w + tx) as usize] = 255;
                }
            }
        }
        Mask { width: target_w, height: target_h, data }
    }
}

impl Grid for Mask {
    type Item = bool;
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn get(&self, x: u32, y: u32) -> bool {
        Mask::get(self, x, y)
    }
}

/// One detected subject: its working-resolution mask and bounding box.
#[derive(Debug, Clone)]
pub struct Instance {
    pub mask: Mask,
    pub bbox: Rect,
    pub class_id: u32,
    pub confidence: f32,
}

/// Abstraction over the BiRefNet segmentation model, so the gate/refine
/// logic doesn't hard-depend on a specific ONNX runtime binding.
pub trait SegmentationModel {
    /// `image` is the preprocessor's working-resolution RGB output.
    ///
    /// `Err` means no verdict (failed inference, unusable output), not "touches no edge": the image
    /// takes its no-verdict route and the batch goes on, no panic. See
    /// docs/architecture/01-why-classification-is-optional.md.
    fn segment(&self, image: &image::RgbImage) -> Result<Vec<Instance>, String>;
}

/// A model that is shown a crop of the frame but answers for the whole frame: the mask is pasted
/// back at `crop` (working-resolution pixels) on a blank working-size canvas, box = full frame.
/// See docs/architecture/01-why-two-resolutions.md.
pub struct CroppedModel<'a, M: SegmentationModel> {
    model: &'a M,
    /// Width and height of the working image the crop was cut from.
    canvas: (u32, u32),
    crop: Rect,
}

impl<'a, M: SegmentationModel> CroppedModel<'a, M> {
    pub fn new(model: &'a M, canvas: (u32, u32), crop: Rect) -> Self {
        CroppedModel { model, canvas, crop }
    }
}

impl<M: SegmentationModel> SegmentationModel for CroppedModel<'_, M> {
    /// `image` is the crop itself.
    fn segment(&self, image: &image::RgbImage) -> Result<Vec<Instance>, String> {
        let (w, h) = self.canvas;
        let whole = Rect::from_bounds(0, 0, w.saturating_sub(1), h.saturating_sub(1));
        Ok(self
            .model
            .segment(image)?
            .into_iter()
            .map(|found| Instance { mask: found.mask.pasted_into(w, h, self.crop.x, self.crop.y), bbox: whole, ..found })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    /// A stand-in model: marks the whole image it is shown.
    struct AllForeground;

    impl SegmentationModel for AllForeground {
        fn segment(&self, image: &RgbImage) -> Result<Vec<Instance>, String> {
            let (w, h) = image.dimensions();
            let mask = Mask { width: w, height: h, data: vec![255; (w * h) as usize] };
            Ok(vec![Instance { mask, bbox: Rect::from_bounds(0, 0, w - 1, h - 1), class_id: 0, confidence: 1.0 }])
        }
    }

    /// The mask lands at the crop's position; the rest of the frame is background.
    #[test]
    fn the_mask_lands_where_the_crop_was_cut() {
        let crop = Rect { x: 30, y: 20, w: 40, h: 25 };
        let model = CroppedModel::new(&AllForeground, (100, 80), crop);

        let found = model.segment(&RgbImage::new(40, 25)).unwrap().remove(0);

        assert_eq!((found.mask.width, found.mask.height), (100, 80));
        assert_eq!(found.mask.data.iter().filter(|&&v| v > 0).count(), 40 * 25);
        assert!(found.mask.get(30, 20) && found.mask.get(69, 44));
        assert!(!found.mask.get(29, 20) && !found.mask.get(30, 19) && !found.mask.get(70, 44) && !found.mask.get(69, 45));
        assert_eq!(found.bbox, Rect { x: 0, y: 0, w: 100, h: 80 });
    }

    /// A crop that is the whole frame gives the model's own mask back, unchanged.
    #[test]
    fn a_whole_frame_crop_changes_nothing() {
        let model = CroppedModel::new(&AllForeground, (50, 40), Rect { x: 0, y: 0, w: 50, h: 40 });
        let found = model.segment(&RgbImage::new(50, 40)).unwrap().remove(0);
        assert!(found.mask.data.iter().all(|&v| v == 255));
    }

    /// Cropping takes the part inside the region and moves it to the origin.
    #[test]
    fn a_cropped_mask_is_the_part_inside_the_region_at_the_origin() {
        let mask = Mask { width: 10, height: 10, data: vec![0; 100] }
            .pasted_into(10, 10, 0, 0);
        let mut mask = mask;
        for y in 2..5 {
            for x in 3..7 {
                mask.data[y * 10 + x] = 255;
            }
        }
        let c = mask.cropped(Rect { x: 2, y: 1, w: 6, h: 5 });
        assert_eq!((c.width, c.height), (6, 5));
        assert!(c.get(1, 1) && c.get(4, 3), "marked pixels move by the region's corner");
        assert!(!c.get(0, 0) && !c.get(5, 4));
        assert_eq!(c.data.iter().filter(|&&v| v > 0).count(), 12);
    }
}
