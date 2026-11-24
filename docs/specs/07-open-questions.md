# 7 · Open questions

[Up: table of contents](TOC-specs.md)

- **Q3** · ~~SC output contract~~ — **answered**, see [§3.2](03-shot-classifier.md).
- **Q6** · ~~R2's five behaviours~~ — **BUILT**: four of them are one rule and live in R1 ([§5.1](05-routing.md)). R2 is left with the full bleed.
- **Q7** · Small-source policy — **PLANNED** A source is **KO** when,
  after the trim, its largest dimension — or its segmask's largest dimension — falls below
  `800 / 1.42 ≈ 563 px` (the 800px output floor divided by the old 1.42× cap). A KO image is a
  failure: it stays in the INPUT folder, unprocessed, like the backdrop-only case. Not yet built.
