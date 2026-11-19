# 2. Modules: `core`, the imaging pipeline

[Up: table of contents](TOC.md)

| Module | Surface |
|---|---|
| `core` | `process_file(input, output) -> Result<(), String>` |
| `core::config` | `JPEG_QUALITY`, `WORKING_SIZE`, `GATE_MARGIN_PX`, `REFINE_BAND_PX`, `REFINE_SAFETY_PX`, `REFINE_CONTEXT_PX` |
| `core::preprocessor` | `Decoded`, `Prepared`, `Trims`, `Framing`, `load`, `preprocess`, `prepare`; `optimizer::optimize`, `optimize_both`, `detect_letterbox` |
| `core::preprocessor::maria_antonieta` | `cut_above_philtrum(section) -> Option<RgbImage>`, `preflight()` — headcut on a face-containing section via an embedded YuNet (OpenCV Zoo, BSD-3-Clause) ONNX; cut above the philtrum, else `None`. Called from `process_file` post-classify when `Options::enable_headcuts` is on and EIX leaves Left or Right free |
| `core::framing` | `decide`, `Decision` (`Keep`, `Centre`, `Peel`), `border_colour`, `deepen_peel` |
| `core::processor::routes` | `Route`, `Route::select`, `dispatch` |
| `core::processor::routes::{center_and_stretch, crop_square, fallback}` | `apply` |
| `core::exporter` | `export`, `ShotInputs`, `shot_tags`; `colors::sample` (BGC/FGC); `save::save_jpeg_srgb`, `icc::srgb_profile` |

`core::framing` runs once after the model, on trim 1's box, trim 2's box (when it moved), the number of
edges the subject touches and the mask. It tells padding and frames from real backdrop:

* **Bars** (trim 1 cut both sides of one axis, the other axis loses at most 2% in total, the subject
  touches an edge, and the two cut sides are of similar width: the thinner is at least
  `FRAMING_BARS_SYMMETRY` = 0.3 of the thicker). Unequal cuts are backdrop round an off-centre subject
  (image 19) and stay `Keep`. Every other bars case is `Centre` (images 3, 33, 35, 41, 46): R1 cuts
  the square from the content box with equal padding either side. The scene behind such bars is
  usually a lifestyle photo, and stretching it was tried on 41 and did not look convincing.
* **Frame** (EIX `0000`, trim 1 cut all four sides, trim 2 did not move, trim 1's box is square within
  `FRAMING_SQUARE_TOLERANCE` (2%), is more than `TRIM_SEGMASK_TOLERANCE_PX` (8) from the mask box and
  holds the mask): `Peel`. `deepen_peel` goes on inward while a line is at least 99% frame colour outside
  the mask (plus one line of erosion); then `original`, `working` and the mask are cropped and only the
  edge check (`judge`) runs again.

R1's margin: [spec §5.1](../specs/05-routing.md).

The refine params live in `config` next to `WORKING_SIZE` because they are expressed in
working-resolution pixels — change one and the others shift meaning.
