# Core image roles

[Up: agent knowledge](TOC-agent-knowledge.md)

What each image in `JB core img batch` is for. One row per image. A role without a prefix was set by the user. `Proposed:` means it was written from the picture, not yet confirmed by the user.

---
##Dictionary:
BG = background
FG = foreground
FGC = FG color
BGC = BG color
PC = Product color


---

| Image | File | Role |
|---|---|---|
| 1 | `1.png` | Set of (1-3): Transparent PNG input, soft shadows on all edges, Top and bottom of product close to canvas edge. White BG.|
| 2 | `2.png` | Set of (1-3): Transparent PNG input, shadows exiting the frame, Left and right of product close to canvas edge. White BG. |
| 3 | `3.png` | Set of (1-3): Transparent PNG input, half-transparent shadows, white product on a white BG. Tests EIX=0101 behavior. |
| 4 | `4.jpg` | Set of (4-6): Flat white BG, bright PC, hard to distinguish front (4) and back (5) of product. |
| 5 | `5.jpg` | Set of (4-6): Flat white BG, bright PC, hard to distinguish front (4) and back (5) of product. |
| 6 | `6.jpg` | Set of (4-6): No BG in shot, bright PC, hard-edge small gold badge. Mask comes out empty (= the point of this image) |
| 7 | `7.jpg` | Set of (7-9). Extremely low contrast between product and BG: the product has high contrast inside, but its edge has low contrast. Notably on the top-left of the product |
| 8 | `8.jpg` | Set of (7-9): same low-contrast edge test with product on the left. |
| 9 | `9.jpg` | Set of (7-9): Low-contrast edge test of packaging with similar FGC & BGC with visually dominant shadow on the bottom left |
| 10 | `10.jpg` | Set of (10-12): Product is high-contrast and multi-color, one of the FGC's has the same value as BGC. Tests segmask creation and bg stretching behavior. Sleeve and collar edge can cause gray streaks on the background |
| 11 | `11.jpg` | grey-streak image, same as 10 |
| 12 | `12.jpg` | Close-up of a red polo with a chest badge. No background visible. Partial mask influences position of crop area. Treated like image 6 but guided by the partial mask. The product fabric fills the whole picture, which makes it show up as BG in the segmask (also like 6.) |
| 13 | `13.jpg` | White-on-white with near-white shadow, more texture on the product than the background. Set with 14 and 15. |
| 14 | `14.jpg` | Proposed: three white no-show socks laid flat on near-white; white on white, with small gaps between the items. Segmask should pick up all 3 products. |
| 15 | `15.jpg` | white on near-white with soft shadow and harder constrat in product. |
| 16 | `16.jpg` | Soft shadow exists canvas to the right. Heavy texture and contrast in product, low contrast bg. Non-flat bg. Natural bg. Small hole in segmask near product edge top left. Set with 17 and 18. |
| 17 | `17.jpg` | Same FG and BG as 16, 2 holes top-left and top-right. Heavy shadow exiting the canvas on the right.|
| 18 | `18.jpg` | same FG and BG as 16, the bag runs off the right and bottom edges. Monitor its stretch and crop behavior. |
| 19 | `19.jpg` | Proposed: model in a denim jacket on a flat light grey wall; head in shot, body cut off by the bottom edge. black-pants-with-little-detail vs flat-natural-bg-with-little-detail might trigger "edge exists the frame" if graze detection fails. Head-cutter should crop. |
| 20 | `20.jpg` | Heavy-texture dark-color FGC+BGC. Product well clear of the edges.|
| 21 | `21.jpg` | Tests EIX=0111. Heavy Texture, 2 holes in the bottom-center of the mask. |
| 22 | `22.jpg` | Shoes same tint color as BG. PC similar luminance to BGC.Face cut by the top edge, white print close to the BG. |
| 23 | `23.jpg` | Red T-shirt, minimal distance between product and top edge to refine edge/graze/intersect detection. flat white bg. |
| 24 | `24.jpg` | Heavy texture product. Natural BGC. Subject has many holes and fine lines. Basic mask refinement risks losing the detail. |
| 25 | `25.jpg` | Set of (25-27) Full-length head-to-toe model in a print jumpsuit, white wall and herringbone wood floor; a framing the head-cutter would crop. |
| 26 | `26.jpg` | Same as 25, side pose, same wall and wood floor. Head-cutter should crop. |
| 27 | `27.jpg` | Same as 25, back view, same wall and wood floor. Head-cutter should crop. |
| 28 | `28.jpg` | Set of (28-30) : Model from behind in a black T-shirt and zebra-print skirt on an off-white wall. Cut by the bottom edge. Head-cutter should crop, unsure how today. |
| 29 | `29.jpg` | Set of (28-30) : Same as 28, face of model is showing. Head-cutter should crop, unsure how today. |
| 30 | `30.jpg` | Set of (28-30) : Same as 28, face of model is cut off. Head-cutter should not crop, unsure how today. |
| 31 | `31.jpg` | Black pedal bin, front view, on pure flat white. A plain easy case. Thin metal handle cannot be cut off. This is the thing to watch on  this image. Set with 32. |
| 32 | `32.jpg` | Tests an EIX=1100 case where the segmask might not pick up the top-right corner. |
| 33 | `33.jpg` | Low-contrast between PC and BGC. Touches top and bottom edges. Pink on near-white, hands in shot. Ie, contains a human but the human is not clearly visible. Might trip up the headcutter. |
| 34 | `34.jpg` | Set with 35: Non-reco Model in a blue jacket and black trousers on natural background. Face cut by the top edge, like the head-cutter's output. Set with 35. |
| 35 | `35.jpg` | Same outfit as 34, tight crop of the jacket and belt; touches the top and bottom edges. Angle at which the product escapes the image useful for checking graze-vs-intersecting |
| 36 | `36.jpg` | Finds a subject in a busy image and tests our content-aware BG stretching. Should find the old lady. Head-cutter should cut the head off of the old lady. |
| 37 | `37.jpg` | Colourful product on plain BG. Chrominance can trip up the segmask. |
| 38 | `38.jpg` | White product on a near-white BG where shadow causes product and BG to meld into each other. |
| 39 | `39.jpg` | Plain easy case. Dusty pink sweatshirt on light grey BG. |
| 40 | `40.jpg` | Navy croc wallet on pale non-flat natural BG. Segmask used to come out empty, now found (2026-11-20) Shadow leaves the frame. Watch for ugly bg stretching |
| 41 | `41.jpg` | textured scene with white bands; something unstretchable (the snowboard) sits on the rightmost non-white pixels, so it defaults to white bands |
| 42 | `42.jpg` | Product has many holes. Segmask should preserve the entire product. Easy flat BG. |
| 43 | `43.jpg` | Stress image for cropping. All edges blocked, segmask might not be reliable here. Currently handled well. |
| 44 | `44.jpg` | Detector stress image; contains 9 faces. The head-cutter must not modify it. |
| 45 | `45.jpg` | Frame case (hedge-trimmer blade, thin black frame all round): out of scope for the BG-fill work; stays EIX0000 |
| 46 | `46.jpg` | Textured scene with white bands; it could be stretched or inpainted, but that is too ambitious, so it defaults to white bands. |
| 47 | `47.jpg` | A tricky-to-detect product |
| 48 | `48.jpg` | Black shoes on flat natural BG. Stretching the BG caused glitches. Black-band bug: expect no band of any colour, 100% filled |
| 49 | `49.jpg` | White shoes on flat natural BG. Stretching the BG caused glitches. black-band bug: same checks as 48 |
| 50 | `50.jpg` | Super-heavy textured BG, product close to the canvas edge |
| 51 | `51.jpg` | Flat-looking but not flat BG with sharp lines (paper folds), product close to the canvas edge. Right above top of product susceptible to creating streaks. |
