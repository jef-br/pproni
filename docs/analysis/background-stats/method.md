# How the numbers are made

[Up: background statistics](TOC-analysis-background-stats.md)

* Background = everything outside the BiRefNet mask (mask pixels above 127), after growing the mask by 2% of the long side (drops the halo and contact shadow next to the subject).
* **Working scale** = the image shrunk by a whole factor (long side ÷ 800, rounded down), so its long side is 800 to 1599 px. Smaller images are not shrunk.
* **Smooth level** = a 25 px median of L at working scale (3.1% of the long side at 800 px, 1.6% at 1599 px).
* **Residual** = the image minus the smooth level.
* All brightness values are Lab L on a 0-255 scale.
* **bg colour** = the median RGB of the background.
* **L spread** = the robust spread of L (MAD x 1.4826), so a few specks do not move it.
* **drift** = std of the smooth level (vignette, gradient, light falloff).
* **mid** = robust sigma of the residual after a 1.5 px blur: detail coarser than about 2 px and finer than the 25 px median (fabric, dust, scratches, texture).
* **grain** = robust sigma of detail finer than 2 px (sensor and JPEG noise).
* **blk std** = std of the residual in 16x16 blocks at working scale, only blocks fully inside the background. p50 = typical, p90 = the busiest tenth.
* **blob** = connected patch of at least 4 px, at least 2 px inside the background, where the residual (blurred by 1 px) is 8 (visible) or 20 (strong) L levels darker or lighter; counted per megapixel of background.
* **elongated** = counted at 8 levels only, dark and light added, among blobs of 20 px or more (the biggest 3000): long side >= 25 px and aspect >= 4 (scratch or hair shape).
* **biggest blob px** = area of the largest 8-level blob, dark or light.
* Blob counts are in native pixels, so a 3000 px image and an 800 px image are not directly comparable.
