# core::processor::routes::center_and_stretch

[Up: architecture](../TOC.md)

R1's picture work: `mod.rs` (the grown silhouette, `apply`), `compose.rs` (the stretch),
`extend.rs` (the textured-scene reframe) and `inpaint.rs` (the flat fill). The stretch's design and
how it got there: [one stretch function](../../work-state/2026-10-23-one-stretch-function.md).

## mod.rs: the background is not validated

R1 takes the background as trustworthy from the grown silhouette (`Silhouette`) all the way to the
border. Two validations of it were designed and then *deferred*, not forgotten:

* trimming a flat, artificial background run off the outer end;
* advancing the inner end past a cast shadow the mask does not cover.

Neither is worth its complexity until an image argues for it. The argument will look like a smeared
shadow, or a stretched band of the wrong colour along one side.
