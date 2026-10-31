# core::shot_classifier::shotcode

[Up: architecture](../TOC.md)

Detail moved out of `src/core/shot_classifier/shotcode.rs`. The tag itself (`EIX`/`BGC`/`FGC` file
names) is described in [2. Modules: debug tags](../02-debug-tags.md).

## The background-type tag still to come
The shot code is partial: a background-type (`BG`) tag is still to come, and it will cover the
case `mask_too_small` reports today — a mask covering too little of the frame to be a subject.
Routing (`route::decide`) reads only `touches_edges` and `mask_too_small`. Why `eix` must stay the
classifier's real verdict: [2. Modules: debug tags](../02-debug-tags.md).

## What `mask_too_small` catches
Nothing about *why* the mask is small is measured. The images this catches are close-ups where the
frame was already all product and the segmenter latched onto the highest-contrast detail in it, but
the route does not need that story to be true, only that there is no subject here worth framing
around.

## A condition measured and left out
A further condition — "and no uniform background was found" — was measured and left out. It is the
only thing that would tell a genuinely small product on clean paper, jewellery being the obvious
case, from this. Across the 37-image set the two rules flag exactly the same images, so it earns
nothing yet; add it back against a picture that needs it.
