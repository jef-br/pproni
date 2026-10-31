# core::preprocessor::maria_antonieta

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/preprocessor/maria_antonieta.rs` comments. The headcut decisions,
the caller's gate and the section size are
[work state 2026-10-28: headcuts](../../work-state/2026-10-28-headcuts.md); how the model is embedded
is [5. Single hardened exe](../05-single-exe.md). The public entry in the code is `yxunomei` (those
pages still call it `cut_above_philtrum`).

## YuNet's input and outputs

Verified against `face_detection_yunet_2026may.onnx`:

* Input `input`: `[1, 3, H, W]`, dynamic H and W, float32, **BGR** order, pixels 0–255 (no mean
  subtraction, no std division). The module runs it at a fixed `INPUT_SIZE` square to keep anchor
  counts predictable; the section is already square-ish, so a plain resize suffices.
* 12 outputs grouped as `{cls, obj, bbox, kps}` × strides `{8, 16, 32}`. `cls_*` and `obj_*` are
  already sigmoid-ed in the graph; `bbox_*` and `kps_*` are raw regression tensors.
* One anchor per spatial cell. Final score = `cls[i] * obj[i]`.
