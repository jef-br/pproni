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
| `segmentation` | `Mask`, `Instance`, `SegmentationModel`, `CroppedModel` | model-agnostic types; `CroppedModel` runs the model on the optimiser's crop and pastes the mask back |
| `sampling` | `ColorStats`, `sample_background_color` | per-edge background colour |
| `shotcode` | `eix_bits`, `mask_too_small`, `NO_VERDICT_EIX` | the `EIX` debug tag bits |
| `plan` | `Pins`, `SquarePlan` (`new`, `real_region`, `overhang`), `Overhang` | R1 step 1: where the square goes, per axis; a blocked side never moves. Pure geometry |
| `route` | `Shot`, `Route`, `R1Job`, `Decision`, `decide`, `send`, `plan_square` | the classifier's last step: picks R1/R2/R3 and its plan, then runs exactly that route |
| `birefnet` | `BiRefNetConfig`, `BiRefNetModel` | ONNX inference, always built in |
| `gpu_adapters` (private) | `Adapter`, `ranked` | the machine's graphics cards, most dedicated video memory first, software adapter left out; `birefnet` picks its device from this list |
| `cipher` | ChaCha20 keystream | shared by `build.rs` (encrypt) and `birefnet::load` (decrypt) for the embedded model |

`EdgeView` is the load-bearing idea: every per-edge algorithm is written once in "top edge, depth
increasing downward" terms against the `Grid` trait, and the other three edges are presented by
coordinate transform. Nothing edge-specific lives outside `geometry`.
