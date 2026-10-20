# Paths and inputs

## The four folders (do not confuse them)

- **`JB core img batch`** — `C:\Users\JefB\Documents\JBGITROOT\csp\JB core img batch`
  The stable, always-relevant source set. Images are numbered `1.* … 49.*`
  This is the library the runner pulls from. Contents curated by the user.
  Agents read, never delete.

- **`PPRONI-INPUT`** — a *variable* working folder for an actual Pproni run.
  Its contents change: the user (or an agent, when asked) populate it.
  * Agents can pick any selection from `JB core img batch`.
  * Agents may ask the user permission to add other images.
  * The user can be asked to look for specific other images.
  * The runner must **never assume fixed contents** and read it at run time.
  - Path: `C:\Users\JefB\Desktop\PPRONI-INPUT`

- **`PPRONI-BACKUP`** — a *variable* working folder that contains the original
images. Use this folder to recover the originals for a before/after review.

- **`PPRONI-OUTPUT`** — a *variable* working folder that contains the processed
images. Use this folder to recover the "after" images.


## Run discipline

- Verification runs on a **copy**, never the original set. Copy into the
  session scratchpad and run there.
- Build: `cargo build --release` (produces the single hardened exe — see
  [hard-bounds.md](hard-bounds.md)).
  - Note: the agent's `cargo build` is blocked while `<jbpair>` tags remain
    (see [tag-protocol.md](tag-protocol.md)). The orchestrator asks the user for
    a manual build when one is essential.
- Debug tags: drop an (empty) file literally named `PPRONI_DEBUG_TAGS` next to
  the exe before the run — content ignored, presence only (same sidecar
  convention as the model/dll). It turns on verdict-tagged outputs:
  - `<stem>--EIX=<trbl>[--BGC=<hex>][--FGC=<hex>].jpg` — the renamed export
    (`=` stands in for `:`). **EIX is read straight from this filename** — so
    checks 5/6 (EIX stays `0000`) are a filename read, not a pixel scan.
  - `<stem>_segmask.jpg` — the segmentation mask, sRGB JPEG at output size.
  - Outputs land in the run's output dir (what `examples/run_dir.rs` writes to).
    Source of truth: `docs/ARCHITECTURE.md` §Debug tags, `core::exporter`.
