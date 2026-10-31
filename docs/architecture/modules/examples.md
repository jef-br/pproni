# examples

[Up: architecture](../TOC.md)

The two dev harnesses in `examples/`: `run_dir` (batch-runs the shot classifier) and
`inspect_birefnet` (prints a model's tensors). Neither ships in the exe.

## run_dir: files written
For every input, into `<output_dir>`:

```
<stem>--EIX=<trbl>[--BGC=<hex>][--FGC=<hex>].<ext>
        step 0: the input image, copied verbatim, renamed with the shot code derived so far
<stem>___3-class_10_SEGMASK.png
        step 1: BiRefNet foreground mask, full frame
<stem>___3-class_20_REFINE.png
        step 2: white canvas the size of the segmask, with each gated edge's refined region
        pasted back where it sits in the frame (only if an edge was gated)
```

Plus `zzz_results.json` (named to sort last) for a folder run, or `<stem>_results.json` for a
single-file run. Snapshot names (`<stem>___<moduleindex>_<substep>_<TAG>.png`) follow the
[run_dir tag protocol](../../diagrams/run-dir-tag-protocol.md): sorting them alphabetically
reproduces pipeline order, and the flat output keeps each image's files together.

## run_dir: the shot code
Partial: the BG type comes later.

* **EIX** — edge intersection, 4-bit TRBL (top-right-bottom-left). `1000` = top only, `0110` =
  right+bottom, `0000` = no intersection. Always the classifier's real verdict — a
  `mask_too_small` flag (probable full-bleed close-up: tiny mask, no uniform background) is
  reported separately (console/JSON) and never overrides it.
* **BGC** — background colour: dominant colour of the inverse BiRefNet mask, emitted only when it
  covers enough of that region. The old `run_dir` comment said >97.5%; the threshold actually
  used is `BGC_PRESENCE` in `src/core/exporter/colors.rs` (0.9 at the time of writing).
* **FGC** — foreground colour: weighted-median colour of the largest colour blob inside the mask.
  Omitted if the subject is too small to sample.
* `--` separates the stem from the tags; `=` stands in for the spec's `:` (a colon is illegal in
  Windows filenames).

The EIX bits come from `pproni::core::shot_classifier::shotcode`, the colours from
`pproni::core::exporter::colors` — the same code the pipeline exporter uses for its
`PPRONI_DEBUG_TAGS` output ([debug tags](../02-debug-tags.md)).

## Both: why the runtime path is made absolute
Both harnesses hand the loader an absolute path. A bare file name goes to the OS library search,
which reaches System32 — where Windows ships its own, older `onnxruntime.dll` — before it ever
looks at the working directory. The result is a version-mismatch panic naming a DLL you did not
choose. `join` on an already-absolute `ORT_DYLIB_PATH` keeps it unchanged.
