# 1 · Principles & hard invariants

[Up: table of contents](TOC-specs.md)

- **KPI:** repositioning accuracy at high speed.
- **Product is never distorted.** Only background is ever stretched.
- **Real pixels first.** Prefer cropping into the real image over inventing pixels; stretch only when a real-pixel square can't be had.
- **Output:** 1:1 square, side from `MIN_SIZE` to `MAX_SIZE` (800≤x≤2000).
- **Encode:** JPEG at `JPEG_QUALITY`, 4:4:4 (no chroma subsampling), embedded sRGB ICC. `BUILT`
- **Failures** leave the input untouched (never deleted), never crash the batch.
