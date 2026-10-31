# core::preprocessor::optimizer

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/preprocessor/optimizer.rs` comments. The optimiser itself (colour
balance, trim 1, trim 2) is [1. Pipeline: why two resolutions](../01-why-two-resolutions.md).

## The letterbox detector (plan Part C)

The per-edge trim + texture detector is a measurement primitive, upstream of the mask; its per-edge
part is shared with the deferred Part D. It reports, per edge, whether that border is a flat band
and how it relates to the frame's backdrop (`EdgeKind`: `Backdrop`, `FlatOther`, `Varied`), plus
whether the background outside the product proxy is textured. The proxy is `product_box`, since
there is no segmask this far upstream. `detect_letterbox` reads it to find flat padding bars on a
textured scene; `preprocess` calls it on the full-resolution image and keeps the result in
`Prepared::letterbox`, which R1 reads.

## When bars are trimmed

The plan's table: an opposite pair of edges that are both `FlatOther` (flat, in a colour that is not
the frame's backdrop) while the other pair is `Varied` is padding on that axis, and it is trimmed.
Every other mix leaves the image whole. That is the hard bound (the EIX no-flip bound in
[hard bounds](../../agent-knowledge/hard-bounds.md)): a freestanding product on a plain backdrop has
`Backdrop` edges, so it can never be trimmed and `EIX0000` cannot flip.

Because the backdrop is the median of the border samples, two same-coloured bars read as the
backdrop themselves (10 of the 16 samples sit in them), so only bars that differ from the border
median — in practice two differently coloured bars — are trimmed. Same-coloured bars are handled
later by `core::framing` ([work state 2026-10-21: R1 stretch rework](../../work-state/2026-10-21-r1-stretch-rework.md)).

Texture is then measured on the trimmed interior, outside its product proxy; a flat interior is a
frame (the deferred Part D case) and is left alone.
