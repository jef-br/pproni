# Pproni architecture

[Up: docs home](../TOC.md)

Map of every module, its public surface, and how data flows through the pipeline. Code
documentation: source comments point here instead of repeating it.

**Maintenance rule** (keep the diagram in step with the code): [6. The diagram](06-diagram.md).

| File | What it covers |
|---|---|
| [01-pipeline.md](01-pipeline.md) | The five stages, the framing step, the stage table |
| [01-why-two-resolutions.md](01-why-two-resolutions.md) | `original` / `working` / `model_input`; the optimiser: colour balance, trim, trim 2 |
| [01-why-classification-is-optional.md](01-why-classification-is-optional.md) | `None` (no verdict) vs an empty edge list |
| [02-modules-app.md](02-modules-app.md) | `app`: batch, progress, shell, header, theme, console, desktop, i18n |
| [02-modules-core.md](02-modules-core.md) | `core`: config, preprocessor, headcut, framing, routes, exporter; bars/frame; R1 margin |
| [02-debug-tags.md](02-debug-tags.md) | The `PPRONI_DEBUG_TAGS` marker: EIX/BGC/FGC file names, segmask, stage trace |
| [02-modules-shot-classifier.md](02-modules-shot-classifier.md) | `core::shot_classifier`: gate, refine, geometry, segmentation, BiRefNet, cipher |
| [03-routing.md](03-routing.md) | R1 / R2 / R3, and how spec §5's rows live inside them |
| [04-known-gaps.md](04-known-gaps.md) | What is knowingly missing |
| [05-single-exe.md](05-single-exe.md) | One hardened exe, no install: embedded encrypted models, unpacked runtime |
| [06-diagram.md](06-diagram.md) | `JBA2B.drawio.svg`, what each container says, the maintenance rule |

## Module pages
Detail moved out of source comments that had no other home in the docs
([comment rule](../agent-knowledge/code-comments.md)). One page per module, or per folder of small files.

| Page | Source |
|---|---|
| [app.md](modules/app.md) | `src/app/` (progress footer, one write per frame) |
| [build.md](modules/build.md) | `build.rs` (icon, fixed encrypted names, symmetric cipher) |
| [core.md](modules/core.md) | `src/core/mod.rs`, `options.rs` (how `Options` travels) |
| [core-config.md](modules/core-config.md) | `src/core/config.rs` (where each group of constants is explained) |
| [core-exporter.md](modules/core-exporter.md) | `src/core/exporter/` (save then move, resize) |
| [core-framing.md](modules/core-framing.md) | `src/core/framing.rs` (padding vs backdrop, `border_colour`, `deepen_peel`) |
| [core-preprocessor.md](modules/core-preprocessor.md) | `src/core/preprocessor/mod.rs` (ICC: sRGB check, tintbox conversion) |
| [core-preprocessor-maria-antonieta.md](modules/core-preprocessor-maria-antonieta.md) | headcut: YuNet input and outputs, why 320 |
| [core-preprocessor-optimizer.md](modules/core-preprocessor-optimizer.md) | why the colour balance, the trims, the letterbox / bar detector |
| [core-processor-routes.md](modules/core-processor-routes.md) | `processor/mod.rs`, `routes/mod.rs`, R2 `crop_square`, R3 `fallback` |
| [core-processor-routes-center-and-stretch.md](modules/core-processor-routes-center-and-stretch.md) | R1: silhouette, the stretch, grain, seam-carve reframe, flat fill |
| [core-runtime.md](modules/core-runtime.md) | `src/core/runtime.rs` (DirectML preload) |
| [core-shot-classifier.md](modules/core-shot-classifier.md) | `src/core/shot_classifier/` wiring, `gate`, `cipher`, `gpu_adapters` |
| [core-shot-classifier-birefnet.md](modules/core-shot-classifier-birefnet.md) | tensors, `DeformConv` rewrite, session memory, `logit`, loading |
| [core-shot-classifier-geometry.md](modules/core-shot-classifier-geometry.md) | who goes through `EdgeView`, `Grid` implementors |
| [core-shot-classifier-plan.md](modules/core-shot-classifier-plan.md) | R1's square: one rule for every edge count, when it is `None` |
| [core-shot-classifier-refine.md](modules/core-shot-classifier-refine.md) | graze test, trimap run walk |
| [core-shot-classifier-route.md](modules/core-shot-classifier-route.md) | the current R1/R2/R3 decision table, `plan_square` |
| [core-shot-classifier-sampling.md](modules/core-shot-classifier-sampling.md) | summed-area table for the mask window, the colour distance |
| [core-shot-classifier-segmentation.md](modules/core-shot-classifier-segmentation.md) | why the `Mask` is not pixel-accurate |
| [core-shot-classifier-shotcode.md](modules/core-shot-classifier-shotcode.md) | the `BG` tag still to come; `mask_too_small` |
| [core-trace.md](modules/core-trace.md) | stage numbers, colour legends, files per image |
| [examples.md](modules/examples.md) | `run_dir` outputs, shot-code fields, runtime path |
| [main.md](modules/main.md) | `src/main.rs` (command line) |
| [tests.md](modules/tests.md) | what `tests/` covers |
