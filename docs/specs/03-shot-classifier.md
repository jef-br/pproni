# 3 · Shot Classifier (SC) — the brain `BUILT`

[Up: table of contents](TOC.md)

SC runs on the working-resolution copy and answers one question: **which image edges does the
subject actually reach?** That verdict is what routing keys on ([§5](05-routing.md)).

A bounding box cannot answer it reliably — a box can touch an edge while the silhouette does not,
and vice versa for a diagonal pose. So the answer comes from segmentation, not geometry.

## 3.1 How it works

BiRefNet (dichotomous image segmentation, ONNX) produces one foreground mask for the whole image —
no boxes, no classes, no NMS. The edge verdict is then taken in two passes:

- **Pass 1 · gate.** Scan only a `GATE_MARGIN_PX`-wide band along each edge of the mask. A hit means
  "worth checking precisely", *not* "touches": the mask is upsampled from the model's internal
  resolution and is not trusted for the verdict.
- **Pass 2 · refine.** For each gated edge, intersect the mask with the border band, crop that region
  at full resolution, and build a trimap — interior foreground, exterior background, a ring around
  the boundary marked unknown. Classify the unknown ring by colour distance to sampled foreground
  and background. The refined mask, not the raw mask, answers touching or not.

Pass 2 does real work at every scale. When the working copy equals the original there is no
upsampling error left to correct, but the colour matting on the unknown ring runs identically.

## 3.2 Output contract

`ShotClassification { touches_edges: Vec<Edge>, refinements: Vec<EdgeRefinement> }` — or **no
verdict**, when the model is unavailable or inference failed on that image.

Those are different states and they route differently ([§5](05-routing.md)). This answers Q3.

## 3.3 Triggers

SC remains the place a Phase-2 human detector would hook in (people, head-to-toe framing).
