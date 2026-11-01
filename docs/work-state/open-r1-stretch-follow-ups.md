# Open: R1 stretch/inpaint follow-ups (2026-10-16)

[Up: table of contents](TOC.md)

* **Lifestyle / ambiance detection** — 2026-09-21-0 (snowboarder on mountains) still outputs
  white where the gated edges land, because the local-BGC fallback correctly refuses the
  textured strip. The user's hint was that preprocess's 16 border samples (`optimizer::border_samples`)
  plus the `prep.crop` fill-ratio can flag the image as lifestyle *before* R1 runs, after which
  the correct output is a per-edge content-aware stretch outside the segmask rather than white.
  **The current plan's write-up of these characteristics is likely a misinterpretation of what
  the user meant.** A dedicated planning conversation is needed — walk through the user's
  definition of lifestyle/ambiance (what the image looks like, what the segmask + border
  samples will show, what the desired output is) and agree on the routing *before* any code.
* **Not tuned yet:** clearance (20 working px).
* **Telea / fast-marching inpaint** — kept on standby. `center_and_stretch::inpaint::fill_segment` now
  only fills lines with no background at all (flat BGC/white). If a wider inpainter is needed it
  slots in there.
* **ONNX LaMa-style inpaint kicker** — for truly textured scenes (lifestyle), the long-term
  option is embedded like BiRefNet (`shot_classifier::birefnet` + `cipher` + `build.rs`).
* **BGC confidence** — `colors::sample` returns a binary pass/fail. R1 now judges per line whether
  the BGC is "close enough" (inside the nearby background's min/max), so a weak BGC is simply not
  believed there. A dominant-cluster fraction is no longer needed for R1.
