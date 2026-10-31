# 7 · Problem map

[Up: table of contents](TOC.md)

| # | Symptom | Status |
|---|---|---|
| P1 | Subject box creeps into hand / pants / shadow | Stale — not relevant since the 512 model; segmentation does not include shadow. |
| P2 | Subject box too loose | Addressed — [§4](04-resolution-policy.md): boundaries confirmed at full resolution. |
| P3 | Flat shot under-detected, box cuts in | Stale — not relevant since the 512 model; re-test against segmentation if it recurs. |
| feet | Head-to-toe model loses feet (fb_02 #9) | Stale — not relevant since the 512 model. |
| nonna | Real-life scene → double stretch | Fixed by R1 (`BUILT`): anchors on the mask bbox and stretches background, not the whole frame ([§5.1](05-routing.md)). [§6](06-crop-compose-r2.md)'s crop-in-first is R2-only, `DEFERRED`. |
