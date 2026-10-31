# Decided 2026-10-21: R1 stretch rework (user spec; code in `center_and_stretch/compose.rs`)

[Up: table of contents](TOC.md)

* One line at a time (a row for the left and right edge, a column for the top and bottom edge).
  Product band = silhouette grown by one clearance (`R1_FILL_SOURCE_CLEARANCE_WORKING_PX`, a
  disc, per row and per column). It replaces the old `safety` + `clearance` pair (`R1_SAFETY_*`), which did the same job twice.
* **Superseded 2026-10-23, see [one stretch function](2026-10-23-one-stretch-function.md):** the stretch curve (0% next to the product, +100% at the
  border, `R1_STRETCH_CURVE_POWER`), the invented pixels (mirror, fade to the neighbours' average,
  boxplot reference, `R1_FILL_INPAINT_RADIUS_PX`) and the averaging of neighbouring lines' stretch.
  They are gone from the code; one even stretch replaced them.
* Removed: gating (`gated`, BGC tolerance, local-BGC fallback, `classify_edge`, the stage-90 strip overlay).
  BGC is now sampled on every image.
* Letterbox/bar detector moved into the preprocessor (`Prepared::letterbox`), per plan Part C: an
  opposite pair `FlatOther` and the other pair `Varied`; texture outside `product_box` on the trimmed
  interior. Stage image 02 shows it. Consequence: same-colour bars read as backdrop, so the detector
  misses 41, 46 (and slightly 30, 33, 35). That's now handled by `core::framing` ([the trims and framing](2026-10-21-trims-and-framing.md)).
