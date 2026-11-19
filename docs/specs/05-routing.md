# 5 · Detection outcomes & routing `BUILT`

[Up: table of contents](TOC.md)

Three routes, keyed on the edge-intersection (EIX) verdict:

| Route | Verdict | Strategy |
|---|---|---|
| **R1** · Center & Stretch | a usable mask with room to grow | square around the subject, bled edges pinned, background stretched to fill |
| **R2** · CropSquare | no usable mask, or no room to grow | largest square already in the frame, centred |
| **R3** · Fallback | EIX not set — no verdict | whole image, uncropped, centred on a square canvas |

**KO images never reach routing.** A source deemed unprocessable — **KO** ([§9](09-open-questions.md) Q7: the trimmed image,
or its segmask, has a largest side below `800 / 1.42 ≈ 563 px`) — is a *failure*, not a route. It is
left untouched in the INPUT folder, exactly like the backdrop-only case. Route selection only ever
sees processable images.

R2 takes three verdicts:

| verdict | why R1 cannot | example |
|---|---|---|
| mask covers < `MASK_MIN_COVERAGE` of the frame | nothing to frame around | 6 (0.0%), 30 (4.4%) |
| EIX `0101` — left and right bled, nothing else | the square cannot be wider than the frame, and the subject is taller than that | 3 |
| EIX `1111` — all four bled | nowhere to put a margin, no band to stretch from | none in this set |

Left+right crops but top+bottom does not, and the asymmetry is not about the axis: left+right bled
means the square is capped at the frame width while the subject is taller, so something must be
cropped. Top+bottom bled means the square is as tall as the frame and the width has room to stretch
into — which R1 does well (4, 35, 36). A *third* bled edge hands the image back to R1, because the
one remaining free edge is somewhere to put the slack (29).

## 5.1 One rule, not six behaviours `REVISED 2026-09-09`

The five bleed behaviours this table used to list separately are one rule with different edges
pinned. **An edge the subject bleeds off is blocked**: the square may not move it, because the
subject continues past it and background invented there would be painted over a subject we cannot
see. Everything else follows, per axis:

- **Size.** Each free side of an axis takes half the margin (4.2% of the extent): two free sides need
  `extent × (1 + margin)`, one free side `extent × (1 + margin / 2)`. An axis blocked at
  both ends needs exactly `extent` — there is no gap to leave and nothing to leave it with. The
  square's side is the larger of the two needs.
- **Placement.** An axis's slack goes to its free sides: split evenly when both are free, all of it
  to the one that is free otherwise.

| edges | what the rule produces | old wording |
|---|---|---|
| 0 | both axes take the margin, both split it evenly | free-standing |
| 1 | that edge pinned, 4.2% on the opposite side | flush to that edge, centre the other axis |
| 2 opposite | that axis takes no margin and fills exactly | fill that whole axis |
| 2 adjacent | both pinned, slack onto the two free sides | flush into the shared corner |
| 3 | slack entirely onto the one free side | fill the boxed-in axis |
| 4 | — no free side, no background: R2 | fully bled → CoG/MSR |
| L+R | — capped below what the subject needs: R2 | (not previously listed) |

All of rows 0–3 live in R1, and none of them reaches route selection. Only the full bleed is
genuinely different: the subject's box *is* the frame, so there is nowhere legal to put the slack
*and* no background band anywhere to stretch from. Both of R1's mechanisms fail at once, which is
what makes R2 a route rather than a branch.

**Worked example — 28.jpg.** 980×1470, model seen from the back, the skirt running off the
bottom, EIX `0010`. The mask measures 692×1416 flush against the bottom edge.

```
vertical need   = 1416 × 1.042 = 1475   # one free side (the top): it gets 4.2%, not the two-sided 8.4%
horizontal need =  692 × 1.084 =  750
side            = 1475
```

Bottom pinned at y=1470, so all 59px of vertical slack lands above her head: 54px of real backdrop
already in frame plus 5px of new canvas, stretched from the band above her head. Horizontally the
square is centred on the mask and overhangs both sides (236px left, 259px right), filled from the
studio paper. The cut-off skirt is reproduced exactly — never stretched, never cropped.

**Impossible case.** An axis blocked at both ends whose square still comes up short — a wide
subject bleeding off the top and bottom of a shallow frame. Neither vertical side may move and the
slack has nowhere legal to go, so R1 declines and the image falls to R3.

> **Data note.** Measured over 112 images with segmentation: **84** touch 0 edges, **12** touch 1,
> **12** touch 2, **2** touch 3, **2** touch 4. So R1 covers 110 of 112, and R2's remaining case is
> the 2 full bleeds.

R3 has no row here. It is the answer to "we don't know", and it exists because a model failure must
not be silently processed as a clean free-standing shot.
