# Decided 2026-10-23: one stretch function for all four edges, an even stretch (user)

[Up: table of contents](TOC.md)

* `compose::stretch_edge` is the only place R1 stretches, for all four edges alike.
* Each line's background is stretched by one even scale. Nothing is invented: no mirror, no blur, no
  noise.
* A line with no background left is filled flat with the BGC (white without one), then blended from
  the clean lines either side.
* Blended lines get the photo's own measured grain (user approved, 2026-10-22). Nothing else is added.
* No cap on the stretch: a tight free-standing subject overhangs on every side.
* Rejected: the border trim (leaving the outermost real pixels out of the stretch source).
* How it works: [module page](../architecture/modules/core-processor-routes-center-and-stretch.md).
