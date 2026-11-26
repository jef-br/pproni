# 1. Pipeline: why classification is optional

[Up: table of contents](TOC-architecture.md)

`classify` returns `Option`.

* **An empty mask** is a normal answer: the model found no subject. It is `Some`:
  `mask_too_small` is true, the tag reads `EIX=0000`, a segmask is written, the image goes to R2.
* **`None`** is no verdict ([spec: EIX](../specs/03-eix.md)). It comes only from a model error (an
  `Err` from the model run on this image). That is a bug, not a Pproni outcome. Today `classify`
  drops it silently: the image is tagged `EIX=____`, gets no segmask, and goes to R3. Reporting it
  is #31.

`core::preflight` stops Pproni at startup if the model won't load so a missing model cannot cause `None`.