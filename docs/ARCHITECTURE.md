# Pproni architecture

Map of every module, its public surface, and how data flows through the pipeline.

**Maintenance rule:** one node per module, one node per `struct`/`enum`. Add a module or a public
function → patch the diagram node in the same commit. A node with no matching file means the
diagram drifted.

> **Diagram status.** The four diagrams this document used to embed (`class-map`, `pipeline-flow`,
> `detection-internals`, `geometry-routing`) were deleted when the classical detector was retired.
> `JBA2B.drawio.svg` survives and was rebuilt against the current code — see
> [The diagram](#the-diagram). The maintenance rule above applies again: every stage has a node to
> patch.

---

## 1. Pipeline

`core::process_file` runs five stages in order, with a framing step (`core::framing`) between classify and dispatch. `app::batch::run` fans this over every image in the
input folder with `rayon`; on success the source file is moved to the backup folder (`PPRONI-BACKUP`),
on failure it is left in place.

```
load ──▶ preprocess ──▶ classify ──▶ dispatch ──▶ export
```

| Stage | Module | In → Out |
|---|---|---|
| **load** | `core::preprocessor::load` | path → `Decoded` (image + ICC profile, EXIF applied) |
| **preprocess** | `core::preprocessor::preprocess` | `Decoded` → `Prepared` (sRGB, alpha flattened, + true-colour working copy + the optimiser's crop); `Err` for an image with nothing but backdrop |
| **classify** | `core::classify` (private) | `Prepared` → `Option<(ShotClassification, Mask)>` |
| *framing* | `core::framing::decide` (in `process_file`) | trims + mask → `Keep`, `Centre`, `Crop` or `Peel`; `Crop` and `Peel` crop the picture, `Centre` sets `Prepared::framing` |
| **dispatch** | `core::processor::routes::dispatch` | `Prepared` + verdict → `RgbImage` |
| **export** | `core::exporter::export` | `RgbImage` (+ optional `ShotInputs`) → size envelope → JPEG on disk, source moved to `PPRONI-BACKUP` |

`classify` carries the segmentation `Mask` out with the verdict; the exporter needs it for the
debug tags below.

### Why two resolutions

`Prepared` carries the image three times: `original` at full resolution, `working` reduced to
`config::WORKING_SIZE` on its longest side (both true colour), and `model_input`, the image BiRefNet
sees: `working` after the optimiser. `Prepared::crop` says where `model_input` was cut from `working`.

The optimiser is `preprocessor::optimizer::optimize_both` (`optimize` wraps it and keeps only the model input and the crop). It does two things in order, then a control:

1. **Colour balance.** A Levels move worked out once per image: per channel it finds `lo` (the highest
   value with at most 0.68% of the pixels at or below it) and `hi` (the 99.32 percentile), shares the
   lowest `lo` and the highest `hi` across the three channels, derives a gamma
   `1 + lo/(255-lo) - (255-hi)/hi` kept between 0.01 and 9.99, and applies one 256-entry table to
   every channel. The one rounding happens when the table is built.
2. **Trim**, on the balanced image. The 16 border pixels (4 per edge, at 0/25/50/75% of the edge,
   going round clockwise) are first filtered for outliers: a sample is dropped when its L1 distance to
   the per-channel median exceeds `TRIM_OUTLIER_FLOOR + TRIM_OUTLIER_K·MAD`, with the `TRIM_MIN_KEEP`
   closest always kept. The kept samples give the backdrop colour range: per channel, the lowest and
   highest sample. A pixel inside the range on all three channels is backdrop. Gaps are closed with a 2x2 window
   (`TRIM_CLOSE_K`): a pixel only counts as product if it sits in a 2x2 block of non-backdrop pixels.
   The tightest box around the product is squared to its largest side and grown on every side by the
   larger of 10 px and 10% of that side (`TRIM_MARGIN_MIN_PX`, `TRIM_MARGIN_SHARE`). The balanced image
   is cropped to that square, centred on the product and slid to stay inside the frame (the margin
   shrinks if the frame is too narrow; if the product is longer than the frame's short side no square
   fits, the margin drops to 10 px and the crop is cut at the frame). No product left: `Err`, the image is not processed and
   stays in the input folder.
3. **Trim 2, the control** (`recheck_box`): the same trim run again on trim 1's box. It counts as moved when
   a side differs by more than `TRIM_RECHECK_TOLERANCE_PX` (2). The model never sees it; `core::framing`
   reads both boxes against the mask.

`core::classify` shows the model only that crop, wrapped in `shot_classifier::CroppedModel`, which
pastes the returned mask into a blank working-size canvas at the crop position (full-frame box). The
gate, refinement and routes therefore see a full-frame mask, as if the model had been shown the whole
frame.

The segmentation model runs on the optimiser's crop of `working`. Its pass-2 refinement then re-decides the subject boundary
against full-resolution pixels in `original`, because the mask it produced is upsampled and not
pixel-accurate at the edge. Hand the classifier the same image twice and the scale between them
collapses to 1.0, the refinement ring shrinks to its floor, and pass 2 stops doing anything.

### Why classification is optional

`classify` returns `Option`. `None` means *no verdict* — the model is unavailable, or inference
failed on this image. That is deliberately distinct from `Some` with an empty `touches_edges`, which
means *we looked, and the subject touches no edge*.

They route differently, so a model failure degrades one image to a safe framing instead of being
silently processed as a clean free-standing shot.

---

## 2. Modules

### `app` — the host shell

| Module | Surface | Notes |
|---|---|---|
| `app::batch` | `run(input, output, backup, footer, lang) -> Summary`, `Summary` | rayon fan-out, one level of subfolder recursion; drives `progress` |
| `app::progress` | `Progress`, `new(total, footer, lang)`, `tick` | in-place unicode bar, always 40 cells with the `NN%` drawn over them; throttled ~15 Hz via non-blocking `try_lock`; self-disables when stdout is not a TTY |
| `app::shell` | `run()` | Windows double-click flow: desktop `PPRONI-INPUT` → `PPRONI-OUTPUT`, originals → `PPRONI-BACKUP`. Draws the three screens |
| `app::header` | `print()` | Windows-only; hand-tuned banner (block logo + name + tagline), same on all three screens |
| `app::theme` | `WIDTH`, the palette, `rule`, `callout`, `slug` | 24-bit ANSI matching the design hex; owns the frame every screen is drawn in |
| `app::console` | `init`, `clear`, `ui_language`, `folder_url`, `pause` | Windows-only; ANSI, OSC-8 links, UI language |
| `app::desktop` | `desktop_dir()` | Windows-only |
| `app::i18n` | `Lang`, `drop_prompt`, `close_line`, `interrupt_hint`, `backup_label`, `output_label`, `remaining`, `finished` | EN/ES/FR/NL/IT/DE |

`console`, `desktop`, `header` and `shell` are `#[cfg(windows)]`; `progress` and `theme` are cross-platform. On other hosts only the two-argument CLI form
(`pproni <input_dir> <output_dir>`) is available, and `i18n` reads as dead code because nothing calls
it — that is expected, not drift.

### `core` — the imaging pipeline

| Module | Surface |
|---|---|
| `core` | `process_file(input, output) -> Result<(), String>` |
| `core::config` | `JPEG_QUALITY`, `WORKING_SIZE`, `GATE_MARGIN_PX`, `REFINE_BAND_PX`, `REFINE_SAFETY_PX`, `REFINE_CONTEXT_PX` |
| `core::preprocessor` | `Decoded`, `Prepared`, `Trims`, `Framing`, `load`, `preprocess`, `prepare`; `optimizer::optimize`, `optimize_both`, `detect_letterbox` |
| `core::framing` | `decide`, `Decision` (`Keep`, `Centre`, `Crop`, `Peel`), `border_colour`, `deepen_peel` |
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
* **Frame**: `Peel`, as described in `docs/WORK_STATE.md`.

R1's margin: each free side of an axis takes 4.2% of the subject's extent, so an axis with two free
sides takes 8.4% and one with a single free side 4.2%. An axis pinned on both sides takes none.

The refine params live in `config` next to `WORKING_SIZE` because they are expressed in
working-resolution pixels — change one and the others shift meaning.

#### Debug tags (`PPRONI_DEBUG_TAGS` marker file)

Off by default — the shipped exe writes a clean `<stem>.jpg`. Drop a file named `PPRONI_DEBUG_TAGS`
next to the executable (content ignored, only presence matters — same sidecar convention
`core::sidecar` uses for `onnxruntime.dll`/the `.onnx` model) and the exporter instead names each
output from the shot classifier's verdict:

```
<stem>--EIX=<trbl>[--BGC=<hex>][--FGC=<hex>].jpg   the export, renamed
<stem>_segmask.jpg                                the segmentation mask, sRGB JPEG at output size (nearest)
```

`EIX` is the 4-bit top-right-bottom-left edge string — always the classifier's real verdict, the
same one `Route::select` routed on; `BGC`/`FGC` are the dominant background / weighted-median
foreground colours, each emitted only when sampleable. `=` stands in for the spec's `:` (illegal in
Windows filenames). `EIX` comes from `shot_classifier::shotcode::eix_bits`; `BGC`/`FGC` are sampled
once, by `exporter::colors`, from the true-colour `working` copy and the mask — never from
`model_input`. The `run_dir` dev harness uses the same two. With no verdict (a failed inference, or the sidecar files not found) there is no mask:
the name carries `EIX=____` and no segmask is written.

`ShotClassification::mask_too_small` flags a mask that covers too little of the frame to be a
subject. It is informational for the tag and never overrides `eix` — a tag must never claim an edge
intersection the pipeline didn't actually route on. It doesn't appear in the filename today;
`run_dir`'s console output and `zzz_results.json` are where it's currently visible.

The same marker also drives `core::trace`: when it is present, every image writes a per-stage
trace (`<stem>  -  NN.jpg`, `<stem>  -  log.txt`) into the output image's folder, next to its
`<stem>_segmask.jpg`. One switch for all debug output, debug and release alike.

### `core::shot_classifier` — the classifier

One module per job. `SegmentationModel` keeps the gate/refine logic independent of the ONNX
binding, so it can also be exercised against a stand-in model in tests — real BiRefNet inference is
always built into the exe (no feature flag).

| Module | Surface | Job |
|---|---|---|
| `shot_classifier` | `ShotClassification`, `classify_instance` | runs both passes for one instance |
| `gate` | `EdgeProximity`, `gate` | pass 1: cheap border-band scan, per edge |
| `refine` | `RefinementInput`, `RefineParams`, `EdgeRefinement`, `refine_edge` | pass 2: trimap + colour matting at full resolution |
| `geometry` | `Edge`, `Grid`, `EdgeView`, `Rect` | edge-relative coordinates |
| `segmentation` | `Mask`, `Instance`, `SegmentationModel` | model-agnostic types |
| `sampling` | `ColorStats`, `sample_background_color` | per-edge background colour |
| `shotcode` | `eix_bits`, `mask_too_small`, `NO_VERDICT_EIX` | the `EIX` debug tag bits |
| `birefnet` | `BiRefNetConfig`, `BiRefNetModel` | ONNX inference, always built in |
| `cipher` | ChaCha20 keystream | shared by `build.rs` (encrypt) and `birefnet::load` (decrypt) for the embedded model |

`EdgeView` is the load-bearing idea: every per-edge algorithm is written once in "top edge, depth
increasing downward" terms against the `Grid` trait, and the other three edges are presented by
coordinate transform. Nothing edge-specific lives outside `geometry`.

---

## 3. Routing

Three routes, keyed on the shot classifier's verdict:

| Route | Verdict | Strategy |
|---|---|---|
| **R1** `CenterAndStretch` | a mask that describes a subject, with room to grow | square around the subject; bled edges pinned, background stretched to fill |
| **R2** `CropSquare` | `mask_too_small`, or left+right bled and nothing else, or all four bled | the largest square already in the frame, centred on the frame |
| **R3** `Fallback` | no verdict at all | fit the whole image into a square canvas, centred, white fill |

**Three routes, six behaviours.** `docs/pproni-spec.md` §5's table is not a competing route list — it is
the *inside* of these routes. Four of its bleed rows are one rule seen from four angles, and
`center_and_stretch` applies that rule: a bled edge is pinned, an axis with a free side takes the
margin, and the slack goes to the free sides. Choosing among them is a behaviour, not a routing
decision, so it never reaches `Route::select`. Only the full bleed leaves R1, because with all four
edges blocked there is no background anywhere to stretch from.

R3 has no spec row. It is the answer to "we don't know": nothing cropped, nothing scaled, so a wrong
guess costs framing rather than pixels.

**Status:** all three routes are implemented. Spec behaviours are carried in their own module docstring.

---

## 4. Known gaps

- BiRefNet inference is always built in; `core::preflight` aborts at startup, before any file is
  touched, if the ONNX session can't be built (e.g. the embedded runtime can't be unpacked to any
  writable directory) — a batch never silently falls back to R3 for every image.

---

## 5. Single hardened exe, no install

**Pproni ships as one hardened executable with no installation step. This is a hard requirement.**

Hardening is already in place in `Cargo.toml`'s release profile — fat LTO, one codegen unit,
symbols and debuginfo stripped, no PDB, abort on panic — and is anti-RE as much as size. The
embedded model is also **encrypted at rest** (see below), so it no longer carves out of the exe
with `binwalk`/`strings`. This is obfuscation, not secrecy: the key ships in the binary, so it
raises the effort bar without pretending a determined analyst can't recover the weights.

**Met.** `cargo build --release` produces one exe file (~195MB) and nothing else. Two things are
compiled in:

- **The model.** `shot_classifier::birefnet` embeds one BiRefNet export and loads it through
  `Session::commit_from_memory`. It is stored **encrypted**: `build.rs` reads the plaintext out of
  `models/` — `MODEL_DIR` plus `MODEL_FILE` there are the only things that pick which export ships —
  encrypts it under a per-build key (a ChaCha20 keystream in `shot_classifier::cipher`,
  shared with the build script by path), and `include_bytes!`'s the encrypted blob from `OUT_DIR`;
  `birefnet::load` decrypts into a heap buffer only for as long as ORT needs to copy the graph in.
  The plaintext model is never a static string in the image and never touches disk.
- **The ONNX Runtime.** `core::runtime` embeds `onnxruntime.dll` the same way. `ort`'s
  `load-dynamic` needs a real path to `LoadLibrary`, so at first use the bytes are written once to
  `<%LOCALAPPDATA%|temp>/pproni-<tag>/pproni.dll` and that path is handed to `ort::init_from`.
  The `<tag>` is the library's size plus a content hash: a rebuilt runtime lands in a fresh
  directory, repeat runs reuse the file already there, and the write is atomic (temp name then
  rename) so concurrent Pproni processes don't corrupt it.

Both source files are gitignored — the model in `models/`, `onnxruntime.dll` at the repo root — and
a missing one is a *build* failure naming the file, not a runtime surprise. `models/` is the single
place any `.onnx` lives; it holds every variant being tried, and only `MODEL_FILE` decides which one
is baked in. The only files Pproni still touches beside the exe are
optional markers (`PPRONI_DEBUG_TAGS`) and the `ORT_DYLIB_PATH` override the `examples/` dev harness
uses to point at a hand-placed runtime.

### Why the runtime is unpacked rather than linked

Static linking was the first choice and it doesn't work here. pyke publishes a prebuilt *static*
ONNX Runtime for `x86_64-pc-windows-msvc` (`ortrs_static`, listed in `ort-sys`' `dist.txt`), but it
is **ORT 1.20.0**, and BiRefNet's decoder needs the native `DeformConv` op that only arrives in ORT
1.22:

```
Could not find an implementation for DeformConv(19) node with name 'node_deform_conv2d'
```

The session builds and then fails at load, on every image. Newer `ort` does not rescue it: from
`ort-sys` rc.13 the Windows distribution is Microsoft's own build (`ms@1.28.0`), a DLL — pyke
stopped shipping static Windows libraries. Re-exporting the model to avoid `DeformConv` is not the
way out either: without the native op it decomposes to `GatherND`, which allocates enormous
intermediates (see the `birefnet` module docs). So the runtime stays a DLL — it is just carried
*inside* the exe and unpacked on demand rather than shipped next to it.

### Linking it in later, if a static ORT >= 1.22 appears

Build ONNX Runtime >= 1.22 from source with CMake as a static library, point `ort-sys` at it with
`ORT_LIB_LOCATION`, and swap `load-dynamic` for the default linking. `ort` rc.9 requests API
version 20 and the ORT C API is forward-compatible, so a 1.22+ library serves it. `birefnet`'s
`init_runtime` and the whole of `core::runtime` then delete — nothing else in the codebase knows
the runtime is dynamic. This would drop the runtime unpack (a one-time ~14MB write on first run)
and shrink the exe slightly, but it is no longer on the critical path for the single-file
requirement.

### Note: the runtime DLL still needs the VC++ runtime

`onnxruntime.dll` imports `msvcp140.dll` / `vcruntime140*.dll`. Those are present on any
up-to-date Windows 11 and were already a requirement of the old sidecar layout, so this is not a
regression — but a truly bare target still needs the Visual C++ Redistributable.

---

## 6. The diagram

`docs/diagrams/JBA2B.drawio.svg` is an end-to-end flowchart across six containers: **App**, **Core**
(Preprocessor → Shot Classifier → Processor → Exporter) and a dev-harness lane. It was rebuilt
against the current code, so the maintenance rule at the top of this document applies again — add a
module or a public function, patch its node in the same commit.

What each container now says:

- **App** — launch, `core::preflight` (unpack the embedded runtime, build the session, abort on
  failure), the desktop folder bootstrap, and `batch::run`'s rayon fan-out with its one level of
  subfolder recursion mirrored into `PPRONI-OUTPUT` and `PPRONI-BACKUP`.
- **Preprocessor** — decode + EXIF, the unconditional alpha flatten onto white, the sRGB branch,
  the optimiser (colour balance, then the trim crop), the letterbox/pillarbox-bar detector, and
  `Prepared { original, working, model_input, crop, letterbox }`.
- **Shot Classifier** — segmentation, pass-1 `gate`, pass-2 `refine_edge`, the graze test, and the
  `ShotClassification` + `Mask` that leaves for the Processor. The no-verdict `None` is a node of
  its own, because it is not the same thing as an empty edge list.
- **Processor** — `Route::select`'s two questions and the three routes behind them.
- **Exporter** — `resize::to_envelope`, the `PPRONI_DEBUG_TAGS` branch, the JPEG save, and the move of
  the original into `PPRONI-BACKUP` on success.
- **Dev harness** — `examples/run_dir.rs`, replacing the proposed `Pproni-Analyzer` container whose
  target (`src/bin/pproni_analyzer.rs`) was deleted with the classical detector.

Two green bold tags survive from the debug-image protocol, `SEGMASK` and `REFINE`, and `run_dir`
follows that protocol for the two snapshots it writes: `<stem>___3-class_10_SEGMASK.png` and
`<stem>___3-class_20_REFINE.png` (naming rules in `docs/diagrams/README.md`). The older invented
tags went with the detector.

Edits must be made through the draw.io editor (the VS Code **Draw.io Integration** extension), or
through an equivalent render that regenerates both layers — the `content` attribute and the visible
SVG have to be rewritten together, or the picture and its source silently disagree. See
`.claude/skills/diagram-sync/SKILL.md`.
