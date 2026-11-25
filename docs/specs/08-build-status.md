# 8 · Build status

[Up: table of contents](TOC-specs.md)

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
| R2 · CoG/MSR guided crop | `PLANNED` — a refinement of the centred crop above: anchor on the saliency-weighted centre instead of the frame's middle. Described as a refinement in [core-processor-routes](../architecture/modules/core-processor-routes.md) and [core-processor-routes-center-and-stretch](../architecture/modules/core-processor-routes-center-and-stretch.md) |
| Output size envelope | `BUILT` — in the exporter |
| Save JPEG + ICC | `BUILT` |
| Single hardened exe, no install | `BUILT` — models + runtime embedded; see [architecture §5](../architecture/05-single-exe.md) |
| Headcuts | `ONGOING` |
| · via YuNet (heads with a detectable face) | `BUILT` — switched off for now (`PERMIT_HEADCUTS = false`) until the feature is complete |
| · without YuNet, by heuristics (heads with no clear face) | `BACKLOG` — not started. Signals so far: skin colour inside the image; the box has an aspect ratio a human could also have; the top edge is not touched (EIX); images with similar filenames had a headcut. More signals may be needed |
