# core::framing

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/framing.rs` comments. The bars and frame rules themselves
(`Keep`, `Centre`, `Peel`, their thresholds) are in [2. Modules: `core`](../02-modules-core.md).

## What tells padding from backdrop
What trim 1 trims off is either real backdrop round a floating product or not picture at all
(padding bars, a frame). The mask of the one model run, on trim 1's crop, is the reliable fact
that tells them apart.

## The frame's colour (`border_colour`)
The dominant colour of the picture's outer four lines all the way round: colours are grouped in
5-bit bins, and the answer is the mean of the fullest bin.

Dominant, not median, and four lines deep rather than one: the very last row or column of a JPEG is
often a one-pixel line of another colour, and on a thin frame that line can be half of the outer
ring (it was, on the frame this was written for).

## Peeling deeper than trim 1 (`deepen_peel`)
Trim 1 stops where something not of the frame's colour begins, so where the product runs into the
frame (a dark product against a dark frame) the box can stop several pixels inside it. Each side of
trim 1's box is moved inward while its outermost line is still frame: at least 99% of the pixels
that are NOT under the mask lie within `T_EDGE_FLAT` of the frame colour on every channel. No side
moves more than a quarter of the box.

The mask is what makes this safe: product pixels never count against the frame, and a line with
real background or product outside the mask stops the peel.
