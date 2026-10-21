# Pproni work state

Read this first. Keep it short. Update it at the end of each work session.
Last updated: 2026-10-21.

## Decided
* The 512 model (`birefnet_lite_512.onnx`) is canonical — proven to work, and what `build.rs`
  bakes in (`MODEL_FILE`). The 384 int8 export is a parked, untested speed experiment, kept only as
  the commented-out alternative in `build.rs`; 448 is out of scope.
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
* The 5% rule (`MASK_MIN_COVERAGE`) is the user's call. Never change it unasked. It is the only 5% rule.
* No retries, no guards, no fallbacks anywhere in the trim. Add none without asking.
* Goal now: improve results by refining the preprocessor, not by fine-tuning the model.

## Decided 2026-10-21: R1 stretch rework (user spec; code in `center_and_stretch/compose.rs`)
* One line at a time (a row in pass 1, a column in pass 2; pass 2 is the same code on the transposed
  picture). Product band = silhouette grown by one clearance (`R1_FILL_SOURCE_CLEARANCE_WORKING_PX`, a
  disc, per row and per column). It replaces the old `safety` + `clearance` pair.
* The source on each side is stretched: 0% next to the product, up to +100% at the border, curve
  `x^(e*1.68)` (`R1_STRETCH_CURVE_POWER`). All of a line's stretch is only ~18% of its source length;
  the rest of the overhang is invented. A line's stretch is averaged with its neighbouring lines and
  never exceeds what that line needs.
* Invented pixels: proposal = the mirrored stretched pixel (or a neighbour's colour when the source is
  shorter than the neighbour radius), clamped to the background's min/max. When the BGC lies inside
  that range it is the boxplot median, and the proposal is averaged with the reference value at its
  quantile (quantile from brightness, one for all channels). The reference is the background of the
  lines within 3 x radius, not the whole side: a whole-side reference amplified noise 3-12x on
  gradient backdrops (measured on 22).
* Removed: gating (`gated`, BGC tolerance, local-BGC fallback, `classify_edge`, the stage-90 strip overlay).
  BGC is now sampled on every image.
* Letterbox/bar detector moved into the preprocessor (`Prepared::letterbox`), per plan Part C: an
  opposite pair `FlatOther` and the other pair `Varied`; texture outside `product_box` on the trimmed
  interior. Stage image 02 shows it. Consequence: same-colour bars read as backdrop, so the detector
  misses 41, 46 (and slightly 30, 33, 35). That is now handled by `core::framing` (next section).

## Decided 2026-10-21: the trims and framing (`optimizer::optimize_both`, `core::framing`)
* **One model run.** BiRefNet is shown trim 1's backed-off crop, once. Its mask is the one reliable fact
  about the picture; nothing re-runs the model (the earlier guard that re-ran it on trim 2's crop was
  dropped: it changed 24 and 37 by 1-2 px only).
* **Trim 2** (`recheck_box`) re-runs the trim on trim 1's tight box; it counts as "moved" when any side
  differs by MORE than `TRIM_RECHECK_TOLERANCE_PX` (2). It is information only (`Prepared::trims`).
  Stage image 03: trim 1 cyan, trim 2 yellow (only when it moved), the crop green.
* **Framing** (`core::framing::decide`, after the model run, pure and unit-tested; the reason is one
  log line per image, `framing: ...`, in `<n>  -  log.txt`):
  * **Bars**: trim 1 cut both sides of ONE axis (the other axis loses at most `FRAMING_OTHER_AXIS_SLACK`
    = 2% in total), the subject touches an edge (EIX not `0000`) and the two cut sides are of similar
    width -> `Crop` (one edge, smooth content) or `Framing::CentreContent` (otherwise; see the decisions
    below). CentreContent: R1 (`plan_square`) cuts a square as wide as the content's longer side, no
    margin, centred on the content: equal padding either side, aligned on nothing. Trim 2 is NOT needed for this (on 41 and 30
    it did move; the user had expected it not to).
  * **Frame**: EIX `0000`, trim 1 cut all four sides, trim 2 did not move, trim 1's box is square
    (`FRAMING_SQUARE_TOLERANCE` 2%), and it is NOT the product itself (not within
    `TRIM_SEGMASK_TOLERANCE_PX` = 8 of the mask box) and contains the mask -> peel: `deepen_peel` goes on
    inward while a line is >=99% frame colour outside the mask (+1 line of erosion), then `original`,
    `working` and the mask are cropped and only the edge check (`judge`) runs again. 45 becomes
    EIX 0010 with the blade running off the bottom, as the user wanted.
  * Everything else: `Keep`, R1 as before.
* First run on the 49 set (before the 2026-10-21 decisions below): Peel on 45; CentreContent on 3 (R2
  route, no effect), 19, 33, 35, 41, 46. Now: Crop on 41; CentreContent on 3, 33, 35, 46; 19 is Keep.
* **Decided 2026-10-21 (images 19, 41, 46, and the margin):**
  * Added without being asked, and kept: `FRAMING_BARS_SYMMETRY` (0.3). The bars rule also needs the two
    cut sides to be of similar width; 19 is 0.05, real bars 0.5-1.0. 19's "bars" are plain backdrop, so
    it is `Keep`: bottom pinned, subject centred.
  * R1 margin: one free side of an axis gets 4.2%, two free sides 8.4%, both pinned none
    (`plan.rs`, spec §5 example 28 redone). It changed 18, 19, 22, 28, 29, 32, 34, 41, 45, 47.
  * Bars with ONE touched edge and smooth content (`Decision::Crop`): the bars are cut off, the verdict
    is kept (EIX cannot flip), R1 frames the content: 41 gets the bottom pinned, 4.2% on top, the
    background stretched over the whole canvas. Busy content (46: texture 69.5, 41: 10.0, threshold
    `T_TEXTURE_HIGHPASS` 12.5) stays `Centre` with white padding: stretching it smeared the photo.
    Two or more touched edges stay `Centre` (3, 33, 35).
* Clearance (20 working px) and the other framing thresholds: kept, no problem measured. The user allowed
  tuning if one shows up. 41's texture (10.0) is only 2.5 under the threshold.
* The stale `jbPAUSE` tags and "unwired" comments are removed (the Part C reframe is wired and decided by
  `core::framing`); none was turned into a `jbpair`.
* The user suggested erosion/dilation to plug holes in trim 1's region if a trim failed; not needed so
  far (`product_box` already closes with a 2x2 window).

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
* Recalibrating the int8 model (384 fp32 is no better).

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
* **Weak spots of the 2026-10-21 rework** (run on the 49-image set, scratch): the strip under the feet
  on 22 (small source, shadow gets mirrored: blotchy), and rough invented noise 2-3x the real
  background's on smooth studio backdrops (invisible below 1 level). Not tuned yet: clearance (20
  working px), neighbour radius (24 px), reference window (3x radius).
* **Telea / fast-marching inpaint** — kept on standby. `center_and_stretch::inpaint::fill_segment` now
  only fills lines with no background at all (flat BGC/white). If a wider inpainter is needed it
  slots in there.
* **ONNX LaMa-style inpaint kicker** — for truly textured scenes (lifestyle), the long-term
  option is embedded like BiRefNet (`shot_classifier::birefnet` + `cipher` + `build.rs`).
* **BGC confidence** — `colors::sample` returns a binary pass/fail. R1 now judges per line whether
  the BGC is "close enough" (inside the nearby background's min/max), so a weak BGC is simply not
  believed there. A dominant-cluster fraction is no longer needed for R1.

### Older open items

* **Stale (pre-512, 384/int8-era) — kept only as a record.** Eight images where the 384 int8 model
  found nothing (847_det0, 579_det4, 579_det5, 648_det5, 653_det4, 759_det0, 759_det1, 812_det5) and
  the 579_det5 int8-vs-fp32 decoder-split findings (decoder split at D3→D1; int8 20-30% off fp32 in
  the encoder; fp32 found it on the raw trimmed crop, int8 never). No longer relevant under the 512
  model.
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
  score" experiment — int8-specific, no longer relevant under 512.

## State of the repo
* On `main`: `refined-single-384-model` was merged (commit `2f3dcbb`). The trim is committed —
  `preprocessor::optimizer` and the `TRIM_*` consts are in `HEAD`, including a later MAD-based
  sampler refinement (commit `0dfd66c`). Uncommitted working-tree changes remain (the R1
  stretch/inpaint work: `center_and_stretch/inpaint.rs` is new/untracked, `compose.rs` and others
  modified).
* `models/birefnet_lite_384.onnx` (fp32, gitignored) is a leftover test file.
