# Decided 2026-10-21: the trims and framing (`optimizer::optimize_both`, `core::framing`)

[Up: table of contents](TOC.md)

* **Only run birefnet model once per image.**
* **BiRefNet is shown a trim-backed-off crop of the original image**
* **Trim 2 (`recheck_box`) re-runs the trim on trim 1's tight box** It counts as "moved" when any side
  differs by MORE than `TRIM_RECHECK_TOLERANCE_PX` (2). it's information only (`Prepared::trims`).
  Stage image 03: trim 1 cyan, trim 2 yellow (only when it moved), the crop green.
* **Framing** (`core::framing::decide`, after the model run, pure and unit-tested; the reason is one
  log line per image, `framing: ...`, in `<n>  -  log.txt`):
  * **Bars**: trim 1 cut both sides of ONE axis (the other axis loses at most `FRAMING_OTHER_AXIS_SLACK`
    = 2% in total), the subject touches an edge (EIX not `0000`) and the two cut sides are of similar
    width -> `Framing::CentreContent` (see [the decision of 2026-10-23](2026-10-23-bars-always-centred.md)). CentreContent: R1 (`plan_square`) cuts a square as wide as the content's longer side, no
    margin, centred on the content: equal padding either side, aligned on nothing. Trim 2 is NOT needed for this (on 41 and 30
    it did move; the user had expected it not to).
  * **Frame**: EIX `0000`, trim 1 cut all four sides, trim 2 did not move, trim 1's box is square
    (`FRAMING_SQUARE_TOLERANCE` 2%), and it's NOT the product itself (not within
    `TRIM_SEGMASK_TOLERANCE_PX` = 8 of the mask box) and contains the mask -> peel: `deepen_peel` goes on
    inward while a line is >=99% frame colour outside the mask (+1 line of erosion), then `original`,
    `working` and the mask are cropped and only the edge check (`judge`) runs again. 45 becomes
    EIX 0010 with the blade running off the bottom, as the user wanted.
  * Everything else: `Keep`, R1 as before.
* First run on the 49 set (before the 2026-10-21 decisions further down this page): Peel on 45; CentreContent on 3 (R2
  route, no effect), 19, 33, 35, 41, 46. Now: CentreContent on 3, 33, 35, 41, 46; 19 is Keep.
* **Decided 2026-10-21 (images 19, 41, 46, and the margin):**
  * Added without being asked, and kept: `FRAMING_BARS_SYMMETRY` (0.3). The bars rule also needs the two
    cut sides to be of similar width; 19 is 0.05, real bars 0.5-1.0. 19's "bars" are plain backdrop, so
    it's `Keep`: bottom pinned, subject centred.
  * R1 margin: one free side of an axis gets 4.2%, two free sides 8.4%, both pinned none
    (`plan.rs`, [spec §5](../specs/05-routing.md) example 28 redone). It changed 18, 19, 22, 28, 29, 32, 34, 41, 45, 47.
  * (Superseded 2026-10-23, see [bars are always centred](2026-10-23-bars-always-centred.md).) Bars with ONE touched edge and smooth content were cut off
    (`Decision::Crop`) and R1 stretched the background; busy content (46) was centred.
* Clearance (20 working px) and the other framing thresholds: kept, no problem measured. The user allowed
  tuning if one shows up.
* The stale `jbPAUSE` tags and "unwired" comments are removed (the Part C reframe is wired and decided by
  `core::framing`); none was turned into a `jbpair`.
* The user suggested erosion/dilation to plug holes in trim 1's region if a trim failed; not needed so
  far (`product_box` already closes with a 2x2 window).
