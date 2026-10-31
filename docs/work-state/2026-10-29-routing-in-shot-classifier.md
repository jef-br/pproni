# Decided 2026-10-29: routing lives in the shot classifier (user)

[Up: table of contents](TOC.md)

* All decisions about which route runs and which plan it gets are made in `shot_classifier::route`
  (`decide`), then `send` hands the image to the route. `processor::routes::{center_and_stretch,
  crop_square, fallback}` only change the picture. The R1 fall-backs to R3 (empty mask, no legal square) and
  the textured-scene choice (`R1Job::Reframe`) are decided there too; the stage-90 header and log show the
  route that ran. The square planner moved with it: `shot_classifier::plan`.
* One check stays inside R1 by the user's decision: the seam-carve quality gate of the textured-scene
  path, because it needs the pixels it has just made.
* New rule (user): a mask under 5% of the frame is still a subject when its longest side is at least
  `MASK_MIN_LONGEST_SIDE_PX` (733 = 800 − 8.4%, derived from `MIN_SIZE` and `R1_MARGIN`), measured in original
  pixels on the mask's box. Too small only when both say so. Changed 7 shoes (104051800/809/814/815/820/873/875_det3)
  from R2 to R1; nothing else.
* Checked against the committed build on 62 images (49 core + 13 shoes): 596 of 620 output files byte-identical;
  the only differences are those 7 shoes and the header label of 30, 43, 44 (R1 -> R3, same pictures).
* Three bled edges: no longer "R1 takes them". For a square subject every case with top+bottom or left+right bled
  (bar the two R2 cases) has no legal square and goes to R3 (30, 43, 44). `route.rs` has a test that says so.
* Open: diagram (`JBA2B.drawio.svg`) not yet patched for `route`/`plan`, waiting for the user to confirm the
  code. The architecture docs ([stages table](../architecture/01-pipeline.md), [module map](../architecture/02-modules-core.md),
  [R1/R2 table](../architecture/03-routing.md), `Route::select` mentions) and the `MASK_MIN_COVERAGE` row of
  the R2 table in [spec §5](../specs/05-routing.md) (line 117 of the old single-file spec) still describe the old layout.
* Open, PARKED 2026-10-29 for the morning: the user sees pproni "stuck" when headcutting is chosen. Not
  reproduced (batch, real console, and the desktop flow on a scratch Desktop all finish in ~15 s). Ask first:
  which screen, what was typed (`ask_yes_no` re-asks silently on anything but Y/N), which exe, does N work.
* Explained 2026-10-30, waiting for the user's decision: 104051820_det3, 104051873_det3 and 104051875_det3 came
  out uncropped (2000x2000) because the user's run used 2000 px copies of the shoes (the earlier outputs fed back
  in; `PPRONI-BACKUP` now holds those copies, not the 3872 px originals). At 2000 px their mask's longest side is
  633, 691 and 664 px, under 733, with area under 5%, so the rule calls them too small and R2 keeps the whole
  frame. At 3872 px (1226, 1339, 1286 px) the same photos pass and get the 4.2% crop. Same four-shoe check: 800,
  809, 814, 815 are 808, 746, 902, 883 px at 2000 and did get R1 (their output sizes match 1.084 x longest side).
  The rule measures absolute original pixels, so it depends on the input resolution. Not a code bug.
* Open: the GPU run sometimes loses most verdicts silently (`EIX=____`, route R3, about 36 s instead of 14 s):
  2 of 4 identical runs on 2026-10-29. Not looked into; the model error is swallowed in `core::classify`.
