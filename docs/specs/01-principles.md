# 1 · Principles & hard invariants

[Up: table of contents](TOC.md)

- **KPI:** repositioning accuracy at high speed.
- **Product is never distorted.** Only background is ever stretched.
- **Real pixels first.** Prefer cropping into the real image over inventing pixels; stretch only when a real-pixel square can't be had.
- **Max background stretch = 42%** on any side. `SUSPENDED` — dropped from R1 pending evidence it is needed; if artefacts appear, the answer may be further cleaning, seam carving or inpainting rather than a hard cap.
- **Output:** 1:1 square, side ∈ [800, 2000] px.
- **Encode:** JPEG q90–q95, 4:4:4 (no chroma subsampling), embedded sRGB ICC. `BUILT`
- **Failures** leave the input untouched (never deleted), never crash the batch.
