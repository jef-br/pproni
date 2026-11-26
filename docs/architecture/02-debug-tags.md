# 2. Modules: debug tags (`PPRONI_DEBUG_TAGS` marker file)

[Up: table of contents](TOC-architecture.md)

Off by default — the shipped exe writes a clean `<stem>.jpg`. Drop a file named `PPRONI_DEBUG_TAGS`
next to the executable (content ignored, only presence matters; delete it to turn tagging back
off) and the exporter instead names each output from the shot classifier's verdict:

```
<stem>--EIX=<trbl>[--BGC=<hex>][--FGC=<hex>].jpg   the export, renamed
<stem>_segmask.jpg                                the segmentation mask, sRGB JPEG at output size (nearest)
```

Why a marker file: no shell, no environment variable, because Pproni ships as one file with nothing
to configure. It is the only thing Pproni ever looks for beside the exe; the runtime and the weights
are inside it. BiRefNet looks at a square: it squashes the picture it is shown to 512×512. Its mask is
mapped back to the proportions of that picture (trace stage 15, `<stem>  -  15.jpg`). The segmask file
is that mask stretched nearest-neighbour to the output's width and height, so it is squashed into the
output's shape (a square, for the three routes). The output picture is cropped or padded by its route
and the mask is not, so the two do not line up pixel for pixel: use the segmask to see what the model
found, and stage 15 to see it in the picture's own proportions.

[`EIX`](../specs/03-eix.md) is the edge verdict. `BGC`/`FGC` are the dominant
background / weighted-median foreground colours, each emitted only when sampleable. `=` stands in
for the spec's `:` (illegal in Windows filenames). `BGC`/`FGC` are sampled once, by
`exporter::colors`, from the true-colour `working` copy and the mask — never from `model_input`.
The `run_dir` dev harness uses the same code for all three tags. With no verdict (`EIX=____`) there
is no mask, so no segmask is written. `core::preflight` aborts the run if the model won't load, so
these files do not mean a broken install.

`ShotClassification::mask_too_small` is a source of information: the router reads it and sends the
image to R2. It is true when the mask covers less than 5% of the frame (`MASK_MIN_COVERAGE`) and its
longest side in the original picture is under 733 px (`MASK_MIN_LONGEST_SIDE_PX`: `MIN_SIZE` less
R1's margin). It never changes `EIX`. The flag is not in the file name. You can see it in the trace log, in `run_dir`'s console and in
`zzz_results.json`.

The same marker also drives `core::trace`: when it is present, every image writes a per-stage
trace (`<stem>  -  NN.jpg`, `<stem>  -  log.txt`) into the output image's folder, next to its
`<stem>_segmask.jpg`. One switch for all debug output, debug and release alike.
