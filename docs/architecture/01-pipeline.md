# 1. Pipeline

[Up: table of contents](TOC-architecture.md)

`core::process_file` runs the stages of the [spec](../specs/02-pipeline-overview.md) in order. `app::batch::run` fans this over every image in the input folder and its subfolders one level down,
with `rayon`; on success the source file is moved to the backup folder (`PPRONI-BACKUP`),
on failure it is left in place.

```
load ──▶ preprocess ──▶ classify ──▶ [headcut] ──▶ framing ──▶ dispatch ──▶ route ──▶ export
```

| Stage | Module | In → Out |
|---|---|---|
| **load** | `core::preprocessor::load` | path → `Decoded` (image + ICC profile, EXIF applied) |
| **preprocess** | `core::preprocessor::preprocess` | `Decoded` → `Prepared` (sRGB, alpha flattened, + true-colour working copy + the optimiser's crop); `Err` for an image with nothing but backdrop |
| **classify** | `core::classify` (private) | `Prepared` → `Option<(ShotClassification, Mask)>` |
| *headcut* | `core::preprocessor::maria_antonieta` (called by `process_file`, only when `Options::enable_headcuts` is `true`) | `Prepared` + verdict → `Prepared` cut above the face + edges judged again |
| **framing** | `core::framing::decide` (in `process_file`) | trims + mask → `Keep`, `Centre` or `Peel`; `Peel` crops the picture, `Centre` sets `Prepared::framing` |
| **dispatch** | `core::shot_classifier::route::decide` | `Prepared` + verdict → `Decision` (the route) |
| **route** | `core::shot_classifier::route::send` | `Prepared` + verdict + route → `RgbImage` |
| **export** | `core::exporter::export` | `RgbImage` (+ optional `ShotInputs`) → size envelope → JPEG on disk, source moved to `PPRONI-BACKUP` |

`classify` carries the segmentation `Mask` out with the verdict; the exporter needs it for the
[debug tags](02-debug-tags.md).
