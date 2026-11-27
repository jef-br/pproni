# core::config

[Up: module pages](TOC-module-architecture.md)

Each constant's unit and owner stay at the constant in `src/core/config.rs`. Where the constants are explained:

* Trim constants (`TRIM_*`): [core::preprocessor::optimizer](core-preprocessor-optimizer.md) and
  [1. Pipeline: why two resolutions](../01-why-two-resolutions.md).
* R1 constants (`R1_*`): [core::processor::routes::center_and_stretch](core-processor-routes-center-and-stretch.md)
  and [spec §5.1](../../specs/05-routing.md).
* Shot classification (`WORKING_SIZE`, `GATE_MARGIN_PX`, `REFINE_*`, `GRAZE_RATIO_MAX`):
  [spec §3](../../specs/03-shot-classifier.md) and [1. Pipeline: why two resolutions](../01-why-two-resolutions.md).
* Mask size (`MASK_*`): [spec §5](../../specs/05-routing.md) and [debug tags](../02-debug-tags.md).
* White bars (`FRAMING_*`): [core::framing](core-framing.md) and
  [Decided 2026-10-21: trims and framing](../../work-state/2026-10-21-trims-and-framing.md).
* Output size (`MIN_SIZE`, `MAX_SIZE`): [core::exporter](core-exporter.md) and [spec §1](../../specs/01-principles.md).
* Edge detector (`T_*`): [core::preprocessor::optimizer](core-preprocessor-optimizer.md) and [core::framing](core-framing.md).
* Headcut on a bars image (`HEADCUT_BARS_*`):
  [2026-10-30: headcut on a bars image](../../work-state/2026-10-30-headcut-on-bars-image.md).
* Output JPEG quality (`JPEG_QUALITY`): [spec §1](../../specs/01-principles.md).
* Headcuts on or off (`PERMIT_HEADCUTS`): [spec §8](../../specs/08-build-status.md) and
  [2. Modules: app](../02-modules-app.md).