# 6 · Crop & compose — CoG / MSR square `R2 ONLY`

[Up: table of contents](TOC.md)

**Scope narrowed 2026-09-09.** R1 no longer uses any of this: it anchors on the mask's bounding
box, pins bled edges and stretches background to fill ([§5.1](05-routing.md)). What follows is R2's machinery — the
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
