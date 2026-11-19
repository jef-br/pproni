# core::preprocessor

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/preprocessor/mod.rs` comments. Why the image is kept at two
resolutions, and what the optimiser does to `model_input`:
[1. Pipeline: why two resolutions](../01-why-two-resolutions.md). The bar check `preprocess` runs:
[core::preprocessor::optimizer](core-preprocessor-optimizer.md).

## Colour management

* A source tagged with a non-sRGB RGB profile is colour-managed into sRGB in `preprocess`, before any
  downstream pixel math runs on it. An untagged image, or one already tagged sRGB, is left untouched.
* **sRGB check** (`is_srgb_profile`). Converting an image that is already sRGB into sRGB changes no
  pixel, and at full resolution it is the slowest step before the model, so it is skipped. The test
  is the name in the profile's `desc` tag, which comes in two encodings: ASCII, or UTF-16 big-endian.
  Bytes that are not a profile, or a profile that is not RGB, do not count as sRGB.
* **Conversion** (`convert_to_srgb`) runs through tintbox, a pure-Rust colour engine that is
  bit-identical to lcms2. Anything that doesn't parse as an RGB profile, or that tintbox can't build a
  transform for, leaves the pixels untouched: the same fallback as no profile at all.
