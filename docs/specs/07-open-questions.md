# 7 · Open questions

[Up: table of contents](TOC-specs.md)

- **Q7** · Small-source policy — **PLANNED** A source is **KO** when,
  after the trim, its largest dimension — or its segmask's largest dimension — falls below
  `800 / 1.42 ≈ 563 px` (the 800px output floor divided by the old 1.42× cap). A KO image is a
  failure: it stays in the INPUT folder, unprocessed, like the backdrop-only case.
