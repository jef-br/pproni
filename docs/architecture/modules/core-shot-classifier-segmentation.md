# core::shot_classifier::segmentation

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/shot_classifier/segmentation.rs` comments. Why the model is
abstracted behind `SegmentationModel`: [2. Modules: `core::shot_classifier`](../02-modules-shot-classifier.md).
What `CroppedModel` does and why: [1. Why two resolutions](../01-why-two-resolutions.md).

## Why the mask is not pixel-accurate (`Mask`)
A `Mask` is in working-resolution coordinates: the preprocessor's output space (longest side
`config::WORKING_SIZE`), which is what the segmentation model ran on.

BiRefNet works at one fixed square side, read from the export's input metadata at load time
(`birefnet::input_size` rejects a dynamic axis), so it tracks whichever variant `build.rs` baked in.
The working image is squashed to that square and the mask resampled back. That round trip is why
the mask boundary is not pixel-accurate, and why the pass-2 refinement in `refine` exists at all.
