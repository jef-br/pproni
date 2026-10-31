# core::shot_classifier::birefnet

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/shot_classifier/birefnet.rs` comments. The module table is
[2. Modules: `core::shot_classifier`](../02-modules-shot-classifier.md); the embedded, encrypted
model and the unpacked runtime are [5. Single hardened exe](../05-single-exe.md).

## What the model is
A pure foreground/background model: no boxes, no classes, no NMS — one sigmoid mask at full model
resolution answering "subject or not" per pixel.

## Input and output tensors
Verified against exported `BiRefNet_lite` ONNX models:

- Input `input_image`: `[1, 3, S, S]` float32, RGB, resized (not letterboxed — BiRefNet's own
  preprocessing is a plain resize) and ImageNet-normalized (mean `[0.485, 0.456, 0.406]`, std
  `[0.229, 0.224, 0.225]`). The side `S` (512 and 1024 are the common full-size exports; a smaller
  `S` trades edge accuracy for far less memory) is read from the model's own input metadata at load
  time, not hardcoded.
- Output `output_image`: `[1, 1, S, S]` float32 — raw logits; apply sigmoid, threshold, resize back
  to the source image size.

## Why the shipped model has no `DeformConv`
BiRefNet's decoder uses deformable convolutions. A stock export makes them a single native
`DeformConv` op, which DirectML cannot run. On the `PProni-GPU` branch the shipped model has those 20
nodes rewritten as `GridSample` + `Conv` (same weights, same maths — masks match the original to
float rounding), so the one embedded model runs on a graphics card or on the CPU.

## Session memory and the optimization level
The session uses one intra-op thread because `app::batch` fans images across cores with rayon.
Peak memory tracks that fan-out — roughly 400MB of activations per concurrent worker — and not the
optimization level: measured session-init peak is 393MB at Level1, Level2 and Level3 alike. An
earlier comment in the code blamed Level3 for an OOM during session init; that was a 4GB CI
container, not a property of the graph.

## Thresholding in logit space
`logit` turns the mask threshold into a logit once per image. Thresholding each pixel's sigmoid
cost an `exp` per *output* pixel, near a million of them at working resolution. At the default 0.5
the two forms agree exactly, so the shipped mask is unchanged pixel for pixel.
