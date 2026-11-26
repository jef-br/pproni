# 6. The diagram

[Up: table of contents](TOC-architecture.md)

**Maintenance rule:** one node per module, one node per `struct`/`enum`. Add a module or a public
function → patch the diagram node in the same commit. A node with no matching file means the
diagram drifted.

> **Diagram status.** The four diagrams the architecture doc used to embed (`class-map`, `pipeline-flow`,
> `detection-internals`, `geometry-routing`) were deleted when the classical detector was retired.
> `JBA2B.drawio.svg` survives and was rebuilt against the current code — see
> below. The maintenance rule above applies again: every stage has a node to
> patch.


**[JBA2B.drawio.svg](../diagrams/JBA2B.drawio.svg)** is an end-to-end flowchart across seven containers: **App**, **Core** (holding Preprocessor → Shot Classifier → Processor → Exporter) and a **dev harness** lane. It was rebuilt
against the current code, so the maintenance rule at the top of this page applies again — add a
module or a public function, patch its node in the same commit.

Every `struct` and `enum` in `src/` and `examples/` outside `#[cfg(test)]`, public or private, has its
own node, inside the container of the part of Pproni that defines it and labelled with that module's
path (e.g. `shot_classifier::route` / `enum Route`). Helper types are compact nodes, but each one is
its own node.

**Every type node is joined to the node of the `.rs` file that uses it** by a dotted arrow (a solid
flow arrow also counts, as for `Prepared`). If the file has no node of its own yet, it gets one
(`progress.rs`, `i18n.rs`, `options.rs`, `trace.rs`, `gpu_adapters.rs`, `geometry.rs`,
`sampling.rs`, `colors.rs`, `shot_classifier (mod.rs)`, `exporter::export`, `preprocessor::prepare`).
A type used by several files links to its defining file and, when the user sits in the same
container, to the main consumer. Types are lined up beside the node they belong to, so the dotted
lines stay short and run in the gap between the two columns; arrows are kept off text.

What each container now says:

- **App** — launch, `core::preflight` (unpack the embedded runtime, build the BiRefNet and headcut
  sessions, abort on failure), the desktop folder bootstrap, the headcut question (only when
  `PERMIT_HEADCUTS` is on), the `progress` and `i18n` files and `batch::run`'s rayon fan-out with its one level of subfolder
  recursion mirrored into `PPRONI-OUTPUT` and `PPRONI-BACKUP`.
- **Core** — `core::process_file`'s order (prepare, classify, headcut, framing, colours, route,
  export), the headcut step, `core::framing`, `core::options` and `core::trace`.
- **Preprocessor** — decode + EXIF, the unconditional alpha flatten onto white, the sRGB branch
  (an already-sRGB profile is skipped), the optimiser (colour balance, then the trim crop), the
  letterbox/pillarbox-bar detector, `maria_antonieta` (the headcut face detector) and
  `Prepared { original, working, model_input, crop, trims, framing, letterbox }`.
- **Shot Classifier** — segmentation (graphics card or CPU), pass-1 `gate`, pass-2 `refine_edge`, the
  graze test, the `ShotClassification` + `Mask`, and then `route::decide` with `plan_square`: the
  routing decisions live here, not in the routes. The model-error `None` (hidden today, #31) is a node of
  its own, because it is not the same thing as an empty edge list.
- **Processor** — the three routes, `route::send` having picked one. R1 is a chain of nodes, one
  per module of `center_and_stretch`: `flatness`, `grazes`, the solid test, `compose` (with
  `inpaint` and `Silhouette` beside it), `gradients`, `patch`, `solid`, and `extend` for the
  textured-scene reframe.
- **Exporter** — `exporter::export`, `colors` (the one BGC/FGC sample), `resize::to_envelope`, the `PPRONI_DEBUG_TAGS` branch, the JPEG save, and the move of
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
