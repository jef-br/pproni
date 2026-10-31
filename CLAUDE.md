# CLAUDE.md
Project notes for any Claude session working in this repo.
**Read `docs/work-state/TOC.md` first:** what is decided, rejected and still open.
All docs start at `docs/TOC.md` (an Obsidian vault; every folder has its own table of contents).

## Layout
One crate, one `src/` tree.
* `src/app/` — host shell: folder flow, batch, i18n.
* `src/core/` — the pipeline: `load → preprocess → classify → dispatch → export`,
  one module per stage. The shot classifier lives at `src/core/shot_classifier/`. `core::framing`
  (bars and frames) runs between classify and dispatch.
* `examples/`, `tools/` — the BiRefNet dev harness and the ONNX export script.

## Ships as one hardened exe, no install
A hard requirement, met — not optional, and never solved by putting a file beside the exe.
`cargo build --release` produces the single main exe and nothing else: the `.onnx` models (in
`models/`, embedded encrypted) and `onnxruntime.dll` (unpacked at startup to a per-user `pproni-*`
cache file) are compiled in, and a missing one breaks the *build*, not the run. How, why, and the
clean finish: `docs/architecture/05-single-exe.md`. The startup abort (`core::preflight`) instead of
an all-R3 batch: `docs/architecture/04-known-gaps.md`.

## Diagrams
`docs/diagrams/JBA2B.drawio.svg` is the one remaining diagram — an end-to-end App/Core flowchart.
Reach it through the diagram-sync skill: you read the embedded mxGraph XML, not the picture.
Editing needs something that rewrites both layers together — the VS Code **Draw.io Integration**
extension, or a headless render through the drawio webapp that extension bundles. Hand-editing the
compressed blob desyncs source from image.

## Maintenance rule — live again
One node per module, one per `struct`/`enum`; add a module or public function → patch the node in
the same commit. The rule, its history and what each container covers:
`docs/architecture/06-diagram.md`.

## Docs
* `docs/architecture/` — code documentation: module map, pipeline stages, routing, known gaps.
* `docs/specs/` — the design spec, one file per §. Its §5 table is six *behaviours*, not six
  routes: there are three routes, and §5's rows live inside R1 and R2.
* `docs/work-state/`, `docs/diagrams/`, `docs/analysis/`, `docs/agent-knowledge/`, `docs/notes/`.
* Code comments point to these docs instead of repeating them: `docs/agent-knowledge/code-comments.md`.

## Commit attribution
Never add `Co-Authored-By:` trailers or "Generated with [Claude Code]" lines to commits or pull
requests. Commits carry the repo owner as the sole author — no assistant attribution, ever.
