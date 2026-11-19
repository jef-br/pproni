# How the numbers are made

[Up: background statistics](TOC.md)

* Background = everything outside the BiRefNet mask, after growing the mask by 2% of the long side (drops the halo and contact shadow next to the subject).
* All brightness values are Lab L on a 0-255 scale. Colour spread is the robust spread (MAD x 1.4826), so a few specks do not move it.
* **drift** = std of the smooth background level (vignette, gradient, light falloff).
* **mid** = robust sigma of detail between 2 px and about 3% of the image (fabric, dust, scratches, texture).
* **grain** = robust sigma of detail finer than 2 px (sensor and JPEG noise).
* **blk std** = standard deviation of 16x16 blocks, taken at a common ~800 px scale. p50 = typical, p90 = the busiest tenth.
* **blob** = connected patch of at least 4 px whose brightness differs from the local background by 8 (visible) or 20 (strong) L levels, counted per megapixel of background. **elongated** = long side >= 25 px and aspect >= 4 (scratch or hair shape).
* Blob counts are in native pixels, so a 3872 px shoe and an 800 px image are not directly comparable.
