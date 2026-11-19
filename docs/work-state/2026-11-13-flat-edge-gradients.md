# Decided 2026-11-13: flat background on R1's invented edges: the flat stretch and the gradients (user)

[Up: table of contents](TOC-work-state.md)

Built on branch `flat-bg-gradients` from the user's hand-made targets (23 shoe photos) and pseudo-code. Merged into
`main` as version 2.4.0 on 2026-11-19; the branch is deleted. Code: `center_and_stretch/flatness.rs`, `compose.rs`,
`gradients.rs`, `grazes.rs`, `patch.rs`, `solid.rs`; how it works: [module page](../architecture/modules/core-processor-routes-center-and-stretch.md).

## One gate: is the background flat?
* `flatness::is_flat` decides two things. Flat: the stretch starts at the subject's box and the edge
  gradients are painted. Textured: today's stretch (core 36 as it is), no gradients.
* The measure counts rough blocks of pure background (no mask within clearance + 3 px). It replaced the
  mean high-pass of the old gate: core 25-27 scored 4.5 against 3.8 for images that should paint, too
  thin a gap. Rough-block share: those that should paint at most 0.05, 25-27 and the textured ones from 0.20,
  limit 0.10. Amplifying the contrast before measuring did not help; looking at a share of blocks did.
* `T_TEXTURE_HIGHPASS` stays as the preprocessor's own constant; the new limits are `R1_FLAT_*` in `config.rs`.
* With no block clear of the mask there is nothing to measure, and that counts as not flat (today's stretch).

## The flat stretch (user, 2026-11-13)
* On a flat background every edge stretches from the whole strip between the subject's box and the photo's
  border, full length: left/right edges from the box's left/right edge, top/bottom from its top/bottom edge.
* The bare box edge is used: the clearance round the product is not kept in this mode. (The user did not
  say; this is the literal reading of "the bbox edge".)

## Grazed lines (user, 2026-11-18)
* Core 23 got red streaks above the collar: the flat stretch's bare box left 2 px of collar in the source.
  The old grown band had hidden this (it ran off the border, so those lines were filled flat).
* Rule: on a flat background, where the product touches or grazes a flagged edge, those lines are not
  stretched. Blocked line = a mask pixel within `GATE_MARGIN_PX` of the border on it, plus `GATE_MARGIN_PX` either
  side. Each run of blocked lines is filled from the nearest unblocked lines either side (same colour: that
  colour, else a ramp; one side only: copy), with `inpaint::fill_segment_with_noise` at noise 0 (the user:
  noise must not be introduced on a flat background). Code: `grazes.rs`, `compose.rs`.
* The ring fill (ring by ring from the outer edge, ramp plus grain) was tried on cores 50 and 51 and
  rejected by the user: noise, no recurring pattern.

## The second test and the patch fill (user, 2026-11-18)
* Keep the rough-block gate (flat or textured stretch, gradients) as it is. A second test, at the fill stage of
  a grazed (unsafe) area, on either route: a background colour found, or the single safe edge pixel line
  with no variance within a small tolerance, means a flat colour or the exact ramp (no noise). Otherwise step
  2: clone / patch / heal.
* Step 1 is the safe area: stretched as before. Step 2 is the unsafe area, treated as a 2D gap, not as lines.
* Sources: the safe overhang, and real background outside the product and its margin. Never from inside the
  margin. No mirroring. Context for matching and healing: every real non-product pixel next to the gap.
* Tolerance 3 levels is my proposal (p5-p95 of the 5-px-smoothed edge line); the user has not confirmed it.
  Measured: flat cores 0-13, 51 top 11-14, 50 65-100.
* Resolved 2026-11-18/19: core 22's darker bottom (234 against 241-245) came from the gradients' one global
  colour, not from the BGC fill; fixed by the local colours under "The gradients". Core 51's jagged stretch beside
  the gaps is fixed by the solid-background look below.
* Code: `patch.rs`, `grazes.rs` (`Blocked::choose_fill`), `compose.rs` (`compose_with_holes`), `mod.rs`.

## The gradients
* Where the square reaches past the image, the stretched background fades into **one flat colour**:
  no streaks, no smeared shadows. Five zones: left, right, bottom, two bottom corners.
* Changed 2026-11-18 (user): a zone fades into the colour at its own edge, not one colour for all zones:
  `a` = mean of the edge band, `b` = mean of the outermost line (safe lines only); ramp from `a` at the
  band's inner edge to `b` at the canvas edge. Corners take the bottom's. Too few lines: the old whole-photo
  median (outside the mask, outside every zone, above the box's bottom). Never a colour from the product.
  Effect on the core set: 22 bottom 234 -> 241 (GPU 241-245), 1 loses its grey side bars, 34 and 18 barely move.
* A zone is painted only on a side the square had to invent (overhang above 0). Corners need the
  bottom and their side. No overhang anywhere: nothing changes.
* Bottom fade: ends `REFINE_NARROW_BAND_PX` under the box, full colour at 25% of the box height away
  (`R1_FLAT_EDGE_FADE_SHARE`) but never closer than `GATE_MARGIN_PX`. With no room for that: no bottom
  zones (core 22 gets 16 px, core 34 gets 31 px).
* `GATE_MARGIN_PX` and `REFINE_NARROW_BAND_PX` are used as they are, in original-image pixels. The user
  did not ask for them to be scaled by `working_scale`, so they are not.
* Top gradients are **not** desired (user, 2026-11-13).

## Review of the first run (user)
* Core 1-24 and 28-49 good. Core 25-27 wrongly got gradients (texture). Shoes: the shadow under the shoe
  still showed the old stretch's streaks. Fixed by the flat gate and the flat stretch above; a review run is pending.
* Core 17: parked by the user; it looked alright. Core 34 and 35 (overhang 7 and 2 px): alright.

## Open (2026-11-19)
* The tolerance of 3 levels for the second test is my proposal; the user has not confirmed it.
* The solid-background look's threshold (steepest step 0.5) rests on three images (34, 51, the loafers 104388854_det3).
* The diagram `JBA2B` was synced on 2026-11-19 (one node per struct and enum, each linked to its `.rs` file). Types
  used in another part of Pproni, like `Shot` or `Mask`, link only to their defining file; the long cross-lane lines
  are not drawn.

## The solid-background look (user, 2026-11-19)
* Core 51 (wrinkled paper) stayed botched after the patch fill: the jagged part is the stretched overhang, not the
  gaps. The user's idea: paint everything but the product in the predominant background colour, keeping the
  product (mask + 2 px, 1 px feather) and the soft shadow under it. The shadow band is the user's recipe: mask
  moved down 40 px minus the mask, 40 px blur down, 20 px Gaussian, 100% next to the mask, black ramp from 33% to
  66% of the mask's height. Sizes are shares of the canvas (2% and 1% of a 2000 px side).
* Isolation (user said yes): flat gate flat + no BGC + a grazed edge + steepest step above 0.5. Measured: 34 (smooth
  floor) 0.36, 51 0.64, loafers 104388854_det3 0.62. Other measures (colour share, luma spread) ranked 34 as less flat
  than 51 and were dropped. Only three images pass the first three tests, so the threshold rests on little data.
* Effect on the core set: only 51 and 104388854_det3 changed. Code: `solid.rs`, `flatness::crease_step`, `mod.rs`.

