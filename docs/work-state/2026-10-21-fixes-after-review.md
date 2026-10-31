# Fixed 2026-10-21 (user review of the rework)

[Up: table of contents](TOC.md)

* Image 1 had a horizontal step above the right shoe: the product band is one span per line, and the
  right shoe starts a few lines after the left one, so the band's edge jumped 575 px in one line.
  Fix: the band edge may now move at most `R1_BAND_SLOPE_PX_PER_LINE` (1) per line (only widening).
* Image 36's overhang: stretch read with a cubic (Catmull-Rom) instead of linear, and invented pixels
  fade from the mirrored detail into the neighbours' average colour with distance from the seam.
  Studio backgrounds got closer to the real noise level too (22: 0.61 -> 0.34, real 0.26).
* 45 (dark frame round the picture): fixed by the frame peel ([the trims and framing](2026-10-21-trims-and-framing.md)). R1 had been inventing the bottom overhang
  from the frame bar.
