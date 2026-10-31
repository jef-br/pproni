# Decided 2026-10-23: bars are always centred (user)

[Up: table of contents](TOC.md)

* `Decision::Crop` is gone. Every bars case (trim 1 cut both sides of one axis, similar widths, subject
  touches an edge) is `Centre`: the trimmed content is centred with equal white bands. Only 41 changed
  in the 49 set; 46 was already centred. The texture input of `framing::decide` went with it.
* Why: lifestyle / ambiance backgrounds (41, 46) could not be stretched convincingly. Tried in scratch
  (not in the tree): an even stretch with nothing invented; a stretch with per-pixel caps (+300% smooth,
  +50% busy); Criminisi patch fill for what is left, with a Poisson fit, a reuse penalty and measured
  grain. The result on 41 looked plausible at normal size, but seams, repetition or foreign content
  stayed visible when zoomed in.
* Findings worth keeping (scratch only, may come back): a plain even stretch with no mirror, blur or
  noise fixed the artefacts on 1, 22, 25-27, 36 and 48; blur follows the peak local magnification, so
  spread the stretch evenly. The Hough-line pin (lines near the mask that reach a border freeze that
  edge) fixed the snowboard on 41; it is not in the tree either.
