# 2. Modules: `core`, the imaging pipeline

[Up: table of contents](TOC-architecture.md)

| Module | Surface |
|---|---|
| `core` | `process_file(input, output, backup, options) -> Result<(), String>`, `preflight()`, `backend()` |
| `core::config` | `JPEG_QUALITY`, `PERMIT_HEADCUTS`, `WORKING_SIZE`, `GATE_MARGIN_PX`, `REFINE_NARROW_BAND_PX`, `REFINE_SAFETY_PX`, `REFINE_CONTEXT_PX`, the model paths (`birefnet_model_path!`, `yunet_model_path!`); every other tuning value: [core-config](modules/core-config.md) |
| `core::options` | `Options` (`enable_headcuts`), built once per run |
| `core::runtime` | `onnxruntime_dylib()`: unpacks the embedded ONNX Runtime |
| `core::trace` | `begin`, `image`, `note`, `active`, `debug_enabled`: the debug trace, on only with the `PPRONI_DEBUG_TAGS` marker |
| `core::preprocessor` | `Decoded`, `Prepared`, `Trims`, `Framing`, `load`, `preprocess`, `prepare`; `optimizer::optimize`, `optimize_both`, `detect_letterbox` |
| `core::preprocessor::maria_antonieta` | `cut_above_philtrum(section) -> Option<RgbImage>`, `preflight()` — headcut on a face-containing section via an embedded YuNet (OpenCV Zoo, BSD-3-Clause) ONNX; cut above the philtrum, else `None`. Called from `process_file` post-classify when `Options::enable_headcuts` is on and EIX leaves Left or Right free |
| `core::framing` | `decide`, `Decision` (`Keep`, `Centre`, `Peel`), `border_colour`, `deepen_peel` |
| `core::processor::routes` | the three route modules below; the decision itself is `core::shot_classifier::route` (`Route`, `R1Job`, `Shot`, `Decision`, `decide`, `send`, `plan_square`) |
| `core::processor::routes::{center_and_stretch, crop_square, fallback}` | `apply` |
| `core::exporter` | `export`, `ShotInputs`, `shot_tags`; `colors::sample` (BGC/FGC); `save::save_jpeg_srgb`, `icc::srgb_profile` |

`core::framing` runs once after the model, on trim 1's box, trim 2's box (when it moved), the number of
edges the subject touches and the mask. It tells padding and frames from real backdrop:

* **Bars** (trim 1 cut both sides of one axis, the other axis loses at most `FRAMING_OTHER_AXIS_SLACK` in total, the
  subject touches an edge, and the two cut sides are of similar width: the thinner is at least
  `FRAMING_BARS_SYMMETRY` of the thicker). Unequal cuts are backdrop round an off-centre subject
  (image 19) and stay `Keep`. Every other bars case is `Centre` (images 3, 33, 35, 41, 46): R1 cuts
  the square from the content box with equal padding either side. The scene behind such bars is
  usually a lifestyle photo, and stretching it was tried on 41 and did not look convincing.
* **Frame** (EIX `0000`, trim 1 cut all four sides, trim 2 did not move, trim 1's box is square within
  `FRAMING_SQUARE_TOLERANCE`, differs from the mask box by more than `TRIM_SEGMASK_TOLERANCE_PX` on
  at least one side, and holds the mask with the same slack): `Peel`. `deepen_peel` goes on inward
  while a line is at least 99% frame colour outside the mask (plus one line of erosion); then
  `original`, `working` and the mask are cropped and only the edge check (`judge`) runs again.
* Anything else: `Keep`.

```
░ = cut off by trim 1     ▓ = the subject's mask     a side counts as cut when it lost > 2 px
                                                     (TRIM_RECHECK_TOLERANCE_PX)

┌───┬──────────────┬───┐   BARS → Centre   (left/right shown; top/bottom works the same)
│░░░│              │░░░│   • left and right both cut
│░░░│     ▓▓▓▓     │░░░│   • top + bottom together lose ≤ 2% of the height  (FRAMING_OTHER_AXIS_SLACK)
│░░░│    ▓▓▓▓▓▓    │░░░│   • the subject touches an edge (here: the bottom)
│░░░│    ▓▓▓▓▓▓    │░░░│   • thinner bar ≥ 0.3 × thicker bar                (FRAMING_BARS_SYMMETRY)
└───┴────▓▓▓▓▓▓────┴───┘   → R1 cuts the square from the content box, equal padding either side

┌─┬────────────┬───────┐   UNEQUAL CUTS → Keep
│░│            │░░░░░░░│   • as BARS, but thinner bar < 0.3 × thicker bar
│░│    ▓▓▓▓    │░░░░░░░│   → the cut is real backdrop round an off-centre subject (image 19)
│░│   ▓▓▓▓▓▓   │░░░░░░░│
└─┴───▓▓▓▓▓▓───┴───────┘

┌──────────────────────┐   FRAME → Peel
│░░░░░░░░░░░░░░░░░░░░░░│   • EIX 0000: the subject touches no edge
│░░┌────────────────┐░░│   • all four sides cut
│░░│ trim 1's box   │░░│   • trim 2, run on trim 1's box, moved no side by > 2 px
│░░│     ┌────┐     │░░│   • trim 1's box is square within 2%               (FRAMING_SQUARE_TOLERANCE)
│░░│     │▓▓▓▓│     │░░│   • the mask box is inside trim 1's box (8 px slack) and more than
│░░│     │▓▓▓▓│     │░░│     8 px away from it on at least one side         (TRIM_SEGMASK_TOLERANCE_PX)
│░░│     └────┘     │░░│   → crop to trim 1's box, go on inward while a line is ≥ 99% frame
│░░└────────────────┘░░│     colour, then judge the edges again
│░░░░░░░░░░░░░░░░░░░░░░│
└──────────────────────┘
```

R1's margin: [spec §5.1](../specs/05-routing.md).

The refine params live in `config` next to `WORKING_SIZE` because they are expressed in
working-resolution pixels — change one and the others shift meaning.
