# Background statistics (outside the segmask)

[Up: analysis](../TOC-analysis.md)

Measured 2026-10-30 on core images 1-49; cores 50-51 added 2026-11-25. Blobs, colour spread and texture of
everything outside the BiRefNet mask. Rows are sorted flattest first (by block std p90).

## Re-measured 2026-11-25
Re-run on Pproni 2.4.0 (commit `2830fc3`, model `birefnet_512_nodeform_op19`) with the same script, the same
images and fresh masks from `run_dir`. Every number for the 49 core images came out the same, byte for byte, so
the table is still current. The first run (2026-10-30) already used this model, on the GPU branch.
* **New:** cores 50 and 51 were measured too and are in the core table. 50 (hay) has block std p90 29.2, the busiest
  of all; 51 (wrinkled paper) has 0.61. The loafers `104388854_det3` are not in it (no copy on disk).
* **Found:** 11 of the 51 core images have a perfectly flat background (block std p90 of 0.20 or less). The busiest are
  41, 3, 46, 36, 50 (p90 above 10) and 40, whose stats were of the whole picture because its mask was empty.
  Cores 6 and 40 still get an empty mask from `run_dir`, which gives the model the whole image (see their notes
  in the core table for what Pproni itself does).
* **Status:** the flat-or-textured question is now decided in the code by a rough-block gate, not by these numbers
  ([flat-edge-gradients note](../../work-state/2026-11-13-flat-edge-gradients.md)).
* **If the problem returns** we need new images that have: a background these numbers call flat (p90 under 0.5, no
  blobs) that still comes out streaky or uneven after R1; or a textured background that the gate calls flat. Send the
  original and its segmask, then run `scripts/bgstats.py` on them (originals are in `JB core img batch`).

* [How the numbers are made](method.md)
* [Core set (51)](core-set.md)

## Files
* `data/core.csv` (51 images): every measured column, not only the ones in the table.
* `scripts/bgstats.py` makes a CSV from images + masks (usage in its docstring).
* `images/` (local only, not in git): `ov_core/` overlays (grey = mask grown by 2%, red/cyan = every pixel 8 or
  more L levels darker/lighter, with no 4 px minimum); `masks_core/` the `run_dir` output, segmask and refine
  snapshots; `sheet1.jpg` a contact sheet of overlays.

![sheet1](images/sheet1.jpg)
