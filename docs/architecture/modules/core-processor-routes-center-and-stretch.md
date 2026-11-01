# core::processor::routes::center_and_stretch

[Up: architecture](../TOC.md)

R1's picture work: `mod.rs` (the grown silhouette, `apply`), `compose.rs` (the stretch),
`extend.rs` (the textured-scene reframe) and `inpaint.rs` (the flat fill). How each one works is
below. The decisions behind the stretch:
[one stretch function](../../work-state/2026-10-23-one-stretch-function.md).

## mod.rs

### The grown silhouette (`Silhouette`)

The product silhouette, grown outward, as the span it covers on every row (`row_x`) and every
column (`col_y`) of the *original* image, each `[lo, hi)`. That is the **product band**: copied
untouched and never a stretch source. Growing it keeps a cuff, an edge or a tiny shadow hugging the
product out of the stretch (plan A.2; the grey streak on images 10 and 11). The growth is
`R1_FILL_SOURCE_CLEARANCE_WORKING_PX` (plan name `FILL_SOURCE_CLEARANCE_PX`), the one
product-to-background distance the fill may use, in working pixels, scaled at use.

It is built once from the working-resolution mask, scaled up to the original, then grown by
`clearance_px` pixels in every direction (a disc, not a square). `clearance_px` is
`R1_FILL_SOURCE_CLEARANCE_WORKING_PX` scaled up to original pixels (plan A.2). Lines the grown shape
misses take the span of the nearest line that has one, so the band changes smoothly instead of
jumping between silhouette and bounding box.

**`spans_of`: reading the mask once.** The mask is at most `WORKING_SIZE` on its longest side and
the original can be many times that, so the mask is read once, in its own pixels, and the answer is
carried over to the original's lines. An original pixel `ox` reads mask column `mask_x(ox)`, which
never goes down as `ox` goes up, so the originals that read one mask column are one unbroken run: a
mask row's span is the first original of the run of its lowest set column and the last original of
the run of its highest. Only mask pixels some original pixel reads count (all of them, unless the
mask is the larger one). A test checks it against asking the mask about every original pixel.

**`grow_spans`: the disc and the slope limit.** A line's grown span is the union of the spans of
the lines within `r` of it, each widened by the disc's reach at that distance,
`floor(sqrt(r² − d²))`. Lines the disc does not reach take the span of the nearest line that has
one (the earlier one on a tie); with no span anywhere they come out empty, `(0, 0)`. Last, no edge
may move faster than `slope` (`R1_BAND_SLOPE_PX_PER_LINE`) pixels per line: an edge that would is
pulled outward from the line where it jumps (a cone along the lines: `lo` pulled down and `hi`
pushed up, forward, then back), so the span only ever widens. Without it, two shoes side by side,
where the second starts a few lines after the first, make the band's edge jump hundreds of pixels in
one line, which shows as a step in the stretch.

### No saliency in R1

The subject is the mask's own bounding box. The saliency-weighted centre of gravity is
`crop_square`'s, where a full bleed leaves no background to anchor against:
[spec §6](../../specs/06-crop-compose-r2.md).

### The background is not validated

R1 takes the background as trustworthy from the grown silhouette (`Silhouette`) all the way to the
border. Two validations of it were designed and then *deferred*, not forgotten:

* trimming a flat, artificial background run off the outer end;
* advancing the inner end past a cast shadow the mask does not cover.

Neither is worth its complexity until an image argues for it. The argument will look like a smeared
shadow, or a stretched band of the wrong colour along one side.

## compose.rs: the stretch

**Order.** `compose` lays the real part of the picture into a blank canvas the size of the square,
then calls `stretch_edge` once for each edge the square reaches past: left, right, top, bottom. Left
and right go first so top and bottom read full-width rows: that fills the corners, which have no
real pixels of their own. `stretch_edge` is the only place where R1 stretches, and the four edges
run the same code: the edge only sets how a pixel is addressed. A line is a row for left and right,
a column for top and bottom, run outward from the band. A test turns one scene so each edge in turn
plays the left edge, and checks the results match.

**One line.** Per line, three parts:

* the **product band**: the silhouette grown by the clearance, left untouched;
* the **source**: the background from the band out to the real border;
* the **overhang**: from the real border out to the canvas border.

The segment between the canvas border and the band is rewritten: the source is stretched evenly
over itself and the overhang. Nothing is invented. A column of the square past the picture's own
columns takes the nearest real column's band, which is where the left and right edges took their
background from.

**`stretch_line`: an even stretch.** One scale for the whole source, `total / n`, so no part of it
is magnified more than another: the pixel next to the product is the source pixel next to it, and
the last one is the border pixel. Reads are cubic (Catmull-Rom, `sample_at`): where the stretch
magnifies the most a linear read goes soft and smeared; the cubic keeps the edges of the
background's texture. It passes exactly through the source pixels. It can overshoot slightly at
sharp steps; the result is clamped to a valid colour. When `total` is not more than the source
there is nothing to cover, and the nearest `total` pixels are kept as they are.

**A line with no source (bled).** When the grown product runs to the real border there is nothing
to stretch. The segment is first filled flat with the background colour, never black
(`inpaint::fill_segment`): `bgc`, the one dominant background colour the exporter sampled, or white
without one. Then `borrow_neighbour_lines` (plan A.5) turns each bled pixel into the blend of the
pixels at the same position on the nearest clean line either side, weighted by closeness, plus the
photo's measured grain. A run of bled lines becomes a smooth ramp between the two clean lines round
it, with no hard-edged block; a clean line on one side only is copied; an edge with no clean line
keeps the flat fill. The nearest clean lines are found by two sweeps. The donor lines have
background at every position of the bled line's segment, because their segments are the longer
ones. Only bled pixels are ever written, so every stretched or copied pixel is byte-for-byte
unchanged.

**`edge_grain`: the photo's own grain.** The blend of two clean lines is smooth and the photo round
it is not: without grain an 8-bit ramp becomes flat blocks one level apart, and the JPEG save
flattens any dither. So the grain is measured per edge and per channel, as the standard deviation
of the noise, on the real source before anything is stretched. Only a bled line uses it, so it is
measured only when the edge has one; that has to be known before the stretch loop, which writes
over the source.

* Samples: every second line and every second pixel along it, whose whole 3x3 window lies in the
  source of its own line and of the two lines beside it.
* Residual: `v - mean of the 8 neighbours`, as an absolute value. Its 25th percentile is taken, which sits in flat
  areas, so texture does not count as grain.
* Scale to a Gaussian's sigma: the 25th percentile of `|N(0, s)|` is `0.3186 s`, and the residual's
  own sigma is `sigma * sqrt(9/8)` (one pixel against the mean of its eight neighbours).
* Pixels are addressed by their distance `k` from the real border, so all edges measure the same
  way. An edge with fewer than `GRAIN_MIN_SAMPLES` (30) samples reads as no grain.

The grain is added with `gauss`: a standard-normal value from Box-Muller on a hash of (line,
distance, channel), so the same picture always gives the same grain.

## extend.rs: the textured-scene reframe (plan Part C)

R1 runs it (`R1Job::Reframe`, chosen in `shot_classifier::route`) when the preprocessor trimmed
flat letterbox/pillarbox bars round a textured scene (`preprocessor::optimizer::trim_letterbox`,
`Prepared::letterbox`; see [the letterbox detector](core-preprocessor-optimizer.md)). Such a scene
is reframed **without** R1's stretch, which smears a textured overhang (measured on 268/46).
`reframe_to_square` is pure and tries, highest first:

1. **Crop** (`square_crop_preserving`): the largest square crop (the shorter frame side) that still
   holds the whole subject box, centred on the subject and slid inside the frame. No invented
   pixels. It declines when the subject is longer than that side on either axis. When it fits,
   centring on the subject and clamping into the frame always keeps the whole subject inside
   (`side >= subject.w` and `side >= subject.h`).
2. **Seam-carve** (`seam_carve_to_square`): minimum-energy seam insertion out to a square of the
   longer side. Only *extension* is handled; an axis larger than the target is the crop / white-pad
   case.
3. **White-pad** (`white_pad`), the last resort: centre the scene on a white square of its longer
   side.

**How the seam-carve works.**

* Energy per pixel: `energy_map`.
* The `k` cheapest seams: the minimum top-to-bottom seam (classic cumulative-energy DP) is found and
  removed from a working copy, `k` times, so the `k` are distinct and not the same cheap seam `k`
  times (Avidan–Shamir). Each seam's energy is read from the original map, so the gate is stable.
* Insertion: `insert_vertical_seams`; the height grows through `grow_height`.
* Rounds: each axis grows in rounds of at most its current length, so no single round more than
  doubles it (seam insertion past ~2x degrades even on clean scenes).
* Quality gate: any round whose cheapest seams average above `config::R1_SEAM_CARVE_MAX_SEAM_ENERGY`
  per pixel fails the whole carve, and `reframe_to_square` falls through to the white pad. This
  gate is the one check left inside a route
  ([work state 2026-10-29](../../work-state/2026-10-29-routing-in-shot-classifier.md)).

## inpaint.rs: the flat fill

A line (a row or a column) is *fully bled* on one side when the product, grown by the clearance,
runs to the border there: there is nothing to stretch and nothing to invent from. Those segments
land in `fill_segment` and get a flat BGC-or-white fill; `compose`'s neighbour-line borrow may then
replace them with the background of the clean lines round them. Every line that does have a source
stays with `compose`; this module never touches it.

`fill_segment` also has arms R1 does not use: with the end pixels of the stretched runs either side
(`left_ctx` / `right_ctx`, "left" being the lower output index whatever image edge that is) it
blends between them, or fills flat from the one present. A wider inpainter, if ever needed, slots in
behind the same signature ([open follow-ups](../../work-state/open-r1-stretch-follow-ups.md)).

The fill carries uniform noise up to ±`R1_INPAINT_NOISE_SIGMA` from a signed byte roll, so it does not read as a dead-flat seam against the JPEG-noisy stretched pixels
next to it. The noise comes from a tiny xorshift generator seeded by the context colours and the
segment length, so the output image is deterministic.
