# 4 · Resolution & bbox policy `BUILT`

[Up: table of contents](TOC-specs.md)

- **Working size:** if the largest dimension is ≤ `WORKING_SIZE` (which the user decided to set at 1024 px), the image is used as-is;
  larger images are reduced to `WORKING_SIZE` on the long side. Never upscaled — upscaling would invent detail
  the model then reads as real.
- **Boundaries are resolved at full resolution.** Segmentation runs on a reduced copy (the optimiser's crop of the working copy) for speed, but pass 2 re-decides the boundary against full-resolution pixels.
  Both resolutions travel together in `Prepared` (its `original` and `working` fields) for exactly this reason. This keeps the edge checks exact.

## 4.1 Edge-intersection detection `BUILT`

Edge intersection is the gate/refine result described in [§3.1](03-shot-classifier.md): a decision per edge taken on the segmentation mask and confirmed at full resolution.
