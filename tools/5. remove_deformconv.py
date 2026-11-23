"""
Rewrite a stock BiRefNet export so a graphics card (DirectML) can run it.

    python "tools/5. remove_deformconv.py" [INPUT] [OUTPUT]

INPUT  defaults to models/birefnet_lite_512.onnx, the stock export of `tools/2. export_onnx.py`.
OUTPUT defaults to INPUT's name without "_lite" plus "_nodeform_op19", in the same folder:
       models/birefnet_lite_512.onnx -> models/birefnet_512_nodeform_op19.onnx, the file
       `build.rs` embeds. An existing OUTPUT is overwritten. Run from the repo root.

Two rewrites, same weights, same maths (masks match the stock model to float rounding):

1. Every `DeformConv` (DirectML has no kernel for it; the stock export has 20) becomes
   `GridSample` + `Conv`. For each of the K = kH*kW taps the sampling grid is
   (output position * stride - pad + tap * dilation + the learned offset); GridSample reads the
   input there bilinearly with zero padding and align_corners=1, which is what DeformConv does.
   The samples are scaled by the learned mask, laid out as C*K channels, and one 1x1 `Conv` with
   the original weight (reshaped to [oC, C*K, 1, 1]) and bias sums them. Only group=1 and
   offset_group=1 are handled; the model uses nothing else, and anything else is an error.
2. Every `Gelu` becomes x * (erf(x / sqrt 2) + 1) * 0.5. `Gelu` is the only operator in the graph
   that needs opset 20; with it gone the file declares opset 19.
3. The `allowzero=1` flag on the stock export's 221 Reshape nodes is removed (no shape has a 0 in
   it, so nothing changes). With the flag, DirectML does not start the model and the run falls
   back to the CPU without saying so.

The old per-node shape annotations are dropped, because the rewrite makes them stale. The input
and output (fixed side S, `input_image` / `output_image`) are unchanged.

To check a result, A/B it against the stock export:
    python "tools/4. compare_models.py" models/birefnet_lite_512.onnx models/birefnet_512_nodeform_op19.onnx

Requires: onnx, numpy
"""
import math
import os
import sys

import numpy as np
import onnx
from onnx import helper, numpy_helper, shape_inference

DEFAULT_INPUT = os.path.join("models", "birefnet_lite_512.onnx")
OPSET = 19


def default_output(path):
    folder, name = os.path.split(path)
    stem = os.path.splitext(name)[0].replace("_lite", "")
    return os.path.join(folder, f"{stem}_nodeform_op19.onnx")


def attrs(node):
    return {a.name: helper.get_attribute_value(a) for a in node.attribute}


def dims_of(value_info):
    return [d.dim_value for d in value_info.type.tensor_type.shape.dim]


def const(inits, name, array):
    inits.append(numpy_helper.from_array(np.asarray(array), name))
    return name


def rewrite_deform_conv(node, n, shapes, weights, inits):
    """The nodes that replace one DeformConv node. `n` numbers the block (1, 2, ...)."""
    a = attrs(node)
    if a.get("group", 1) != 1 or a.get("offset_group", 1) != 1:
        raise SystemExit(f"{node.name}: only group=1 and offset_group=1 are handled, got {a}")
    x, weight, offset = node.input[0], node.input[1], node.input[2]
    bias = node.input[3] if len(node.input) > 3 and node.input[3] else None
    mask = node.input[4] if len(node.input) > 4 and node.input[4] else None

    _, c, h, w = shapes[x]
    out_c, _, kh, kw = weights[weight].shape
    k = kh * kw
    sh, sw = a.get("strides", [1, 1])
    dh, dw = a.get("dilations", [1, 1])
    pad_top, pad_left = a.get("pads", [0, 0, 0, 0])[:2]
    oh, ow = shapes[offset][2], shapes[offset][3]

    p = f"dc{n}_"
    # Where each tap of each output pixel lands on the input, before the learned offset.
    tap_i, tap_j = np.divmod(np.arange(k), kw)
    base_y = tap_i[:, None, None] * dh - pad_top + np.arange(oh)[None, :, None] * sh
    base_x = tap_j[:, None, None] * dw - pad_left + np.arange(ow)[None, None, :] * sw
    base_y = np.broadcast_to(base_y, (k, oh, ow))[None].astype(np.float32)
    base_x = np.broadcast_to(base_x, (k, oh, ow))[None].astype(np.float32)

    i64 = np.int64
    s5 = const(inits, p + "s5", np.array([1, k, 2, oh, ow], i64))
    i0 = const(inits, p + "i0", np.array(0, i64))
    i1 = const(inits, p + "i1", np.array(1, i64))
    by = const(inits, p + "by", base_y)
    bx = const(inits, p + "bx", base_x)
    ch = const(inits, p + "ch", np.array(2.0 / (h - 1), np.float32))
    cw = const(inits, p + "cw", np.array(2.0 / (w - 1), np.float32))
    one = const(inits, p + "one", np.array(1.0, np.float32))
    sflat = const(inits, p + "sflat", np.array([1, k * oh, ow, 1], i64))
    s5b = const(inits, p + "s5b", np.array([1, c, k, oh, ow], i64))
    sm = const(inits, p + "sm", np.array([1, 1, k, oh, ow], i64))
    sc = const(inits, p + "sc", np.array([1, c * k, oh, ow], i64))
    sw_ = const(inits, p + "sw", np.array([out_c, c * k, 1, 1], i64))

    def node_(op, ins, out, **kw_):
        return helper.make_node(op, ins, [out], name=f"{p}{out}", **kw_)

    nodes = [
        # The offset holds (dy, dx) per tap: split it.
        node_("Reshape", [offset, s5], p + "off5"),
        node_("Gather", [p + "off5", i0], p + "dy", axis=2),
        node_("Gather", [p + "off5", i1], p + "dx", axis=2),
        node_("Add", [by, p + "dy"], p + "py"),
        node_("Add", [bx, p + "dx"], p + "px"),
        # Pixel position -> GridSample's -1..1 range (align_corners=1).
        node_("Mul", [p + "py", ch], p + "gy0"),
        node_("Sub", [p + "gy0", one], p + "gy"),
        node_("Mul", [p + "px", cw], p + "gx0"),
        node_("Sub", [p + "gx0", one], p + "gx"),
        node_("Reshape", [p + "gx", sflat], p + "gxf"),
        node_("Reshape", [p + "gy", sflat], p + "gyf"),
        node_("Concat", [p + "gxf", p + "gyf"], p + "grid", axis=3),
        node_("GridSample", [x, p + "grid"], p + "samp",
              mode="bilinear", padding_mode="zeros", align_corners=1),
        node_("Reshape", [p + "samp", s5b], p + "samp5"),
    ]
    taps = p + "samp5"
    if mask:
        nodes += [
            node_("Reshape", [mask, sm], p + "m5"),
            node_("Mul", [p + "samp5", p + "m5"], p + "sampm"),
        ]
        taps = p + "sampm"
    nodes += [
        node_("Reshape", [taps, sc], p + "cols"),
        node_("Reshape", [weight, sw_], p + "w2"),
        helper.make_node(
            "Conv", [p + "cols", p + "w2"] + ([bias] if bias else []), [node.output[0]],
            name=p + "conv", kernel_shape=[1, 1]),
    ]
    return nodes


def rewrite_gelu(node, n, inits):
    """x * (erf(x / sqrt 2) + 1) * 0.5, the exact Gelu."""
    p = f"gelu{n}_"
    x = node.input[0]
    f32 = np.float32
    r2 = const(inits, p + "r2", np.array(1.0 / math.sqrt(2.0), f32))
    one = const(inits, p + "one", np.array(1.0, f32))
    half = const(inits, p + "half", np.array(0.5, f32))

    def node_(op, ins, out):
        return helper.make_node(op, ins, [out], name=out)

    return [
        node_("Mul", [x, r2], p + "a"),
        node_("Erf", [p + "a"], p + "b"),
        node_("Add", [p + "b", one], p + "c"),
        node_("Mul", [x, p + "c"], p + "d"),
        helper.make_node("Mul", [p + "d", half], [node.output[0]], name=p + "e"),
    ]


def drop_allowzero(nodes, weights, original_nodes):
    """Remove `allowzero=1` from every Reshape whose shape is a constant with no 0 in it.

    The flag only matters when a shape holds a 0, so dropping it there changes nothing. The
    stock export sets it on 221 Reshapes (none has a 0), and DirectML does not start the model
    with it: the run silently falls back to the CPU, about 15 times slower. Returns the count.
    """
    consts = dict(weights)
    for node in original_nodes:
        if node.op_type == "Constant":
            for a in node.attribute:
                if a.name == "value":
                    consts[node.output[0]] = numpy_helper.to_array(a.t)
    dropped = 0
    for node in nodes:
        if node.op_type != "Reshape" or attrs(node).get("allowzero", 0) != 1:
            continue
        shape = consts.get(node.input[1])
        if shape is None or (shape == 0).any():
            continue
        kept = [a for a in node.attribute if a.name != "allowzero"]
        del node.attribute[:]
        node.attribute.extend(kept)
        dropped += 1
    return dropped


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_INPUT
    dst = sys.argv[2] if len(sys.argv) > 2 else default_output(src)

    model = onnx.load(src)
    for gelu in (n for n in model.graph.node if n.op_type == "Gelu"):
        if attrs(gelu).get("approximate", b"none") != b"none":
            raise SystemExit(f"{gelu.name}: only the exact Gelu is handled")

    # Tensor shapes (the model has a fixed side), from the model's own annotations.
    annotated = shape_inference.infer_shapes(model)
    shapes = {v.name: dims_of(v) for v in annotated.graph.value_info}
    weights = {i.name: numpy_helper.to_array(i) for i in model.graph.initializer}

    inits = []
    nodes = []
    n_deform = n_gelu = 0
    for node in model.graph.node:
        if node.op_type == "DeformConv":
            n_deform += 1
            nodes += rewrite_deform_conv(node, n_deform, shapes, weights, inits)
        elif node.op_type == "Gelu":
            nodes += rewrite_gelu(node, n_gelu, inits)
            n_gelu += 1
        else:
            nodes.append(node)
    if not n_deform and not n_gelu:
        raise SystemExit(f"{src}: no DeformConv and no Gelu in it, nothing to rewrite")

    n_allowzero = drop_allowzero(nodes, weights, model.graph.node)

    del model.graph.node[:]
    model.graph.node.extend(nodes)
    model.graph.initializer.extend(inits)
    del model.graph.value_info[:]
    for opset in model.opset_import:
        if opset.domain in ("", "ai.onnx"):
            opset.version = OPSET

    onnx.checker.check_model(model)
    os.makedirs(os.path.dirname(dst) or ".", exist_ok=True)
    onnx.save(model, dst)
    print(f"{src} -> {dst}: {n_deform} DeformConv and {n_gelu} Gelu rewritten, "
          f"{n_allowzero} Reshape flags dropped, opset {OPSET}")


if __name__ == "__main__":
    main()
