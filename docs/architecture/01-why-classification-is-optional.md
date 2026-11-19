# 1. Pipeline: why classification is optional

[Up: table of contents](TOC.md)

`classify` returns `Option`. `None` means *no verdict* — the model is unavailable, or inference
failed on this image. That is deliberately distinct from `Some` with an empty `touches_edges`, which
means *we looked, and the subject touches no edge*.

They route differently, so a model failure degrades one image to a safe framing instead of being
silently processed as a clean free-standing shot.
