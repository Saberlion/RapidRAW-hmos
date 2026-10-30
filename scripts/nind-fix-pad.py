"""NIND denoise ONNX graph surgery: negative-pads Pad -> equivalent Slice.

The upstream `nind_denoise_utnet_684.onnx` contains one Pad node with negative
pads (crop semantics, pads=[0,0,-2,-2,0,0,-2,-2], constant_value=0). MindSpore
Lite's PadFusion does not support negative pads: conversion succeeds but the
model dies at runtime with an InferShape failure on that node (verified against
the 2.7.0 Kit engine; converter flags --optimize=none / --inputShape /
--inputDataFormat do not help). The crop is exactly Slice-equivalent when
constant_value == 0, and the replacement is byte-identical under ONNX Runtime
(verified against the original model).

Usage: python nind-fix-pad.py <src.onnx> <dst.onnx>

Requires: python with onnx + numpy (output verified deterministic with
onnx 1.23.2; scripts/convert-nind-ms.ps1 gates the final hash, so any
serialization drift fails loudly instead of shipping a divergent model).
"""
import sys

import numpy as np
import onnx
from onnx import helper, numpy_helper

SRC, DST = sys.argv[1], sys.argv[2]

m = onnx.load(SRC)
g = m.graph
inits = {i.name: i for i in g.initializer}

pad_nodes = [n for n in g.node if n.op_type == "Pad"]
assert len(pad_nodes) == 2, f"expected 2 Pad nodes, got {len(pad_nodes)}"

target = None
pads = None
for n in pad_nodes:
    arr = onnx.numpy_helper.to_array(inits[n.input[1]])
    if (arr < 0).any():
        assert target is None, "multiple negative-pads Pad nodes"
        target, pads = n, arr

assert target is not None and pads is not None, "no negative-pads Pad found"
cv = onnx.numpy_helper.to_array(inits[target.input[2]])
assert float(cv) == 0.0, f"constant value {cv} != 0 -- NOT slice-equivalent"

print("target Pad:", target.name, "mode=constant pads=", pads.tolist())
# ONNX pads layout (NCHW): [n_b, c_b, h_b, w_b, n_e, c_e, h_e, w_e]
assert pads[0] == 0 and pads[1] == 0 and pads[4] == 0 and pads[5] == 0
h_b, w_b, h_e, w_e = int(pads[2]), int(pads[3]), int(pads[6]), int(pads[7])
# negative begin/end pads = crop from start/end (must be all-negative to be slice-equivalent)
assert h_b <= 0 and w_b <= 0 and h_e <= 0 and w_e <= 0, (
    "mixed positive/negative pads -- NOT slice-equivalent"
)
start_h, start_w = -h_b, -w_b  # crop-from-begin -> slice start offset
end_h, end_w = h_e, w_e        # crop-from-end   -> negative slice end (count from end)
print(f"crop equivalence: Slice axes=[2,3] starts=[{start_h},{start_w}] ends=[{end_h},{end_w}]")

starts = numpy_helper.from_array(np.array([start_h, start_w], dtype=np.int64), "slice_starts")
ends = numpy_helper.from_array(np.array([end_h, end_w], dtype=np.int64), "slice_ends")
axes = numpy_helper.from_array(np.array([2, 3], dtype=np.int64), "slice_axes")

slice_node = helper.make_node(
    "Slice",
    inputs=[target.input[0], "slice_starts", "slice_ends", "slice_axes"],
    outputs=list(target.output),
    name=(target.name or "node_pad_1") + "_slice",
)

new_nodes = [slice_node if n is target else n for n in g.node]
del g.node[:]
g.node.extend(new_nodes)
g.initializer.extend([starts, ends, axes])

onnx.checker.check_model(m)
onnx.save(m, DST)
print("saved:", DST)
