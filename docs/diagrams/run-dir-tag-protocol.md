# run_dir: tagging a diagram Rectangle and naming its debug image

[Up: table of contents](TOC.md)

The protocol lives on `examples/run_dir.rs`, which currently writes two snapshots
(`SEGMASK`, `REFINE`). `JBA2B.drawio.svg` is the only diagram in this folder, so every
tagged Rectangle lives there. Out of scope for this protocol: the shipped exporter's
`PPRONI_DEBUG_TAGS` outputs (`<stem>_segmask.jpg` and the stage trace
`<stem>  -  NN.jpg`) each keep their own naming.

`examples/run_dir.rs` (a dev-only harness, see its own doc comment) writes one
intermediate image per internal decision point it snapshots. Each of those images is tied to
a specific Rectangle inside a Container on [JBA2B.drawio.svg](JBA2B.drawio.svg) — the detailed
per-decision flowchart. Whenever a new snapshot moment is added:

1. Find that Rectangle in its Container on `JBA2B.drawio.svg` and prepend its text with a
   one-word, ≤15-char title, bright green (`#00CC00`) and bold. Edit **both** layers or the
   tag is incomplete: the compressed mxGraph source in the SVG's `content=` attribute (the
   real editable source — decode/edit/recompress it, don't just touch the visible SVG, or
   the tag is lost next time the file is reopened in the VS Code drawio editor) and the
   rendered SVG's duplicate text (the `<div>` inside `<foreignObject>` *and* the `<text>`
   fallback — GitHub's SVG sanitizer may not render `foreignObject`, so the fallback is what
   shows there). Before writing a tag, check both layers for an existing one: copy its exact
   wording and colour, and fill in only the layer that lacks it. If no single existing rectangle covers the stage (branches merge or
   diverge), tag every terminal rectangle that could produce that snapshot with the same
   tag rather than inventing a new box.
2. Name the output file `<original stem>___<moduleindex>_<substep>_<TAG>.png`, so sorting
   filenames alphabetically reproduces real pipeline order:
   - `moduleindex` — one digit-word pair per Container, fixed regardless of what
     `run_dir` currently snapshots: `0-app`, `1-core`, `2-prep` (Preprocessor),
     `3-class` (Shot Classifier), `4-proc` (Processor), `5-expo` (Exporter). The digit
     exists because alphabetizing the words themselves gets the order wrong (e.g. `class` <
     `prep` < `proc` alphabetically, but Preprocessor actually runs before Shot Classifier).
     (`run_dir` skips the Preprocessor, so it has only used `3-class` so far.)
   - `substep` — a 2-digit number gapped by 10s (`10`, `20`, `30`, ...) reflecting real
     execution order *within* that Container. The gap leaves room to insert a step later
     (e.g. `15`) without renumbering its neighbors. Same reasoning as `moduleindex`: the
     TAG words won't reliably alphabetize in execution order on their own (`DETECT` <
     `SEGMASK` alphabetically, but the segmentation mask is actually computed first).
   - `TAG` — exactly the green-bold tag from step 1, verbatim, no extra prefix.
3. `run_dir` never creates per-image subfolders — output is one flat directory, so the
   filename alone (not folder placement) must carry both the pipeline order and the diagram
   traceability.
4. If the value the diagram points to isn't reachable from `examples/run_dir.rs` today,
   expose it through the classifier's existing public API — the way `run_dir` already reads
   `SegmentationModel::segment` and `classify_instance` — by adding a minimal `pub` accessor on
   the module that computes it. (The old `detect.rs` / `debug_*` wrapper pattern was retired with
   the classical detector; there is no `detect.rs` to route through now.) This touches core files;
   get an explicit OK first.

Current mapping (as of this writing, what `run_dir` writes): `3-class_10_SEGMASK` and
`3-class_20_REFINE`. These are the only two green tags on `JBA2B.drawio.svg`; the older
`PREPROC` / `SLIC` / `Geodesics` / `DETECT` / `FILL` / `FINAL` tags went with the classical
detector. Tags are copied verbatim (step 2), so a tag authored in mixed case, like the old
`Geodesics`, keeps that exact casing in the filename.
