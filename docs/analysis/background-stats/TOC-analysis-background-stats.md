# Background statistics (outside the segmask)

[Up: analysis](../TOC-analysis.md)

Measured 2026-10-30 on the 49 core images and the 13 shoes (3872 px). Blobs, colour spread and
texture of everything outside the BiRefNet mask. Rows are sorted flattest first (by block std p90).

## Frozen snapshot (user, 2026-11-20)
We measured this once and do not re-run it. The model, the routes and the image set have changed since (GPU build
merged 2026-11-19; cores 50-51 and the loafers `104388854_det3` are not in it).
* **Found:** 11 of the 49 core images have a perfectly flat background (block std p90 of 0.20 or less). The busiest are
  41, 3, 46, 36 (p90 above 10) and 40, whose stats were of the whole picture because its mask was empty.
  Cores 6 and 40 had an empty mask: 6 still has one, 40 now has a good one (see their notes in the core table).
* **Status:** the flat-or-textured question is now decided in the code by a rough-block gate, not by these numbers
  ([flat-edge-gradients note](../../work-state/2026-11-13-flat-edge-gradients.md)).
* **If the problem returns** we need new images that have: a background these numbers call flat (p90 under 0.5, no
  blobs) that still comes out streaky or uneven after R1; or a textured background that the gate calls flat. Send the
  original and its segmask, then run `scripts/bgstats.py` on them (originals are in `JB core img batch`).

* [How the numbers are made](method.md)
* [Shoes (13)](shoes.md)
* [Core set (49)](core-set.md)

## Files
* `data/core.csv` (49 images), `data/shoes.csv` (13 shoes): every measured column, not only the
  ones in the tables. `data/core_test.csv` is an earlier test run on 45 core images (6-9 missing).
* `scripts/bgstats.py` makes a CSV from images + masks (usage in its docstring).
  `scripts/mkreport.py` turned the two CSVs into the single report these pages were split from; it
  does not write the thumbnail and file columns.
* `images/` (local only, not in git): `ov_core/`, `ov_shoes/` overlays (grey = mask grown by 2%,
  red/cyan = blobs); `masks_core/`, `masks_shoes/` the `run_dir` output, segmask and refine
  snapshots; `shoes_in/` the 13 shoe originals; `sheet1.jpg`, `sheet2.jpg` contact sheets of overlays.

![sheet1](images/sheet1.jpg)
![sheet2](images/sheet2.jpg)
