# core::preprocessor::maria_antonieta

[Up: module pages](TOC-module-architecture.md)

Detail moved out of the `src/core/preprocessor/maria_antonieta.rs` comments. The headcut decisions,
the caller's gate and the section size are
[work state 2026-10-28: headcuts](../../work-state/2026-10-28-headcuts.md); how the model is embedded
is [5. Single hardened exe](../05-single-exe.md). The public entries in the code are `cut_above_philtrum` (the
headcut) and `preflight` (builds the model's session at startup, so a broken model stops Pproni
before any image).

## YuNet's input and outputs

Verified against `face_detection_yunet_2026may.onnx`:

* Input `input`: `[1, 3, H, W]`, dynamic H and W, float32, **BGR** order, pixels 0–255 (no mean
  subtraction, no std division). The module runs it at a fixed `INPUT_SIZE` square (320) to keep anchor
  counts predictable; 320 is enough resolution for a face that already fills a headcut section. The
  section is already square-ish, so a plain resize suffices.
* 12 outputs grouped as `{cls, obj, bbox, kps}` × strides `{8, 16, 32}`. `cls_*` and `obj_*` are
  already sigmoid-ed in the graph; `bbox_*` and `kps_*` are raw regression tensors.
* One anchor per spatial cell. Final score = `cls[i] * obj[i]`.
