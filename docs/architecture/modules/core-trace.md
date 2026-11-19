# core::trace

[Up: architecture](../TOC.md)

The per-stage debug trace, on only under the `PPRONI_DEBUG_TAGS` marker: [debug tags](../02-debug-tags.md).

## Stage pictures and their colours

What each stage number in `core::trace::stage` writes as `<stem>  -  <NN>.jpg`.

| NN | Constant | Picture |
|---|---|---|
| 01 | `ORIGINAL` | Preprocessing: the original, after EXIF, alpha, flatten and colour profile (exif+alpha+flat+color-profile) |
| 02 | `LETTERBOX` | Preprocessing: the flat-bar check on the original. Emitted for every image |
| 03 | `TRIM` | Preprocessing: the two trims on the colour-balanced working copy. Emitted for every image |
| 10 | `PRIMARY` | Shot classifier: the model input; 11, 12 and 15 follow from it |
| 90 | `R1_PLAN` | Plan view on the original. Emitted for every image |

* **02, the flat-bar check.** A bar along each edge: green = backdrop, orange = flat in another
  colour, magenta = varied. Dimmed = the bars the check would trim. Cyan = the product proxy; the
  bright rest is where texture is measured.
* **03, the trims.** Cyan = trim 1's tight box. Yellow = trim 2's tight box (drawn only when it
  replaced trim 1). Green = the backed-off crop BiRefNet is shown.
* **10 and up, the model.** 10 = model input; +1 = ImageNet-normalised input; +2 = raw logits, with
  L=0 → 128, L<0 → 0-127, L>0 → 129-255; +5 = segmask at working resolution.
* **90, the plan view.** Subject box (cyan), R1's planned square (yellow), grey = invented area, and
  semi-transparent edge bars for the gate/verdict: red = touches, orange = gated but cleared. Subject
  and square are optional: a missing verdict or an impossible plan still produce a useful picture.
  The grey padding is exactly what R1 has to invent by stretching; a missing subject skips the cyan
  outline, a missing plan the yellow square and the grey padding.

## Files per image
Into the output image's folder:

* `<stem>  -  <NN>.jpg`, one per stage. `NN` is fixed per stage in `core::trace::stage`: a skipped
  stage leaves a gap, nothing is renumbered.
* `<stem>  -  log.txt`: the stages written, in order, plus the numbers behind each decision (mask
  coverage, which edges the gate flagged, the graze ratio, the route and why).

A stage's picture is built by a closure that runs only while tracing, so an overlay costs nothing
in a normal run.

## Stage 90 in detail (`trace_stage_90`, `plan_view`)
* The same drawing code runs on every image; a value that is absent (no bbox, no plan, no overhang)
  is simply not drawn, and not written to the log.
* The header names the route the classifier chose, which is the route that runs, so the picture and
  the routing decision are read together, not from two separate places.
