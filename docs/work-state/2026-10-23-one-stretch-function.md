# Decided 2026-10-23: one stretch function for all four edges, an even stretch (user)

[Up: table of contents](TOC.md)

* `compose::stretch_edge` is the only place R1 stretches. `compose` calls it four times, for the left,
  right, top and bottom edge, each only when the square reaches past that edge. Left and right come
  first, so the top and bottom edges read full-width rows (that fills the corners). The vertical pass
  used to be the horizontal one on a transposed picture and missed the neighbour-line borrow; now all
  four edges behave the same, and a unit test turns one scene so every edge plays the left edge and
  checks the results match.
* Per line the background between the product band and the real border is stretched by ONE scale
  (`stretch_line`, cubic reads), so nothing is magnified more than the rest. Nothing is invented: no
  mirror, no blur, no noise. A line with no background left (the grown product reaches the border) is
  filled flat with the BGC (white without one), then takes a blend of the clean lines either side,
  weighted by distance (`borrow_neighbour_lines`); with a clean line on one side only it copies that
  line, and an edge with no clean line keeps the flat fill. Copying the nearest clean line (first
  version) left a dark rectangle with hard edges on 49's right side, so a run of blocked lines is now
  a smooth ramp between the two clean lines around it. Only 48 and 49 changed in the 49 set.
* The blended lines get the photo's own grain added (user approved measured grain in filled areas,
  2026-10-22): `edge_grain` measures it on the real source of the edge (25th percentile of the residual
  against the mean of the 8 neighbours, so texture does not count; no grain where the source is clean),
  `gauss` adds it from a fixed hash, so the same picture gives the same result. Without it, the
  8-bit ramp on 22 became two flat blocks one level apart, and the JPEG save flattened any dither.
  Only 22 changed in the 49 set. Nothing else is added anywhere.
* Fixes the glitch under the sandal on 22 (the vertical pass drew a striped block there), the mirror on
  1, the blur on 25-27 and 36's right side, and the rectangle on 49. A faint rectangle outline can
  still be seen under 22's toe at 14x contrast (the fill is smoother than the photo around it).
* Border trim (leave the 3 outermost real pixels out of the stretch source): tried and reverted
  2026-10-23. No input in the 49 set has a JPEG edge line on a stretched edge, so it fixed nothing
  visible. The stripes first blamed on it came from the crop boundary of the old bars-dropped route (41)
  and from tiny per-column sources next to 22's toe.
