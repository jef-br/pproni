# core::shot_classifier::plan

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/shot_classifier/plan.rs` comments. The sizing and placement rule
and its worked example (28.jpg) are [spec §5.1](../../specs/05-routing.md).

## One rule for every edge count
The per-axis size and placement rule is spec §5.1's. A subject that touches no edge is the
degenerate case of that rule, not a separate one: `Pins::NONE` goes through the same code.

## Why the square may reach past a border
The side is derived from the subject, not from the image. That is what lets the square reach past
a border, which is the case R1 exists for, not a failure mode. A side that lands inside the image
crops there; a side past it leaves an `Overhang` for the fill to cover.

## When `SquarePlan::new` returns `None`
An axis blocked at both ends that still comes up short: the *other* axis demanded a bigger square,
and there is nowhere legal to put the difference. Growing a blocked axis would invent background on
top of a subject that continues off-frame, and splitting the difference anyway would stretch the
product. R1 has no answer, so it says so, and the caller (`shot_classifier::route::decide`) sends
the image to R3, which frames it safely.
