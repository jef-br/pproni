# core::preprocessor::optimizer

[Up: architecture](../TOC.md)

Detail moved out of the `src/core/preprocessor/optimizer.rs` comments. The optimiser itself (colour
balance, trim 1, trim 2) is [1. Pipeline: why two resolutions](../01-why-two-resolutions.md); this
page holds only what that page does not say.

## Why the colour balance

The segmentation model finds a subject more reliably when the picture uses its full tonal range and
its midtones sit where the subject/background contrast is easiest to read. It is a Photoshop "Levels"
move, worked out once for the whole image.

**One shared range.** Every channel votes, even one with no spread: a pure-red image has `lo` = 0 from
its flat green and blue. One range keeps the colour balance (every channel is remapped by the same
amounts) and keeps clipping to a minimum (the widest range any channel needs is the one used).

**The gamma.** 1.00 is neutral; above 1 lightens the midtones and below 1 darkens them. A higher `lo`
pushes it up and a lower `hi` pulls it down: it leans against the brightness shift the remap to
`lo`..`hi` causes.

**The table.** `build_table` spreads `lo`..`hi` over 0-255 (at or below `lo` becomes 0, at or above
`hi` becomes 255), then bends the midtones by the curve `255 * t ^ (1 / gamma)` for `t` between 0 and
1, so 0 and 255 stay put. `gamma` stays at full precision and each entry is rounded once, as it is
made, so every pixel afterwards costs one table read and nothing else.

## The trims

**Why trim 1.** Cropping away the empty backdrop makes the product fill more of what the model looks
at. The model is shown trim 1's box backed off, and runs once: its mask is the one reliable fact about
the picture, which is why both boxes are handed to `core::framing` to be read against it.

**Outlier drop.** The rule (`drop_outliers`) is on the pipeline page linked above. The L1 distance is
`|ΔR|+|ΔG|+|ΔB|`, and MAD is the median of those distances (16 on the frame border). The floor
guards near-uniform edges from being shredded; the MAD term lets noisier edges keep a wider spread.
The `TRIM_MIN_KEEP` closest are always kept so a mostly-product edge can't collapse the backdrop range
to one colour.

**Product test.** In `product_box` (the backdrop closed with a square window), anything thinner than
a `TRIM_CLOSE_K` x `TRIM_CLOSE_K` block of product pixels, and everything outside the frame, is
backdrop; the box spans every such block.

## The letterbox detector (plan Part C)

The per-edge trim + texture detector is a measurement primitive, upstream of the mask; its per-edge
part is shared with the deferred Part D. It reports, per edge, whether that border is a flat band
and how it relates to the frame's backdrop (`EdgeKind`: `Backdrop`, `FlatOther`, `Varied`), plus
whether the background outside the product proxy is textured. The proxy is `product_box`, since
there is no segmask this far upstream. `detect_letterbox` reads it to find flat padding bars on a
textured scene; `preprocess` calls it on the full-resolution image, before the mask exists, and keeps the result
in `Prepared::letterbox`; `original` itself is left whole. R1 reads `letterbox` and re-frames the
trimmed part (`extend::reframe_to_square`). The motivating images: mountain and 268 (flat bars on
a textured scene).

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
