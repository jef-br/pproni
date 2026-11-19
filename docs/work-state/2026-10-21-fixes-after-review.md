# Decided 2026-10-21: fixes after the user's review

[Up: table of contents](TOC.md)

* The product band's edge moves at most `R1_BAND_SLOPE_PX_PER_LINE` (1) per line, only by widening.
* The stretch reads its source with a cubic (Catmull-Rom), not linear.
