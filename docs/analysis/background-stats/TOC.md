# Background statistics (outside the segmask)

[Up: analysis](../TOC.md)

Measured 2026-10-30 on the 49 core images and the 13 shoes (3872 px). Blobs, colour spread and
texture of everything outside the BiRefNet mask. Rows are sorted flattest first (by block std p90).

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
