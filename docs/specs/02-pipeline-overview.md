# 2 · Pipeline overview

[Up: table of contents](TOC.md)

Five stages. The Shot Classifier decides which route an image takes; everything downstream hangs
off that one decision.

| Stage | Does | Status |
|---|---|---|
| **A · Load** | Decode, apply EXIF orientation, read the embedded ICC profile. | `BUILT` |
| **B · Preprocess** | Flatten alpha onto white, colour-manage to sRGB, derive the working-resolution copy, then run the optimiser: colour-balance (Levels, once) then trim crop, producing the image the model sees. Detail in [architecture: why two resolutions](../architecture/01-why-two-resolutions.md). | `BUILT` |
| **C · Classify** | Segment the working copy, then decide per edge whether the subject reaches it. Emits the EIX verdict — or no verdict. | `BUILT` |
| **D · Dispatch** | EIX verdict → one of three routes ([§5](05-routing.md)). | `BUILT` |
| **E · Route** | R1/R2 shape the square; R3 frames it safely. | `BUILT` |
| **F · Export** | Size envelope, then JPEG + ICC, then delete the source on success. | `BUILT` |

Segmentation produces the mask and the edge verdict in one pass — no intermediate box to find and
then classify.
