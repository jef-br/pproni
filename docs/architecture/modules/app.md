# app

[Up: architecture](../TOC.md)

Detail moved out of the `src/app/` comments. The module table is
[2. Modules: `app`](../02-modules-app.md).

## progress.rs

### The footer under the bar
The footer is the block the shell wants held under the bar — the rule / callout / rule that tells
the user how to interrupt. It is drawn once, with the first frame, and then left alone: only the
bar line ever changes, so every later frame steps the cursor up to that one line, rewrites it, and
steps back down. Nothing else writes to stdout while the batch runs (failures are collected, not
printed), so the block stays where it was first drawn.

### Why each frame is one write
Each frame goes out in a single `write_all`. Rust's stdout is a `LineWriter` and flushes on every
newline it is handed, so printing the block line by line put a half-drawn frame on screen fifteen
times a second — which the terminal is free to composite. That was the flicker.
