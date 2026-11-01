# core::shot_classifier::route

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/shot_classifier/route.rs` comments. The routes, R2's verdicts and
why the bleed cases stay inside R1 are [spec §5 and §5.1](../../specs/05-routing.md). The decision to
route here is [Decided 2026-10-29: routing lives in the shot classifier](../../work-state/2026-10-29-routing-in-shot-classifier.md).

Spec §5 and 03-routing.md still describe the older layout; see the open item in
[2026-10-29-routing-in-shot-classifier.md](../../work-state/2026-10-29-routing-in-shot-classifier.md).

## R3 in the current code
R3 `Route::Fallback` takes "no verdict, or no square can be planned": it fits the whole image into a
square canvas. Spec §5's R3 row still says only "no verdict"; the R1-declines case is §5.1's
"Impossible case".

## What reaches `decide`
Whether framing is possible. R2 is the answer to "there is nothing here to frame around, or no room
to do it in". R3 is "we cannot say where the subject is" or "no square respects the bled edges":
nothing is cropped and nothing is guessed.

## R1's square under white bars (`plan_square`)
Normally the square is planned from the subject by `SquarePlan::new`: the edges the subject bleeds
off are the ones the square may not move. When `core::framing` found white bars on one axis
(`Framing::CentreContent`), the square is cut from the content box instead:

* as wide as the content box's longer side, with no margin;
* centred on the box: equal padding either side of the shorter axis, aligned on nothing (not on
  the subject);
* the content box is in working coordinates and is scaled to the original, one factor per axis.

Whatever of the square falls outside the picture is filled as always.
