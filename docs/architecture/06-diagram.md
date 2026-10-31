# 6. The diagram

[Up: table of contents](TOC.md)

**Maintenance rule:** one node per module, one node per `struct`/`enum`. Add a module or a public
function → patch the diagram node in the same commit. A node with no matching file means the
diagram drifted.

> **Diagram status.** The four diagrams the architecture doc used to embed (`class-map`, `pipeline-flow`,
> `detection-internals`, `geometry-routing`) were deleted when the classical detector was retired.
> `JBA2B.drawio.svg` survives and was rebuilt against the current code — see
> below. The maintenance rule above applies again: every stage has a node to
> patch.


[JBA2B.drawio.svg](../diagrams/JBA2B.drawio.svg) is an end-to-end flowchart across six containers: **App**, **Core**
(Preprocessor → Shot Classifier → Processor → Exporter) and a dev-harness lane. It was rebuilt
against the current code, so the maintenance rule at the top of this page applies again — add a
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
`<stem>___3-class_20_REFINE.png` (naming rules in [run_dir tag protocol](../diagrams/run-dir-tag-protocol.md)). The older invented
tags went with the detector.

Edits must be made through the draw.io editor (the VS Code **Draw.io Integration** extension), or
through an equivalent render that regenerates both layers — the `content` attribute and the visible
SVG have to be rewritten together, or the picture and its source silently disagree. See
`.claude/skills/diagram-sync/SKILL.md`.
