# Decided (standing)

[Up: table of contents](TOC.md)

* The 512 model (`birefnet_lite_512.onnx`) is canonical — proven to work, and what `build.rs`
  bakes in (`MODEL_FILE`).
* The preprocessor ends with one function, `preprocessor::optimizer::optimize`: colour balance, then
  the trim the user spec'd. Its output is the only image BiRefNet sees. How both steps
  work, the backdrop-only `Err` and the mask paste-back: [architecture: why two resolutions](../architecture/01-why-two-resolutions.md).
* `working` and `original` stay true colour. BGC/FGC are sampled from `working`.
* The 5% rule (`MASK_MIN_COVERAGE`) is the user's call. Never change it unasked. it's the only 5% rule.
  Since 2026-10-29 it has the longest-side exception ([2026-10-29](2026-10-29-routing-in-shot-classifier.md)).
* No retries, no guards, no fallbacks anywhere in the trim. Add none without asking.
* Goal now: improve results by refining the preprocessor, not by fine-tuning the model.
* The classical detector and its detection/geometry tests are retired.
