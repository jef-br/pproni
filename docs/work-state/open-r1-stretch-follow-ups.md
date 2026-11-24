# Open: R1 stretch/inpaint follow-ups (2026-10-16)

[Up: table of contents](TOC-work-state.md)

* **Lifestyle / ambiance detection — CLOSED 2026-11-20 (user).** The case (41, snowboarder on
  mountains) is covered now: a textured scene with bars goes to `R1Job::Reframe`, and bars are always
  centred ([2026-10-23 note](2026-10-23-bars-always-centred.md)). No separate detection or planning talk needed.
* **Not tuned yet:** clearance (20 working px).
* **Telea / fast-marching inpaint** — kept on standby. `center_and_stretch::inpaint::fill_segment` now
  only fills lines with no background at all (flat BGC/white). If a wider inpainter is needed it
  slots in there.
* **ONNX LaMa-style inpaint kicker** — for truly textured scenes (lifestyle), the long-term
  option is embedded like BiRefNet (`include_bytes!`, as in `shot_classifier::birefnet`).
* **BGC confidence** — `colors::sample` returns a binary pass/fail. R1 now judges per line whether
  the BGC is "close enough" (inside the nearby background's min/max), so a weak BGC is simply not
  believed there. A dominant-cluster fraction is no longer needed for R1.
* **Open 2026-11-19:** the solid-background look's threshold (steepest step 0.5) rests on three images. The second
  test's tolerance (3 levels) was confirmed by the user on 2026-11-20. Details: [2026-11-13 note](2026-11-13-flat-edge-gradients.md).
