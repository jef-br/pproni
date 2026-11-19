# core::shot_classifier::sampling

[Up: architecture](../TOC.md)

Detail moved out of `src/core/shot_classifier/sampling.rs` and the `BoolSummedArea` doc in
`src/core/shot_classifier/geometry.rs`. The module table is
[2. Modules: `core::shot_classifier`](../02-modules-shot-classifier.md).

## Why the mask window uses a summed-area table
`sample_background_color` asks "is any mask pixel within `margin` of this one?" once per pixel of
the sampling strip. `margin` is a full-resolution number — the working-resolution safety margin
times the scale between the two — so the window's radius scales with the image: thousands of
pixels at full resolution, tens of thousands of pixels wide on a large original.

Scanning that window per strip pixel was the pipeline's hot spot: a background pixel, the common
case, only reached its answer after reading every cell. `geometry::BoolSummedArea` (prefix sums
over the mask) gives the same answer in four lookups. Building it is one pass over the rows the
strip can actually reach; every query after it is O(1). The old window scan is kept as the
reference in the `summed_area_matches_the_window_scan` test.

## The colour distance (`ColorStats::distance_sq`)
Squared Euclidean distance in RGB space: cheap and sufficient for a foreground/background decision.
Swap it for a perceptual distance (e.g. CIEDE2000 in Lab) later if the classifier proves sensitive
to it on hard cases.
