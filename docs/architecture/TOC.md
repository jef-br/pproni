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
| [core-exporter.md](modules/core-exporter.md) | `src/core/exporter/` (save then move, resize) |
| [core-preprocessor-maria-antonieta.md](modules/core-preprocessor-maria-antonieta.md) | headcut: YuNet input and outputs |
| [core-preprocessor-optimizer.md](modules/core-preprocessor-optimizer.md) | the letterbox / bar detector |
| [core-processor-routes.md](modules/core-processor-routes.md) | R2 `crop_square`, R3 `fallback` |
| [core-processor-routes-center-and-stretch.md](modules/core-processor-routes-center-and-stretch.md) | R1: why the background is not validated |
| [core-shot-classifier-birefnet.md](modules/core-shot-classifier-birefnet.md) | tensors, `DeformConv` rewrite, session memory, `logit` |
| [core-shot-classifier-refine.md](modules/core-shot-classifier-refine.md) | graze test, trimap run walk |
| [core-shot-classifier-route.md](modules/core-shot-classifier-route.md) | the current R1/R2/R3 decision table |
| [core-shot-classifier-sampling.md](modules/core-shot-classifier-sampling.md) | summed-area table for the mask window |
| [core-shot-classifier-shotcode.md](modules/core-shot-classifier-shotcode.md) | the `BG` tag still to come; `mask_too_small` |
| [core-trace.md](modules/core-trace.md) | stage numbers and colour legends |
| [examples.md](modules/examples.md) | `run_dir` outputs, shot-code fields, runtime path |
| [tests.md](modules/tests.md) | retired detection / geometry tests |
