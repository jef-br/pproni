# CLAUDE.md
Project notes for any Claude session working in this repo.
**Read `docs/WORK_STATE.md` first:** what is decided, rejected and still open.

## Layout
One crate, one `src/` tree.
* `src/app/` — host shell: folder flow, batch, i18n.
* `src/core/` — the pipeline: `load → preprocess → classify → dispatch → export`,
  one module per stage. The shot classifier lives at `src/core/shot_classifier/`. `core::framing`
  (bars and frames) runs between classify and dispatch.
* `examples/`, `tools/` — the BiRefNet dev harness and the ONNX export script.

## Ships as one hardened exe, no install
A hard requirement, met — not optional, and never solved by putting a file beside the exe.
`cargo build --release` produces the single main exe and nothing else. The `.onnx` model
(`shot_classifier::birefnet`, in `models/` — the one place any `.onnx` lives; `build.rs`'s
`MODEL_FILE` picks which variant ships) and `onnxruntime.dll` (`core::runtime`, at the repo root)
are gitignored and compiled into the binary — a missing one breaks the *build*, not the run. The
model is embedded **encrypted**: `build.rs` encrypts it under a per-build key (shared cipher in
`shot_classifier::cipher`) and `birefnet::load` decrypts it in memory before `commit_from_memory`,
so the weights don't carve out of the exe with `binwalk`/`strings`. Obfuscation, not secrecy — the
key ships in the binary. The runtime is unpacked to a per-user cache file under a `pproni-*`
name (not `onnxruntime.dll`). At startup the runtime is unpacked to a per-user cache file (`ort`'s loader needs a real file
to `LoadLibrary`) and the classifier session is built; if that fails the exe aborts before touching
any file (`core::preflight`) rather than silently taking the R3 fallback for the whole batch.
Real inference is always built in — no feature flag. `load-dynamic` stays only because no static
ONNX Runtime >= 1.22 exists for this target (pyke's is 1.20, short of the `DeformConv` op BiRefNet
needs). `docs/ARCHITECTURE.md` §5 has the detail and the clean finish.

## Diagrams
`docs/diagrams/JBA2B.drawio.svg` is the one remaining diagram — an end-to-end App/Core flowchart.
Reach it through the diagram-sync skill: you read the embedded mxGraph XML, not the picture.
Editing needs something that rewrites both layers together — the VS Code **Draw.io Integration**
extension, or a headless render through the drawio webapp that extension bundles. Hand-editing the
compressed blob desyncs source from image.

## Maintenance rule — live again
One node per module, one per `struct`/`enum`; add a module or public function → patch the node in
the same commit. It was suspended while `JBA2B.drawio.svg` still described the retired classical
detector; the diagram was rebuilt against the current code on 2026-09-09, so every stage has a node
to patch again. `docs/ARCHITECTURE.md` §6 says what each container covers.

## Docs
* `docs/ARCHITECTURE.md` — module map, pipeline stages, routing, known gaps.
* `docs/pproni-spec.md` — the design spec. Its §5 table is six *behaviours*, not six routes:
  there are three routes, and §5's rows live inside R1 and R2.

## Commit attribution
Never add `Co-Authored-By:` trailers or "Generated with [Claude Code]" lines to commits or pull
requests. Commits carry the repo owner as the sole author — no assistant attribution, ever.
