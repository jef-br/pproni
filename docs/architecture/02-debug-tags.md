# 2. Modules: debug tags (`PPRONI_DEBUG_TAGS` marker file)

[Up: table of contents](TOC.md)

Off by default — the shipped exe writes a clean `<stem>.jpg`. Drop a file named `PPRONI_DEBUG_TAGS`
next to the executable (content ignored, only presence matters; delete it to turn tagging back
off) and the exporter instead names each output from the shot classifier's verdict:

```
<stem>--EIX=<trbl>[--BGC=<hex>][--FGC=<hex>].jpg   the export, renamed
<stem>_segmask.jpg                                the segmentation mask, sRGB JPEG at output size (nearest)
```

Why a marker file: no shell, no environment variable, because Pproni ships as one file with nothing
to configure. It is the only thing Pproni ever looks for beside the exe; the runtime and the weights
are inside it. The segmask is upscaled nearest-neighbour to the output's size, so it overlays the
tagged output pixel-for-pixel.

`EIX` is the 4-bit top-right-bottom-left edge string — always the classifier's real verdict, the
same one `Route::select` routed on; `BGC`/`FGC` are the dominant background / weighted-median
foreground colours, each emitted only when sampleable. `=` stands in for the spec's `:` (illegal in
Windows filenames). `EIX` comes from `shot_classifier::shotcode::eix_bits`; `BGC`/`FGC` are sampled
once, by `exporter::colors`, from the true-colour `working` copy and the mask — never from
`model_input`. The `run_dir` dev harness uses the same two. With no verdict (a failed inference) there is no mask:
the name carries `EIX=____` and no segmask is written. The model is embedded and `core::preflight`
aborts the run if its session will not build, so a run of all-`EIX=____` files does not mean a
broken install: it points at the classifier or the images themselves.

`ShotClassification::mask_too_small` flags a mask that covers too little of the frame to be a
subject. It is informational for the tag and never overrides `eix` — a tag must never claim an edge
intersection the pipeline didn't actually route on. It doesn't appear in the filename today;
`run_dir`'s console output and `zzz_results.json` are where it's currently visible.

The same marker also drives `core::trace`: when it is present, every image writes a per-stage
trace (`<stem>  -  NN.jpg`, `<stem>  -  log.txt`) into the output image's folder, next to its
`<stem>_segmask.jpg`. One switch for all debug output, debug and release alike.
