# Pproni work state

Read this first. Keep it short. Update it at the end of each work session.
Last updated: 2026-10-23.

## Decided
* The 512 model (`birefnet_lite_512.onnx`) is canonical — proven to work, and what `build.rs`
  bakes in (`MODEL_FILE`).
* The preprocessor ends with one function, `preprocessor::optimizer::optimize`. It does two steps, and
  its output is the only image BiRefNet sees (`Prepared::model_input`, cut from `working` at `Prepared::crop`):
  * 1: colour balance (Levels, once per image).
  * 2: trim, spec'd by the user on 2026-10-13: 16 border samples (4 per edge at 0/25/50/75%);
    outliers dropped by MAD (`TRIM_OUTLIER_FLOOR`/`_K`, always keeping the `TRIM_MIN_KEEP` closest to
    the median); the kept samples' min/max per channel is the backdrop colour range; gaps closed with
    a 2x2 window (`TRIM_CLOSE_K`);
    box around the product; squared to its largest side; margin = larger of 10 px and 10% of that
    side; the square is centred on the product and slid inside the frame (margin squeezed if the frame
    is too narrow; if the product is longer than the frame's short side the margin drops to 10 px and
    the crop is cut at the frame, not square); crop.
  * Nothing but backdrop left: `optimize` returns `Err`; the file stays in INPUT and counts as failed.
  * The model's mask is pasted back into a full working-size canvas by `CroppedModel`
    (`shot_classifier::segmentation`), so the gate, refinement and routes are unchanged.
* `working` and `original` stay true colour. BGC/FGC are sampled from `working`.
* The 5% rule (`MASK_MIN_COVERAGE`) is the user's call. Never change it unasked. it's the only 5% rule.
* No retries, no guards, no fallbacks anywhere in the trim. Add none without asking.
* Goal now: improve results by refining the preprocessor, not by fine-tuning the model.

## Decided 2026-10-21: R1 stretch rework (user spec; code in `center_and_stretch/compose.rs`)
* One line at a time (a row for the left and right edge, a column for the top and bottom edge).
  Product band = silhouette grown by one clearance (`R1_FILL_SOURCE_CLEARANCE_WORKING_PX`, a
  disc, per row and per column). It replaces the old `safety` + `clearance` pair.
* **Superseded 2026-10-23, see below:** the stretch curve (0% next to the product, +100% at the
  border, `R1_STRETCH_CURVE_POWER`), the invented pixels (mirror, fade to the neighbours' average,
  boxplot reference, `R1_FILL_INPAINT_RADIUS_PX`) and the averaging of neighbouring lines' stretch.
  They are gone from the code; one even stretch replaced them.
* Removed: gating (`gated`, BGC tolerance, local-BGC fallback, `classify_edge`, the stage-90 strip overlay).
  BGC is now sampled on every image.
* Letterbox/bar detector moved into the preprocessor (`Prepared::letterbox`), per plan Part C: an
  opposite pair `FlatOther` and the other pair `Varied`; texture outside `product_box` on the trimmed
  interior. Stage image 02 shows it. Consequence: same-colour bars read as backdrop, so the detector
  misses 41, 46 (and slightly 30, 33, 35). That's now handled by `core::framing` (next section).

## Decided 2026-10-21: the trims and framing (`optimizer::optimize_both`, `core::framing`)
* **Only run birefnet model once per image.**
* **BiRefNet is shown a trim-backed-off crop of the original image**
* **Trim 2 (`recheck_box`) re-runs the trim on trim 1's tight box** It counts as "moved" when any side
  differs by MORE than `TRIM_RECHECK_TOLERANCE_PX` (2). it's information only (`Prepared::trims`).
  Stage image 03: trim 1 cyan, trim 2 yellow (only when it moved), the crop green.
* **Framing** (`core::framing::decide`, after the model run, pure and unit-tested; the reason is one
  log line per image, `framing: ...`, in `<n>  -  log.txt`):
  * **Bars**: trim 1 cut both sides of ONE axis (the other axis loses at most `FRAMING_OTHER_AXIS_SLACK`
    = 2% in total), the subject touches an edge (EIX not `0000`) and the two cut sides are of similar
    width -> `Framing::CentreContent` (see the decision of 2026-10-23 below). CentreContent: R1 (`plan_square`) cuts a square as wide as the content's longer side, no
    margin, centred on the content: equal padding either side, aligned on nothing. Trim 2 is NOT needed for this (on 41 and 30
    it did move; the user had expected it not to).
  * **Frame**: EIX `0000`, trim 1 cut all four sides, trim 2 did not move, trim 1's box is square
    (`FRAMING_SQUARE_TOLERANCE` 2%), and it's NOT the product itself (not within
    `TRIM_SEGMASK_TOLERANCE_PX` = 8 of the mask box) and contains the mask -> peel: `deepen_peel` goes on
    inward while a line is >=99% frame colour outside the mask (+1 line of erosion), then `original`,
    `working` and the mask are cropped and only the edge check (`judge`) runs again. 45 becomes
    EIX 0010 with the blade running off the bottom, as the user wanted.
  * Everything else: `Keep`, R1 as before.
* First run on the 49 set (before the 2026-10-21 decisions below): Peel on 45; CentreContent on 3 (R2
  route, no effect), 19, 33, 35, 41, 46. Now: CentreContent on 3, 33, 35, 41, 46; 19 is Keep.
* **Decided 2026-10-21 (images 19, 41, 46, and the margin):**
  * Added without being asked, and kept: `FRAMING_BARS_SYMMETRY` (0.3). The bars rule also needs the two
    cut sides to be of similar width; 19 is 0.05, real bars 0.5-1.0. 19's "bars" are plain backdrop, so
    it's `Keep`: bottom pinned, subject centred.
  * R1 margin: one free side of an axis gets 4.2%, two free sides 8.4%, both pinned none
    (`plan.rs`, spec §5 example 28 redone). It changed 18, 19, 22, 28, 29, 32, 34, 41, 45, 47.
  * (Superseded 2026-10-23, see below.) Bars with ONE touched edge and smooth content were cut off
    (`Decision::Crop`) and R1 stretched the background; busy content (46) was centred.
* Clearance (20 working px) and the other framing thresholds: kept, no problem measured. The user allowed
  tuning if one shows up.
* The stale `jbPAUSE` tags and "unwired" comments are removed (the Part C reframe is wired and decided by
  `core::framing`); none was turned into a `jbpair`.
* The user suggested erosion/dilation to plug holes in trim 1's region if a trim failed; not needed so
  far (`product_box` already closes with a 2x2 window).

## Decided 2026-10-23: bars are always centred (user)
* `Decision::Crop` is gone. Every bars case (trim 1 cut both sides of one axis, similar widths, subject
  touches an edge) is `Centre`: the trimmed content is centred with equal white bands. Only 41 changed
  in the 49 set; 46 was already centred. The texture input of `framing::decide` went with it.
* Why: lifestyle / ambiance backgrounds (41, 46) could not be stretched convincingly. Tried in scratch
  (not in the tree): an even stretch with nothing invented; a stretch with per-pixel caps (+300% smooth,
  +50% busy); Criminisi patch fill for what is left, with a Poisson fit, a reuse penalty and measured
  grain. The result on 41 looked plausible at normal size, but seams, repetition or foreign content
  stayed visible when zoomed in.
* Findings worth keeping (scratch only, may come back): a plain even stretch with no mirror, blur or
  noise fixed the artefacts on 1, 22, 25-27, 36 and 48; blur follows the peak local magnification, so
  spread the stretch evenly. The Hough-line pin (lines near the mask that reach a border freeze that
  edge) fixed the snowboard on 41; it is not in the tree either.

## Decided 2026-10-23: one stretch function for all four edges, an even stretch (user)
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
* Fixes the glitch under the sandal on 22 (the vertical pass drew a striped block there), the mirror on
  1, the blur on 25-27 and 36's right side, and the rectangle on 49. 22's two flat blocks under the toe
  differ by 1 level and show only at 14x contrast.
* Open: the outermost source columns can carry a JPEG edge; a 3-4x stretch turns it into a faint
  stripe (seen on 22's right). Dropping 3 outer columns fixed that in the scratch prototype; not done.

## Fixed 2026-10-21 (user review of the rework)
* Image 1 had a horizontal step above the right shoe: the product band is one span per line, and the
  right shoe starts a few lines after the left one, so the band's edge jumped 575 px in one line.
  Fix: the band edge may now move at most `R1_BAND_SLOPE_PX_PER_LINE` (1) per line (only widening).
* Image 36's overhang: stretch read with a cubic (Catmull-Rom) instead of linear, and invented pixels
  fade from the mirrored detail into the neighbours' average colour with distance from the seam.
  Studio backgrounds got closer to the real noise level too (22: 0.61 -> 0.34, real 0.26).
* 45 (dark frame round the picture): fixed by the frame peel above. R1 had been inventing the bottom overhang
  from the frame bar.

## Rejected
* Raw-colour retry (adds another path).
* Blind zoom-to-65% retry.

## Parked: the first trim (code in commit `cc525bd`, not in the tree)
Replaced on 2026-10-13 by the trim above. Its extra rules (widen on a cut edge, keep step 1's box,
whole-frame guard, whole-frame retry, tolerance numbers) were not asked for and are not in the code.

## Open

### R1 stretch/inpaint follow-ups (2026-10-16)

* **Lifestyle / ambiance detection** — 2026-09-21-0 (snowboarder on mountains) still outputs
  white where the gated edges land, because the local-BGC fallback correctly refuses the
  textured strip. The user's hint was that preprocess's 16 border samples (`optimizer::border_samples`)
  plus the `prep.crop` fill-ratio can flag the image as lifestyle *before* R1 runs, after which
  the correct output is a per-edge content-aware stretch outside the segmask rather than white.
  **The current plan's write-up of these characteristics is likely a misinterpretation of what
  the user meant.** A dedicated planning conversation is needed — walk through the user's
  definition of lifestyle/ambiance (what the image looks like, what the segmask + border
  samples will show, what the desired output is) and agree on the routing *before* any code.
* **Weak spots of the 2026-10-21 rework:** gone with the even stretch (2026-10-23). Not tuned yet:
  clearance (20 working px).
* **Telea / fast-marching inpaint** — kept on standby. `center_and_stretch::inpaint::fill_segment` now
  only fills lines with no background at all (flat BGC/white). If a wider inpainter is needed it
  slots in there.
* **ONNX LaMa-style inpaint kicker** — for truly textured scenes (lifestyle), the long-term
  option is embedded like BiRefNet (`shot_classifier::birefnet` + `cipher` + `build.rs`).
* **BGC confidence** — `colors::sample` returns a binary pass/fail. R1 now judges per line whether
  the BGC is "close enough" (inside the nearby background's min/max), so a weak BGC is simply not
  believed there. A dominant-cluster fraction is no longer needed for R1.

### Older open items

* First run of the new trim on the 9 images in `JB core img batch` (old vs new exe, scratch): 8 give the
  same edge tags and colours. IMG_2619_indigo loses its mask (25.6% -> 0%): the crop (327x919 of
  672x1000) is found empty by the model, so BGC/FGC drop out; EIX stays 0000. On 99218793_det1 a soft
  gradient backdrop falls outside the sampled colour range, so the crop stays wide (836x965 of 1000x1000).
  **Note:** the trim may be under active refactor in another session (plan
  `~/.claude/plans/read-these-plans-in-elegant-teacup.md`) — confirm there before acting on this.
* Idea, **still relevant**: BiRefNet applies ImageNet normalisation itself, so the image gets
  normalised twice (colour balance, then ImageNet) — still implemented under the 512 model. Possible
  last-resort counter-move. Not started.
* **Stale (pre-512, int8).** The "change one input at a time on 102454579_det5 and read int8's best
  score" experiment — int8-specific, no longer relevant under 512.f