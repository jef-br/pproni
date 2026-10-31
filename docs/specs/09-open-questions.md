# 9 · Open questions

[Up: table of contents](TOC.md)

- **Q1** · `LOSS_MAX` value ([§6.4](06-crop-compose-r2.md) ①) — R2 only now; R1 never clips the subject. **Dormant:** [§6](06-crop-compose-r2.md)
  (CoG/MSR) is `DEFERRED`, so this blocks nothing until R2 saliency is built.
- **Q2** · CoG definition — saliency-weighted centroid confirmed? ([§6.4](06-crop-compose-r2.md) ②) **Dormant:** [§6](06-crop-compose-r2.md) is
  `DEFERRED`, so this blocks nothing until R2 saliency is built.
- **Q3** · ~~SC output contract~~ — **answered**, see [§3.2](03-shot-classifier.md).
- **Q5** · Human detector interface — what signal it returns and how [§6](06-crop-compose-r2.md) consumes it. **Parked:
  Phase 2** — not specified until that work starts.
- **Q6** · ~~R2's five behaviours~~ — **answered**: four of them are one rule and live in R1 ([§5.1](05-routing.md)). R2 is left with the full bleed.
- **Q7** · ~~Small-source policy~~ — **resolved 2026-10-20 (`PLANNED`).** A source is **KO** when,
  after the trim, its largest dimension — or its segmask's largest dimension — falls below
  `800 / 1.42 ≈ 563 px` (the 800px output floor divided by the old 1.42× cap). A KO image is a
  failure: it stays in the INPUT folder, unprocessed, like the backdrop-only case. Not yet built.
