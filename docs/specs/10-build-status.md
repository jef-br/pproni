# 10 · Build status

[Up: table of contents](TOC.md)

| Piece | State |
|---|---|
| Load / orient / flatten / sRGB | `BUILT` |
| Working-resolution copy | `BUILT` |
| Shot Classifier — BiRefNet + gate/refine | `BUILT` |
| Edge intersections | `BUILT` |
| Route selection (R1/R2/R3) | `BUILT` |
| R3 · Fallback | `BUILT` |
| R1 · Center & Stretch | `BUILT` |
| R2 · CropSquare | `BUILT` — largest centred square |
| CoG/MSR crop ([§6](06-crop-compose-r2.md)) | `DEFERRED` — R2 crops frame-centred; no image needs saliency yet |
| Output size envelope | `BUILT` — in the exporter |
| Save JPEG + ICC | `BUILT` |
| Single hardened exe, no install | `BUILT` — model + runtime embedded; see [architecture §5](../architecture/05-single-exe.md) |
| Human detector | `PHASE 2` |
