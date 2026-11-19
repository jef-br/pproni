# Decided 2026-10-21: the trims and framing (`optimizer::optimize_both`, `core::framing`)

[Up: table of contents](TOC.md)

* BiRefNet runs once per image, on a backed-off crop of trim 1.
* Trim 2 (`recheck_box`) is information only (`Prepared::trims`).
* Framing, after the model run: bars → `Centre` (see [bars are always centred](2026-10-23-bars-always-centred.md)),
  a frame all round → `Peel`, anything else → `Keep`. How each is detected:
  [architecture: core modules](../architecture/02-modules-core.md).
* `FRAMING_BARS_SYMMETRY` (0.3) stays (added without being asked; the user kept it).
* R1 margin: 4.2% per free side of an axis, none on a pinned side.
* Clearance (20 working px) and the framing thresholds are working values: tuning is allowed when an
  image shows a problem.
* No erosion/dilation to plug holes in trim 1's region.
