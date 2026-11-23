# models

Every BiRefNet `.onnx` export lives here and nowhere else. The files themselves
are gitignored (`*.onnx`) — they are build inputs, not source — so this README is
the only committed file, and it is what makes the folder exist in a fresh clone.

`build.rs` reads exactly one BiRefNet file from here:

```rust
const MODEL_DIR:  &str = "models";
const MODEL_FILE: &str = "birefnet_512_nodeform_op19.onnx";
```

`MODEL_FILE` is a bare filename resolved inside this folder. It is the single
switch that decides which export gets encrypted and compiled into the exe — so
dropping a new variant in here changes nothing until that line names it.

## The two 512 files

Same BiRefNet_lite weights, same maths. They differ in one thing:

| File | `DeformConv` | Runs on |
|---|---|---|
| `birefnet_lite_512.onnx` | the stock export: 20 native `DeformConv` ops (needs ONNX Runtime 1.22 or newer) | CPU, CUDA |
| `birefnet_512_nodeform_op19.onnx` | those 20 ops rewritten as `GridSample` + `Conv` (opset 19) | graphics card (DirectML) and CPU |

The second is the one the exe embeds. Details: `docs/architecture/modules/core-shot-classifier-birefnet.md`.
The `examples/` read it by default too.

A missing `MODEL_FILE` fails the **build**, naming the path. That is deliberate:
the exe ships as one file with the weights inside it, so a model that isn't
there must be caught at build time, never at run time.

Add as many variants as you like while experimenting — `tools/2. export_onnx.py`
writes new exports straight into this folder, and `tools/3. quantize_int8.py`
writes its output beside its input. See `tools/README.md` for the pipeline.
