# Decided 2026-10-21: R1 stretch rework (code in `center_and_stretch/compose.rs`)

[Up: table of contents](TOC.md)

* R1 works one line at a time (a row for the left and right edge, a column for the top and bottom).
* Product band = the silhouette grown by one clearance, `R1_FILL_SOURCE_CLEARANCE_WORKING_PX`.
* No gating: BGC is sampled on every image.
* The bar detector lives in the preprocessor (`Prepared::letterbox`); same-colour bars are left to
  `core::framing`.
* The stretch itself: see [one stretch function](2026-10-23-one-stretch-function.md).
