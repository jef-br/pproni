# 4 · Resolution & bbox policy `BUILT`

[Up: table of contents](TOC.md)

- **Working size:** if the largest dimension is ≤ `WORKING_SIZE` (1024 px), the image is used as-is;
  larger images are reduced to 1024 on the long side. Never upscaled — upscaling would invent detail
  the model then reads as real.
- **Boundaries are resolved at full resolution.** Segmentation runs on the working copy for speed,
  but pass 2 re-decides the boundary against full-resolution pixels. Both resolutions travel together
  as `Prepared { original, working }` for exactly this reason. (This is the fix for the loose-box
  problem, P2.)

## 4.1 Edge-intersection detection `BUILT`

Edge intersection is the gate/refine result described in [§3.1](03-shot-classifier.md): a decision per edge taken on the
segmentation mask and confirmed at full resolution.
