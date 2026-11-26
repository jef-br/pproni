# Paths and inputs

## The four folders (do not confuse them)

- **`JB core img batch`** — `C:\Users\JefB\Documents\JBGITROOT\csp\JB core img batch`
  The stable, always-relevant source set. Images are numbered `1.* … 51.*`
  This is the library the runner pulls from. Contents curated by the user.
  Agents read, never delete.

- **`PPRONI-INPUT`** — a *variable* working folder for an actual Pproni run.
  Its contents change: the user (or an agent, when asked) populate it.
  * Agents can pick any selection from `JB core img batch`.
  * Agents may ask the user permission to add other images.
  * The user can be asked to look for specific other images.
  * The runner must **never assume fixed contents** and read it at run time.
  - Path: `<the user's Desktop folder>\PPRONI-INPUT` (for example: `C:\Users\JefB\Desktop\PPRONI-INPUT`)

- **`PPRONI-BACKUP`** — a *variable* working folder that contains the original
images. Use this folder to recover the originals for a before/after review.

- **`PPRONI-OUTPUT`** — a *variable* working folder that contains the processed
images. Use this folder to recover the "after" images.

## Other `JB` folders
Folders at the repo root whose name starts with `JB` belong to the user: the user uses and controls
them, agents do not reference or touch them. They hold sample images, work notes or temporary files
for one piece of Pproni, and disappear when that piece is done. Today: `JB cuts back only` and
`JB headcut images`. The one exception is `JB core img batch` above, which agents read.

## Run discipline

- Verification runs on a **copy**, never the original set. Copy into the
  session scratchpad and run there. A run moves files: after a successful save the original goes
  from `PPRONI-INPUT` to `PPRONI-BACKUP` and the processed image lands in `PPRONI-OUTPUT`. A file
  that fails stays in `PPRONI-INPUT` untouched; nothing is copied to `PPRONI-BACKUP`
  (`app::batch::run`, `core::exporter`).
- Build: `cargo build --release` (produces the single hardened exe — see
  [hard-bounds.md](hard-bounds.md)).
- Debug tags: drop an (empty) file literally named `PPRONI_DEBUG_TAGS` next to
  the exe before the run — content ignored, presence only. It turns on
  verdict-tagged outputs:
  - `<stem>--EIX=<trbl>[--BGC=<hex>][--FGC=<hex>].jpg` — the renamed export
    (`=` stands in for `:`). **EIX is read straight from this filename** — so
    check 5 (EIX stays `0000`) is a filename read, not a pixel scan.
  - With no verdict the name is `<stem>--EIX=____.jpg` and no segmask is written. What EIX and
    `____` mean: [spec: EIX](../specs/03-eix.md).
  - `<stem>_segmask.jpg` — the segmentation mask, sRGB JPEG at output size.
  - The same marker also switches on the per-stage trace and `log.txt`
    (see [architecture: debug tags](../architecture/02-debug-tags.md)).
  - Outputs land in the output folder the exe was given. (The dev harness `examples/run_dir.rs`
    writes its own names, e.g. `___3-class_10_SEGMASK.png`.)
    Source of truth: [architecture: debug tags](../architecture/02-debug-tags.md), `core::exporter`.
