# core::processor::routes

[Up: architecture](../TOC.md)

The three routes that turn a classified picture into its square. What each one does and which
verdict picks it: [routing](../03-routing.md) and [spec §5](../../specs/05-routing.md). Why the
decisions sit in `shot_classifier::route` and not in the routes:
[work state 2026-10-29](../../work-state/2026-10-29-routing-in-shot-classifier.md).

## processor/mod.rs

`core::processor` holds one thing: `routes`, the routes a classified image can take to reach its
square canvas.

## routes/mod.rs

The three routes: R1 `center_and_stretch` ([its own page](core-processor-routes-center-and-stretch.md)),
R2 `crop_square`, R3 `fallback`. Each one turns the input picture into the output picture, and
nothing else. Which route runs, and with what plan, is decided before it by
`shot_classifier::route` (`decide`), and `send` then hands the image to the route. A route never
hands the image on to another one. The one check left inside a route is R1's seam-carve quality
gate on a textured scene, which needs the pixels it has just produced.

## crop_square.rs (R2)

The route for images where framing *around* the subject is the wrong move, either because there is
no usable subject to frame around or because the frame cannot grow to hold one. It adds nothing: it
takes the largest square the image already contains, centred.

Why each of its three verdicts (decided in `shot_classifier::route::decide`) lands here:

* **The mask covers too little of the frame to be a subject.** A close-up where the segmenter
  latched onto one high-contrast detail. Cropping to that detail magnifies a speck; padding around
  the whole image adds white to a picture that is already all product. Taking a square out of it is
  the only move that neither invents nor magnifies.
* **Left and right both bled, and nothing else.** The subject spans the full width, so the square
  can be at most that wide, and the subject is taller than that. R1 would have to stretch a blocked
  axis to fit, which would paint background over a subject running off-frame. Cropping into the
  height is the answer instead.
* **All four edges bled.** The subject's box *is* the frame. Nowhere to put a margin, no background
  band anywhere to stretch from.

**Centred on the frame, not on the mask.** Two of the three cases arrive here precisely because the
mask is not to be trusted, and the third has a subject spanning the whole width, so the frame is the
more reliable anchor in all three.

**Room left for CoG/MSR.** [Spec §6](../../specs/06-crop-compose-r2.md)'s CoG/MSR square is the
refinement this leaves room for: anchor the crop on the saliency-weighted centre rather than the
frame's middle. Nothing in the 37-image set needs it yet.

## fallback.rs (R3)

Reached when the classifier could not produce an answer at all: a failed inference, an unusable
model output. There is no subject box to anchor on, so the route makes no claim about where the
subject is: it fits the *whole* image, uncropped, into a square canvas sized to the original's
largest dimension, centred, with the remainder filled white.

Because a wrong guess here costs framing, never pixels ([routing](../03-routing.md)), it is the safe
answer for "we don't know", which is why it is a route of its own rather than a degenerate case of
R1.
