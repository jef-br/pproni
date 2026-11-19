# core::shot_classifier::refine

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/shot_classifier/refine.rs` comments. Pass 2's steps are
[spec §3.1](../../specs/03-shot-classifier.md); the module table is
[2. Modules: `core::shot_classifier`](../02-modules-shot-classifier.md).

## The graze test (`graze_ratio`)
Step 6 of `refine_edge`: reaching the border is not sufficient. A hem curving down to kiss the
bottom edge reaches it as surely as a subject the frame cuts in half, and only the second gives R2
an edge to anchor a crop on. So a contact must be a real bleed-off, not a graze, to count as
touching.

A subject the frame truncates holds the same coverage at every depth — its silhouette meets the
border head-on and simply carries on past it. One that grazes runs *along* the border and curves
away, so coverage grows the deeper you look. Comparing mean coverage in a narrow band against a
wide one turns that into a single scale-free number: about 0.61 for a graze, 0.93 and up for a real
bleed-off.

It is measured on the *refined* mask, in crop coordinates, so `wide` and `narrow` arrive already
scaled to full resolution. It has to be the refined mask: where matting moved the boundary, the raw
mask's profile describes a silhouette that is no longer the one being judged — a raw mask stopping
short of a border its subject really reaches has zero coverage in the narrow band and would read as
a graze every time.

The ratio does not care how wide a column range it is taken over — the width cancels — so the crop
spanning only the contact region rather than the whole edge leaves it unchanged.

## Why the trimap ring is found by walking runs (`Trimap::from_mask`)
A pixel is in the ring when some pixel within `ring_px` of it, along a row or a column, holds the
opposite value. Asking that pixel by pixel meant walking `ring_px` steps in four directions each
time, and `ring_px` scales with the original — tens of steps on a large photo, never
short-circuited for the uniform background that dominates the crop.

Walking runs instead answers it in one pass per axis. Within a run of equal values the nearest
opposite pixel is whatever sits just past the run's end, so the ring is the first and last
`ring_px` pixels of every run that actually has a neighbouring run — no per-pixel search at all.
The test `matches_the_brute_force_definition` checks it against the per-pixel definition.
