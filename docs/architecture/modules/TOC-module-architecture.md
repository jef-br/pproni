# Pproni architecture: module pages

[Up: architecture](../TOC-architecture.md)

Detail moved out of source comments that had no other home in the docs
([comment rule](../../agent-knowledge/code-comments.md)). One page per module, or per folder of small files.

| Page | Source |
|---|---|
| [app.md](app.md) | `src/app/` (progress footer, one write per frame) |
| [build.md](build.md) | `build.rs` (the app icon) |
| [core.md](core.md) | `src/core/mod.rs`, `options.rs` (how `Options` travels) |
| [core-config.md](core-config.md) | `src/core/config.rs` (where each group of constants is explained) |
| [core-exporter.md](core-exporter.md) | `src/core/exporter/` (save then move, resize) |
| [core-framing.md](core-framing.md) | `src/core/framing.rs` (padding vs backdrop, `border_colour`, `deepen_peel`) |
| [core-preprocessor.md](core-preprocessor.md) | `src/core/preprocessor/mod.rs` (ICC: sRGB check, tintbox conversion) |
| [core-preprocessor-maria-antonieta.md](core-preprocessor-maria-antonieta.md) | headcut: YuNet input and outputs, why 320 |
| [core-preprocessor-optimizer.md](core-preprocessor-optimizer.md) | why the colour balance, the trims, the letterbox / bar detector |
| [core-processor-routes.md](core-processor-routes.md) | `processor/mod.rs`, `routes/mod.rs`, R2 `crop_square`, R3 `fallback` |
| [core-processor-routes-center-and-stretch.md](core-processor-routes-center-and-stretch.md) | R1: silhouette, the stretch, the flat gate, grazed lines (second test, patch fill), edge gradients, the solid-background look, seam-carve reframe |
| [core-runtime.md](core-runtime.md) | `src/core/runtime.rs` (why `DirectML.dll` is not embedded) |
| [core-shot-classifier.md](core-shot-classifier.md) | `src/core/shot_classifier/` wiring, `gate`, `gpu_adapters` |
| [core-shot-classifier-birefnet.md](core-shot-classifier-birefnet.md) | tensors, `DeformConv` rewrite, session memory, `logit`, loading |
| [core-shot-classifier-geometry.md](core-shot-classifier-geometry.md) | who goes through `EdgeView`, `Grid` implementors |
| [core-shot-classifier-plan.md](core-shot-classifier-plan.md) | R1's square: one rule for every edge count, when it is `None` |
| [core-shot-classifier-refine.md](core-shot-classifier-refine.md) | graze test, trimap run walk |
| [core-shot-classifier-route.md](core-shot-classifier-route.md) | the current R1/R2/R3 decision table, `plan_square` |
| [core-shot-classifier-sampling.md](core-shot-classifier-sampling.md) | summed-area table for the mask window, the colour distance |
| [core-shot-classifier-segmentation.md](core-shot-classifier-segmentation.md) | why the `Mask` is not pixel-accurate |
| [core-shot-classifier-shotcode.md](core-shot-classifier-shotcode.md) | the `BG` tag still to come; `mask_too_small` |
| [core-trace.md](core-trace.md) | stage numbers, colour legends, files per image |
| [examples.md](examples.md) | `run_dir` outputs, shot-code fields, runtime path |
| [main.md](main.md) | `src/main.rs` (command line) |
| [tests.md](tests.md) | what `tests/` covers |
