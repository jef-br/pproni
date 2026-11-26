# 5 · Detection outcomes & routing `BUILT`

[Up: table of contents](TOC-specs.md)

Three routes, keyed on the edge-intersection verdict ([EIX](03-eix.md)):

| Route | Verdict | Strategy |
|---|---|---|
| **R1** · Center & Stretch | the subject must be repositioned | move and grow the canvas so the subject's box is centred; edges the subject runs off stay where they are; new canvas is filled from the background |
| **R2** · CropSquare | no usable mask, or no room to grow | largest square already in the frame, centred |
| **R3** · Fallback | the subject runs off two opposite edges and is too wide (or too tall) for a square; today also a model error, dropped without a word (ticket #31) | the whole image, uncropped, centred on a square canvas; white bands fill the rest |

**KO images will never reach routing** `PLANNED`, not yet built. A source that is too small — **KO**,
see [§7](07-open-questions.md) Q7 — will be a *failure*, not a route: left untouched in the INPUT
folder, as the backdrop-only case already is. Route selection will then only see processable images.

R2 takes three verdicts:

| verdict | why R1 cannot | example |
|---|---|---|
| mask covers less than `MASK_MIN_COVERAGE` of the frame, and its longest side is under `MASK_MIN_LONGEST_SIDE_PX` | nothing to frame around | 6 (0.0%), 40 (0.0%) |
| EIX `0101` — left and right bled, nothing else | the square cannot be wider than the frame, and the subject is taller than that | 3 |
| EIX `1111` — all four bled | nowhere to put a margin, no band to stretch from | none in this set |

Left+right crops but top+bottom does not, and the asymmetry is not about the axis: left+right bled
means the square is capped at the frame width while the subject is taller, so something must be
cropped. Top+bottom bled means the square is as tall as the frame and the width has room to stretch
into — which R1 does well (4, 35, 36). This only works when the blocked axis is already long enough
for the square; otherwise the square does not fit and the image goes to R3 (see *Impossible case*
below). A *third* bled edge follows the same rule: R1 when the square fits (29), R3 when it does not
(30, 43, 44).

## 5.1 One rule, not six behaviours

There is one rule that governs future behavior.  **An edge the subject bleeds off is blocked.**
The square may not move it, because the subject continues past it and background invented there would be a hallucination.
Everything else follows, per axis:

- **Size.** Each free side of an axis takes half of `R1_MARGIN`: two free sides need
  `extent × (1 + R1_MARGIN)`, one free side `extent × (1 + R1_MARGIN / 2)`. An axis blocked at
  both ends needs exactly `extent` — there is no gap to leave and nothing to leave it with. The
  square's side is the larger of the two needs.
- **Placement.** An axis's slack goes to its free sides: split evenly when both are free, all of it
  to the one that is free otherwise.

| edges | what the rule produces | old wording |
|---|---|---|
| 0 | both axes take the margin, both split it evenly | free-standing |
| 1 | that edge pinned, half of `R1_MARGIN` on the opposite side | flush to that edge, centre the other axis |
| 2 opposite | that axis takes no margin and fills exactly | fill that whole axis |
| 2 adjacent | both pinned, slack onto the two free sides | flush into the shared corner |
| 3 | slack entirely onto the one free side | fill the boxed-in axis |
| 4 | — no free side, no background: R2 | fully bled → CoG/MSR |
| L+R | — capped below what the subject needs: R2 | (not previously listed) |

Rows 0–3 go to R1 when the square fits, and to R3 when an axis blocked at both ends still
comes up short (see *Impossible case* below). Rows 4 and L+R go to R2. The full bleed is the
clearest case: the subject's box *is* the frame, so there is nowhere legal to put the slack *and* no
background band anywhere to stretch from. Both of R1's mechanisms fail at once, which is what makes
R2 a route rather than a branch.

**Worked example — 28.jpg.** 980×1470, model seen from the back, the skirt running off the
bottom, EIX `0010`. The mask measures 692×1416 flush against the bottom edge.

```
vertical need   = 1416 × (1 + R1_MARGIN / 2) = 1475   # one free side (the top): half of R1_MARGIN, not all of it
horizontal need =  692 × (1 + R1_MARGIN)     =  750
side            = 1475
```

Bottom pinned at y=1470, so all 59px of vertical slack lands above her head: 54px of real backdrop
already in frame plus 5px of new canvas, stretched from the band above her head. Horizontally the
square is centred on the mask and overhangs both sides (236px left, 259px right), filled from the
studio paper. The cut-off skirt is reproduced exactly — never stretched, never cropped.

**Impossible case.** An axis blocked at both ends whose square still comes up short — a wide
subject bleeding off the top and bottom of a shallow frame. Neither vertical side may move and the
slack has nowhere legal to go, so R1 declines and the image falls to R3.

R3 has no row here. It takes the images whose square does not fit (the impossible case above) and
frames them safely instead of guessing. Today it also takes an image whose model run fails. That is
an error, not a Pproni outcome, and `core::classify` drops it without a word. How to report it is
ticket #31.
