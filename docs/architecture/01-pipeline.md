# 1. Pipeline

[Up: table of contents](TOC.md)

`core::process_file` runs five stages in order, with a framing step (`core::framing`) between classify and dispatch. `app::batch::run` fans this over every image in the
input folder with `rayon`; on success the source file is moved to the backup folder (`PPRONI-BACKUP`),
on failure it is left in place.

```
load ──▶ preprocess ──▶ classify ──▶ dispatch ──▶ export
```

| Stage | Module | In → Out |
|---|---|---|
| **load** | `core::preprocessor::load` | path → `Decoded` (image + ICC profile, EXIF applied) |
| **preprocess** | `core::preprocessor::preprocess` | `Decoded` → `Prepared` (sRGB, alpha flattened, + true-colour working copy + the optimiser's crop); `Err` for an image with nothing but backdrop |
| **classify** | `core::classify` (private) | `Prepared` → `Option<(ShotClassification, Mask)>` |
| *framing* | `core::framing::decide` (in `process_file`) | trims + mask → `Keep`, `Centre`, `Crop` or `Peel`; `Crop` and `Peel` crop the picture, `Centre` sets `Prepared::framing` |
| **dispatch** | `core::processor::routes::dispatch` | `Prepared` + verdict → `RgbImage` |
| **export** | `core::exporter::export` | `RgbImage` (+ optional `ShotInputs`) → size envelope → JPEG on disk, source moved to `PPRONI-BACKUP` |

`classify` carries the segmentation `Mask` out with the verdict; the exporter needs it for the
[debug tags](02-debug-tags.md).
