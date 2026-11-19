# core::config

[Up: architecture](../TOC.md)

Each constant's unit and owner stay at the constant in `src/core/config.rs`. Where the constants are explained:

* Trim constants (`TRIM_*`): [core::preprocessor::optimizer](core-preprocessor-optimizer.md) and
  [1. Pipeline: why two resolutions](../01-why-two-resolutions.md).
* R1 constants (`R1_*`): [core::processor::routes::center_and_stretch](core-processor-routes-center-and-stretch.md)
  and [spec §5.1](../../specs/05-routing.md).
