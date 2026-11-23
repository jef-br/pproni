"""
Export ZhengPeng7/BiRefNet_lite to ONNX at a fixed square side.

    python "tools/2. export_onnx.py" [SIDE]        # default 512

Writes `models/birefnet_lite_{SIDE}.onnx`. Run it from the repo root so the
file lands in the one folder `build.rs` looks in.

Side notes, measured on an i7-12700H:
  256   too coarse - returns an empty mask on pale, soft-edged garments.
  384   ~2.4x faster than 512, mask IoU 0.9814 against it (worst 0.9501).
  448   ~1.8x faster than 512, mask IoU 0.9901 (worst 0.9830).
  512   the reference.

Opset 20 matters: it exports the decoder's deformable convolutions as a single
native `DeformConv` op. An older opset makes torch decompose them into a
GatherND graph of ~16k nodes that is far slower and blows up memory. `DeformConv`
needs ONNX Runtime >= 1.22 at inference, and DirectML cannot run it: the exe embeds a copy
with `DeformConv` rewritten as GridSample + Conv (see models/README.md). The embedded
onnxruntime-directml.dll is 1.24.

The export traces eval mode, so BiRefNet's training-only heads
(`conv_ms_spvn_*`, `gdt_convs_pred_*`) are absent from the graph by design.

Requires: torch, transformers, onnx
"""
import os
import sys

import onnx
import torch
from transformers import AutoModelForImageSegmentation

MODEL_ID = "ZhengPeng7/BiRefNet_lite"
# One folder holds every export; build.rs reads MODEL_FILE out of it.
MODEL_DIR = "models"


def main():
    side = int(sys.argv[1]) if len(sys.argv) > 1 else 512
    os.makedirs(MODEL_DIR, exist_ok=True)
    out = os.path.join(MODEL_DIR, f"birefnet_lite_{side}.onnx")

    print(f"loading {MODEL_ID} ...", flush=True)
    model = AutoModelForImageSegmentation.from_pretrained(
        MODEL_ID, trust_remote_code=True).float().eval()

    dummy = torch.randn(1, 3, side, side, dtype=torch.float32)
    print(f"exporting {out} at {side}x{side} ...", flush=True)
    with torch.no_grad():
        torch.onnx.export(
            model, (dummy,), out,
            input_names=["input_image"], output_names=["output_image"],
            opset_version=20,
        )

    # torch may write weights to a sidecar .onnx.data. build.rs embeds a single
    # blob, so fold them back in or the exe silently ships a headless graph.
    onnx.save_model(onnx.load(out), out, save_as_external_data=False)
    print("done:", out)


if __name__ == "__main__":
    main()
