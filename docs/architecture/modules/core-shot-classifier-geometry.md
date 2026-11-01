# core::shot_classifier::geometry

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/shot_classifier/geometry.rs` comments. The idea behind `EdgeView`
(every per-edge algorithm written once, for the top edge) is
[2. Modules: `core::shot_classifier`](../02-modules-shot-classifier.md). Why `BoolSummedArea` exists:
[core::shot_classifier::sampling](core-shot-classifier-sampling.md).

## Who goes through `EdgeView`
The per-edge algorithms written against `Grid` for the top edge: the border-band scan (`gate`),
background-colour sampling (`sampling`) and the trimap ring (`refine`). `Grid` is implemented for
`segmentation::Mask` directly and for `image::RgbImage` through the thin `sampling::ImageGrid`
adapter, so both run through the same edge-relative code.
