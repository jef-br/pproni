# 1. Pipeline: why two resolutions

[Up: table of contents](TOC.md)

`Prepared` carries the image three times: `original` at full resolution, `working` reduced to
`config::WORKING_SIZE` on its longest side (both true colour), and `model_input`, the image BiRefNet
sees: `working` after the optimiser. `Prepared::crop` says where `model_input` was cut from `working`.

The optimiser is `preprocessor::optimizer::optimize_both` (`optimize` wraps it and keeps only the model input and the crop). It does two things in order, then a control:

1. **Colour balance.** A Levels move worked out once per image: per channel it finds `lo` (the highest
   value with at most 0.68% of the pixels at or below it) and `hi` (the 99.32 percentile), shares the
   lowest `lo` and the highest `hi` across the three channels, derives a gamma
   `1 + lo/(255-lo) - (255-hi)/hi` kept between 0.01 and 9.99, and applies one 256-entry table to
   every channel. The one rounding happens when the table is built.
2. **Trim**, on the balanced image. The 16 border pixels (4 per edge, at 0/25/50/75% of the edge,
   going round clockwise) are first filtered for outliers: a sample is dropped when its L1 distance to
   the per-channel median exceeds `TRIM_OUTLIER_FLOOR + TRIM_OUTLIER_K·MAD`, with the `TRIM_MIN_KEEP`
   closest always kept. The kept samples give the backdrop colour range: per channel, the lowest and
   highest sample. A pixel inside the range on all three channels is backdrop. Gaps are closed with a 2x2 window
   (`TRIM_CLOSE_K`): a pixel only counts as product if it sits in a 2x2 block of non-backdrop pixels.
   The tightest box around the product is squared to its largest side and grown on every side by the
   larger of 10 px and 10% of that side (`TRIM_MARGIN_MIN_PX`, `TRIM_MARGIN_SHARE`). The balanced image
   is cropped to that square, centred on the product and slid to stay inside the frame (the margin
   shrinks if the frame is too narrow; if the product is longer than the frame's short side no square
   fits, the margin drops to 10 px and the crop is cut at the frame). No product left: `Err`, the image is not processed,
   stays in the input folder and counts as failed.
3. **Trim 2, the control** (`recheck_box`): the same trim run again on trim 1's box. It counts as moved when
   a side differs by more than `TRIM_RECHECK_TOLERANCE_PX` (2). The model never sees it; `core::framing`
   reads both boxes against the mask.

`core::classify` shows the model only that crop, wrapped in `shot_classifier::CroppedModel`, which
pastes the returned mask into a blank working-size canvas at the crop position (full-frame box). The
gate, refinement and routes therefore see a full-frame mask, as if the model had been shown the whole
frame.

The segmentation model runs on the optimiser's crop of `working`. Its pass-2 refinement then re-decides the subject boundary
against full-resolution pixels in `original`, because the mask it produced is upsampled and not
pixel-accurate at the edge. Hand the classifier the same image twice and the scale between them
collapses to 1.0, the refinement ring shrinks to its floor, and pass 2 stops doing anything.
