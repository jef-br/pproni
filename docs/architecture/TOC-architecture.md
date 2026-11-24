# Pproni architecture

[Up: docs home](../TOC-PProni.md)

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
| [02-modules-shot-classifier.md](02-modules-shot-classifier.md) | `core::shot_classifier`: gate, refine, geometry, segmentation, BiRefNet |
| [03-routing.md](03-routing.md) | R1 / R2 / R3, and how spec §5's rows live inside them |
| [04-known-gaps.md](04-known-gaps.md) | What is knowingly missing |
| [05-single-exe.md](05-single-exe.md) | One hardened exe, no install: embedded models, unpacked runtime |
| [06-diagram.md](06-diagram.md) | `JBA2B.drawio.svg`, what each container says, the maintenance rule |

## Module pages
One page per module, or per folder of small files, with the detail moved out of source comments: [TOC-module-architecture.md](modules/TOC-module-architecture.md).
