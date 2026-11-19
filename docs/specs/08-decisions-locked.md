# 8 · Decisions locked

[Up: table of contents](TOC.md)

- SC is the brain, and it is segmentation-based: a BiRefNet mask plus a two-pass gate/refine edge
  verdict.
- **"No verdict" is a first-class outcome**, distinct from "touches no edge". It routes to R3.
- Working size ≤1024; boundaries always confirmed at full resolution.
- R1 is bbox-anchored: square = longest mask side + 4.2% per free side (8.4% when both sides of the axis are free, 4.2% when one is pinned, `R1_MARGIN = 2.0*0.042`), bled edges pinned, background stretched to fill. No saliency, no clipping of the subject, and the 42% cap suspended.
- CoG/MSR ([§6](06-crop-compose-r2.md)) is R2's machinery now — the full bleed is the only case with no background to anchor against.
- Three routes, one bleed rule: pin the bled edges, margin the free ones. Rows 0-3 of the old edge table are that rule seen from four angles and all live in R1; only the full bleed is left to R2.
- Output 1:1 [800,2000], JPEG q90–95 4:4:4 sRGB. The envelope is enforced in the exporter — which
  every route ends at — so no route can bypass it.
- Ships as one hardened exe with no install. **Met** — the ONNX Runtime and the model are both
  embedded in the exe (the model encrypted), unpacked/decrypted at runtime, never shipped as files
  beside it. See [architecture §5](../architecture/05-single-exe.md).
