# 3 · Shot Classifier (SC) — the brain `BUILT`

[Up: table of contents](TOC-specs.md)

The model sees the optimiser's crop; the edge checks run on the full working image. SC answers one question: **which image edges does the
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
  and background. The refined mask, not the raw mask, says whether the subject reaches the edge.
- **Pass 2 · graze test.** Reaching the edge is not yet touching. If the silhouette runs *along* the
  edge instead of going *off* it, the contact is a graze and the edge counts as not touching. The
  test compares how much of a narrow strip (`REFINE_NARROW_BAND_PX`) and of the wide strip
  (`GATE_MARGIN_PX`) at the edge the refined mask fills; a ratio under `GRAZE_RATIO_MAX` is a graze.

Pass 2 does real work at every scale. When the working copy equals the original there is no
upsampling error left to correct, but the colour matting on the unknown ring runs identically.

## 3.2 Output contract

`ShotClassification { touches_edges: Vec<Edge>, mask_too_small: bool, refinements: Vec<EdgeRefinement> }`.

The model always returns a mask. When it is empty there is **no verdict**: EIX `0000`,
`mask_too_small` true, and [§5](05-routing.md) sends the image to R2.
