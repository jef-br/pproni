# core::exporter

[Up: module pages](TOC-module-architecture.md)

The last pipeline stage: size envelope, JPEG save, original moved to the backup folder. Public
surface: [module map](../02-modules-core.md). File names and segmask under the debug marker:
[debug tags](../02-debug-tags.md).

## mod.rs

* **Save, then move.** Every route ends in `export`, so the output size envelope is applied here,
  in one place, and no route can bypass it. The original is moved out of `PPRONI-INPUT` into
  `PPRONI-BACKUP` only after a successful save, so a write failure leaves the source untouched for
  a retry. Originals are never lost: they are moved, or copied to `PPRONI-BACKUP` and then removed from `PPRONI-INPUT` when a move is not possible. They keep their original name and extension under `PPRONI-BACKUP`,  in the same subfolder layout the output uses.
* The debug marker (why a file, the segmask, a run of all `EIX=____`): [debug tags](../02-debug-tags.md).

## resize.rs

The envelope itself (`MIN_SIZE` to `MAX_SIZE` on the longest side, 800 to 2000 px) is [spec §1](../../specs/01-principles.md). It is set in `src/core/config.rs`; `to_envelope` applies them."
