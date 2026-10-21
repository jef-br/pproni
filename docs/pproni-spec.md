# Pproni — Design Spec & Source of Truth

*living document · maintained by Claude from your comments + chat · v1 · 2026-08-26*

This is the single place the target Pproni behaviour is written down. It supersedes the flow
diagrams for "what it should do". Status tags mark reality vs intent. Comment on any line;
I fold changes back in here.

**Legend:** `BUILT` · `BROKEN` · `PLANNED` · `CONFIRM`

---

## 1 · Principles & hard invariants

- **KPI:** repositioning accuracy at high speed.
- **Product is never distorted.** Only background is ever stretched.
- **Real pixels first.** Prefer cropping into the real image over inventing pixels; stretch only when a real-pixel square can't be had.
- **Max background stretch = 42%** on any side. `SUSPENDED` — dropped from R1 pending evidence it is needed; if artefacts appear, the answer may be further cleaning, seam carving or inpainting rather than a hard cap.
- **Output:** 1:1 square, side ∈ [800, 2000] px.
- **Encode:** JPEG q90–q95, 4:4:4 (no chroma subsampling), embedded sRGB ICC. `BUILT`
- **Failures** leave the input untouched (never deleted), never crash the batch.

---

## 2 · Pipeline overview

Five stages. The Shot Classifier decides which route an image takes; everything downstream hangs
off that one decision.

| Stage | Does | Status |
|---|---|---|
| **A · Load** | Decode, apply EXIF orientation, read the embedded ICC profile. | `BUILT` |
| **B · Preprocess** | Flatten alpha onto white, colour-manage to sRGB, derive the working-resolution copy, then run the optimiser: colour-balance (Levels, once) then trim crop, producing the image the model sees. Detail in `docs/ARCHITECTURE.md` §"Why two resolutions". | `BUILT` |
| **C · Classify** | Segment the working copy, then decide per edge whether the subject reaches it. Emits the EIX verdict — or no verdict. | `BUILT` |
| **D · Dispatch** | EIX verdict → one of three routes (§5). | `BUILT` |
| **E · Route** | R1/R2 shape the square; R3 frames it safely. | `BUILT` |
| **F · Export** | Size envelope, then JPEG + ICC, then delete the source on success. | `BUILT` |

Segmentation produces the mask and the edge verdict in one pass — no intermediate box to find and
then classify.

---

## 3 · Shot Classifier (SC) — the brain `BUILT`

SC runs on the working-resolution copy and answers one question: **which image edges does the
subject actually reach?** That verdict is what routing keys on (§5).

A bounding box cannot answer it reliably — a box can touch an edge while the silhouette does not,
and vice versa for a diagonal pose. So the answer comes from segmentation, not geometry.

### 3.1 How it works

BiRefNet (dichotomous image segmentation, ONNX) produces one foreground mask for the whole image —
no boxes, no classes, no NMS. The edge verdict is then taken in two passes:

- **Pass 1 · gate.** Scan only a `GATE_MARGIN_PX`-wide band along each edge of the mask. A hit means
  "worth checking precisely", *not* "touches": the mask is upsampled from the model's internal
  resolution and is not trusted for the verdict.
- **Pass 2 · refine.** For each gated edge, intersect the mask with the border band, crop that region
  at full resolution, and build a trimap — interior foreground, exterior background, a ring around
  the boundary marked unknown. Classify the unknown ring by colour distance to sampled foreground
  and background. The refined mask, not the raw mask, answers touching or not.

Pass 2 does real work at every scale. When the working copy equals the original there is no
upsampling error left to correct, but the colour matting on the unknown ring runs identically.

### 3.2 Output contract

`ShotClassification { touches_edges: Vec<Edge>, refinements: Vec<EdgeRefinement> }` — or **no
verdict**, when the model is unavailable or inference failed on that image.

Those are different states and they route differently (§5). This answers Q3.

### 3.3 Triggers

SC remains the place a Phase-2 human detector would hook in (people, head-to-toe framing).

---

## 4 · Resolution & bbox policy `BUILT`

- **Working size:** if the largest dimension is ≤ `WORKING_SIZE` (1024 px), the image is used as-is;
  larger images are reduced to 1024 on the long side. Never upscaled — upscaling would invent detail
  the model then reads as real.
- **Boundaries are resolved at full resolution.** Segmentation runs on the working copy for speed,
  but pass 2 re-decides the boundary against full-resolution pixels. Both resolutions travel together
  as `Prepared { original, working }` for exactly this reason. (This is the fix for the loose-box
  problem, P2.)

### 4.1 Edge-intersection detection `BUILT`

Edge intersection is the gate/refine result described in §3.1: a decision per edge taken on the
segmentation mask and confirmed at full resolution.

---

## 5 · Detection outcomes & routing `BUILT`

Three routes, keyed on the edge-intersection (EIX) verdict:

| Route | Verdict | Strategy |
|---|---|---|
| **R1** · Center & Stretch | a usable mask with room to grow | square around the subject, bled edges pinned, background stretched to fill |
| **R2** · CropSquare | no usable mask, or no room to grow | largest square already in the frame, centred |
| **R3** · Fallback | EIX not set — no verdict | whole image, uncropped, centred on a square canvas |

**KO images never reach routing.** A source deemed unprocessable — **KO** (§9 Q7: the trimmed image,
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

### 5.1 One rule, not six behaviours `REVISED 2026-09-09`

The five bleed behaviours this table used to list separately are one rule with different edges
pinned. **An edge the subject bleeds off is blocked**: the square may not move it, because the
subject continues past it and background invented there would be painted over a subject we cannot
see. Everything else follows, per axis:

- **Size.** An axis with at least one free side needs `extent × (1 + margin)`. An axis blocked at
  both ends needs exactly `extent` — there is no gap to leave and nothing to leave it with. The
  square's side is the larger of the two needs.
- **Placement.** An axis's slack goes to its free sides: split evenly when both are free, all of it
  to the one that is free otherwise.

| edges | what the rule produces | old wording |
|---|---|---|
| 0 | both axes take the margin, both split it evenly | free-standing |
| 1 | that edge pinned, the whole margin on the opposite side | flush to that edge, centre the other axis |
| 2 opposite | that axis takes no margin and fills exactly | fill that whole axis |
| 2 adjacent | both pinned, slack onto the two free sides | flush into the shared corner |
| 3 | slack entirely onto the one free side | fill the boxed-in axis |
| 4 | — no free side, no background: R2 | fully bled → CoG/MSR |
| L+R | — capped below what the subject needs: R2 | (not previously listed) |

All of rows 0–3 live in R1, and none of them reaches route selection. Only the full bleed is
genuinely different: the subject's box *is* the frame, so there is nowhere legal to put the slack
*and* no background band anywhere to stretch from. Both of R1's mechanisms fail at once, which is
what makes R2 a route rather than a branch.

**Worked example — 28.jpg.** 667×1000, model with the head cut off at the top, EIX `1000`.
The mask measures 474×986 flush against the top edge.

```
vertical need   = 986 × 1.042 = 1027    # one free side (the bottom): it gets 4.2%, not the two-sided 8.4%
horizontal need = 474 × 1.084 =  514
side            = 1027
```

Top pinned at y=0, so all 41px of vertical slack lands under the feet: 14px of real floor already
in frame plus 27px of new canvas, stretched from the band below her feet. Horizontally the square
is centred on the mask and overhangs both sides, filled from the studio paper. The cut-off head is
reproduced exactly — never stretched, never cropped.

**Impossible case.** An axis blocked at both ends whose square still comes up short — a wide
subject bleeding off the top and bottom of a shallow frame. Neither vertical side may move and the
slack has nowhere legal to go, so R1 declines and the image falls to R3.

> **Data note.** Measured over 112 images with segmentation: **84** touch 0 edges, **12** touch 1,
> **12** touch 2, **2** touch 3, **2** touch 4. So R1 covers 110 of 112, and R2's remaining case is
> the 2 full bleeds.

R3 has no row here. It is the answer to "we don't know", and it exists because a model failure must
not be silently processed as a clean free-standing shot.

## 6 · Crop & compose — CoG / MSR square `R2 ONLY`

**Scope narrowed 2026-09-09.** R1 no longer uses any of this: it anchors on the mask's bounding
box, pins bled edges and stretches background to fill (§5.1). What follows is R2's machinery — the
full bleed, where the subject's box is the frame and there is no background left to anchor against,
so saliency is the only signal remaining.

Anchors the square on where the salient mass actually sits, prefers real pixels, and only stretches
background within the 42% cap.

### 6.1 Terms

- **MSR** — Most Salient Region: the bounding box of the thresholded saliency heatmap. `[mx,my,mw,mh]`.
- **CoG** — saliency-*weighted* centroid of the MSR, `(cx,cy)`. (Weighted, so a brighter sub-region pulls it — this is why the CoG can sit off the geometric centre.)
- **W,H** — full image size. **margin** = 4.2% of the square side.

### 6.2 Algorithm

```python
# 1. Can a real-pixel square hold the whole MSR?
S_need   = max(mw, mh) * (1 + margin)      # square big enough to keep all of MSR
S_real   = min(W, H)                       # largest square of real pixels

if S_need <= S_real:
    # crop in — no new pixels. Slide a S_need square to centre on CoG, clamp inside image.
    S = S_need;  extend = 0                 # nonna & ~all cases land here
else:
    # 2. A real square would clip the MSR. Only grow if it clips too much.
    S_clip = S_real
    keep   = msr_fraction_kept_by(square=S_clip centred on CoG)
    if keep >= (1 - LOSS_MAX):
        S = S_clip;  extend = 0             # acceptable clip, just crop in
    else:
        S = S_need                          # keep the whole MSR -> must add canvas
        extend = S - S_free                 # S_free = image size on the FREE axis

# 3. On the CONSTRAINED axis (where S <= image): slide the square to centre on CoG,
#    clamp to [0, dim-S].
# 4. On the FREE axis: split the needed `extend` by CoG bias.
p        = cog_fraction_on_free_axis        # 0 = at min edge, 1 = at max edge
ext_min  = round(p * extend)                # side the CoG is NEAR gets less new canvas
ext_max  = extend - ext_min                 # side the CoG faces gets more
crop_free_start = -ext_min                  # crop overhangs the image by ext_min / ext_max

# 5. FILL the overhangs by stretching the background band on each side toward the edge.
#    stretch_side = (band + ext_side) / band  must be <= 1.42,
#    else this is the impossible case.
```

### 6.3 Diagram

```
        ext_min                            ext_max
     (small — CoG                       (large — CoG
       is near)                          faces here)
    <------->                          <------------->
    +--------+---------------+----------------------+   dashed = target square
    |::::::::|  image  W x H |::::::::::::::::::::::|   shaded = new canvas,
    |::::::::|   +-------+   |::::::::::::::::::::::|   filled by stretching
    |::::::::|   |  MSR  |   |::::::::::::::::::::::|   background <= 42%
    |::::::::|   | (*)CoG|   |::::::::::::::::::::::|
    |::::::::|   +-------+   |::::::::::::::::::::::|
    +--------+---------------+----------------------+
```

### 6.4 Worked example `CONFIRM the numbers`

From your comment, re-run so it's internally consistent (rounding at 4.2% margin):

```
image      2000 x 3000  (portrait)          W=2000 H=3000
MSR        [x 300, y 560, w 800, h 2400]    mw=800 mh=2400
CoG        (cx, cy)  — weighted, left-biased on x
S_need   = 2400 * 1.042           = 2501    # round -> 2500
S_real   = min(2000,3000)         = 2000
2500 > 2000  and a 2000-square clips >LOSS_MAX of the MSR  -> grow & extend
free axis = X (width);  extend = 2500 - 2000 = 500 px  (25% of width)
p (CoG on X) ~ 0.21  ->  ext_min ~ 105 (left) , ext_max ~ 395 (right)
crop     = [x -105, y 500, 2500, 2500]      # y fits in real pixels; x overhangs 105 / 395
fill     left band ~400px stretches ~1.26x , right band ~800px stretches ~1.37x
         -> both <= 1.42  OK
```

> **CONFIRM** (all dormant — §6 is `DEFERRED`, so none of these block anything today):
> 1. `LOSS_MAX` — you said **20%** in the rule but used **~10–17%** in the example. Pick one (default set to 20%).
> 2. The CoG number in your example (`x:200`) sits left of the MSR's own left edge (300); with a saliency-*weighted* CoG that's possible, but confirm it's weighted and not a slip.
> 3. margin applied to the square side (4.2%) vs to a single dimension — I used the square side.

### 6.5 Fill & resize `mostly BUILT`

- **Fill:** stretch only the background bands outward to the square; product band copied untouched. Grow anchored on the CoG of the MSR (not the geometric centre).
- **Resize:** uniform scale so the side lands in [800, 2000].

---

## 7 · Problem map

| # | Symptom | Status |
|---|---|---|
| P1 | Subject box creeps into hand / pants / shadow | Stale — not relevant since the 512 model; segmentation does not include shadow. |
| P2 | Subject box too loose | Addressed — §4: boundaries confirmed at full resolution. |
| P3 | Flat shot under-detected, box cuts in | Stale — not relevant since the 512 model; re-test against segmentation if it recurs. |
| feet | Head-to-toe model loses feet (fb_02 #9) | Stale — not relevant since the 512 model. |
| nonna | Real-life scene → double stretch | Fixed by R1 (`BUILT`): anchors on the mask bbox and stretches background, not the whole frame (§5.1). §6's crop-in-first is R2-only, `DEFERRED`. |

---

## 8 · Decisions locked

- SC is the brain, and it is segmentation-based: a BiRefNet mask plus a two-pass gate/refine edge
  verdict.
- **"No verdict" is a first-class outcome**, distinct from "touches no edge". It routes to R3.
- Working size ≤1024; boundaries always confirmed at full resolution.
- R1 is bbox-anchored: square = longest mask side + 4.2% per free side (8.4% when both sides of the axis are free, 4.2% when one is pinned, `R1_MARGIN = 2.0*0.042`), bled edges pinned, background stretched to fill. No saliency, no clipping of the subject, and the 42% cap suspended.
- CoG/MSR (§6) is R2's machinery now — the full bleed is the only case with no background to anchor against.
- Three routes, one bleed rule: pin the bled edges, margin the free ones. Rows 0-3 of the old edge table are that rule seen from four angles and all live in R1; only the full bleed is left to R2.
- Output 1:1 [800,2000], JPEG q90–95 4:4:4 sRGB. The envelope is enforced in the exporter — which
  every route ends at — so no route can bypass it.
- Ships as one hardened exe with no install. **Met** — the ONNX Runtime and the model are both
  embedded in the exe (the model encrypted), unpacked/decrypted at runtime, never shipped as files
  beside it. See `docs/ARCHITECTURE.md` §5.

---

## 9 · Open questions

- **Q1** · `LOSS_MAX` value (§6.4 ①) — R2 only now; R1 never clips the subject. **Dormant:** §6
  (CoG/MSR) is `DEFERRED`, so this blocks nothing until R2 saliency is built.
- **Q2** · CoG definition — saliency-weighted centroid confirmed? (§6.4 ②) **Dormant:** §6 is
  `DEFERRED`, so this blocks nothing until R2 saliency is built.
- **Q3** · ~~SC output contract~~ — **answered**, see §3.2.
- **Q5** · Human detector interface — what signal it returns and how §6 consumes it. **Parked:
  Phase 2** — not specified until that work starts.
- **Q6** · ~~R2's five behaviours~~ — **answered**: four of them are one rule and live in R1 (§5.1). R2 is left with the full bleed.
- **Q7** · ~~Small-source policy~~ — **resolved 2026-10-20 (`PLANNED`).** A source is **KO** when,
  after the trim, its largest dimension — or its segmask's largest dimension — falls below
  `800 / 1.42 ≈ 563 px` (the 800px output floor divided by the old 1.42× cap). A KO image is a
  failure: it stays in the INPUT folder, unprocessed, like the backdrop-only case. Not yet built.

---

## 10 · Build status

| Piece | State |
|---|---|
| Load / orient / flatten / sRGB | `BUILT` |
| Working-resolution copy | `BUILT` |
| Shot Classifier — BiRefNet + gate/refine | `BUILT` |
| Edge intersections | `BUILT` |
| Route selection (R1/R2/R3) | `BUILT` |
| R3 · Fallback | `BUILT` |
| R1 · Center & Stretch | `BUILT` |
| R2 · CropSquare | `BUILT` — largest centred square |
| CoG/MSR crop (§6) | `DEFERRED` — R2 crops frame-centred; no image needs saliency yet |
| Output size envelope | `BUILT` — in the exporter |
| Save JPEG + ICC | `BUILT` |
| Single hardened exe, no install | `BUILT` — model + runtime embedded; see `docs/ARCHITECTURE.md` §5 |
| Human detector | `PHASE 2` |

---

*Pproni source of truth · comment to change any line · I maintain it from your comments + chat*

*Source artifact: https://claude.ai/code/artifact/b3d70cd2-f493-4d01-992b-ace7709b5bc3*
