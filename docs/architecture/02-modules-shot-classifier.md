# 2. Modules: `core::shot_classifier`, the classifier

[Up: table of contents](TOC.md)

One module per job. `SegmentationModel` keeps the gate/refine logic independent of the ONNX
binding, so it can also be exercised against a stand-in model in tests — real BiRefNet inference is
always built into the exe (no feature flag).

| Module | Surface | Job |
|---|---|---|
| `shot_classifier` | `ShotClassification`, `classify_instance` | runs both passes for one instance |
| `gate` | `EdgeProximity`, `gate` | pass 1: cheap border-band scan, per edge |
| `refine` | `RefinementInput`, `RefineParams`, `EdgeRefinement`, `refine_edge` | pass 2: trimap + colour matting at full resolution |
| `geometry` | `Edge`, `Grid`, `EdgeView`, `Rect` | edge-relative coordinates |
| `segmentation` | `Mask`, `Instance`, `SegmentationModel` | model-agnostic types |
| `sampling` | `ColorStats`, `sample_background_color` | per-edge background colour |
| `shotcode` | `eix_bits`, `mask_too_small`, `NO_VERDICT_EIX` | the `EIX` debug tag bits |
| `birefnet` | `BiRefNetConfig`, `BiRefNetModel` | ONNX inference, always built in |
| `cipher` | ChaCha20 keystream | shared by `build.rs` (encrypt) and `birefnet::load` (decrypt) for the embedded model |

`EdgeView` is the load-bearing idea: every per-edge algorithm is written once in "top edge, depth
increasing downward" terms against the `Grid` trait, and the other three edges are presented by
coordinate transform. Nothing edge-specific lives outside `geometry`.
