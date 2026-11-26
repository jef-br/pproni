# 6 · Decisions locked

[Up: table of contents](TOC-specs.md)

- SC is the brain, and it is segmentation-based: a BiRefNet mask plus a two-pass gate/refine edge
  verdict.
- An empty mask is a normal outcome: EIX `0000`, routed to R2 as a too-small mask.
- **No verdict** (`____`) is something else: [EIX](03-eix.md).
- Working size ≤ `WORKING_SIZE`; every edge the gate flags is confirmed at full resolution.
- R1 is bbox-anchored: square = longest mask side + half of `R1_MARGIN` per free side, bled edges pinned, new canvas filled from the background. No saliency and no clipping of the subject.
- R2 cuts the largest square, centred on the frame. A saliency-guided crop (CoG/MSR) is a planned refinement of R2 only: the full bleed is the only case with no background to anchor against, so saliency is the only signal left.
- Three routes, one bleed rule: pin the bled edges, margin the free ones. Rows 0-3 of the old edge table are that rule seen from four angles: they go to R1 when the square fits, R3 otherwise; the full bleed and left+right go to R2.
- Output 1:1, side from `MIN_SIZE` to `MAX_SIZE` (800≤x≤2000), JPEG at `JPEG_QUALITY` 4:4:4 sRGB. The envelope is enforced in the exporter — which
  every route ends at — so no route can bypass it.
- Ships as one hardened exe with no install. **Built** — the ONNX Runtime (DirectML build) and the
  models are all embedded in the exe, the runtime unpacked at startup, never shipped as files beside it. See [architecture §5](../architecture/05-single-exe.md).
