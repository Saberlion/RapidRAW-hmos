#!/usr/bin/env python3
"""Real-input usability eval: build inputs from a real photo, run ORT reference.

App-side preprocessing contracts (from src-tauri/src/face_masking.rs / ai_denoise):
  face_landmarks : [1,256,256,3] f32, NHWC, v/255.0
  selfie         : [1,256,256,3] f32, NHWC, v/127.5-1.0
  nind           : [1,3,504,504] f32, NCHW, v/255.0
"""
import os
import numpy as np
from PIL import Image

ROOT = '/mnt/c/workspace/LightRaw/npu-models'
EVAL = os.path.join(ROOT, 'eval')
os.makedirs(EVAL, exist_ok=True)
PHOTO = '/mnt/c/Windows/Web/Wallpaper/Windows/img0.jpg'

img = Image.open(PHOTO).convert('RGB')
print('photo:', img.size)

# --- inputs ---
face256 = np.asarray(img.resize((256, 256), Image.BILINEAR), dtype=np.float32)
face_in = (face256 / 255.0).astype(np.float32)[None, ...]          # NHWC
selfie_in = (face256 / 127.5 - 1.0).astype(np.float32)[None, ...]  # NHWC
nind504 = np.asarray(img.resize((504, 504), Image.BILINEAR), dtype=np.float32)
nind_in = (nind504 / 255.0).astype(np.float32).transpose(2, 0, 1)[None, ...]  # NCHW

face_in.tofile(os.path.join(EVAL, 'face_input.bin'))
selfie_in.tofile(os.path.join(EVAL, 'selfie_input.bin'))
nind_in.tofile(os.path.join(EVAL, 'nind_input.bin'))
print('inputs:', face_in.shape, selfie_in.shape, nind_in.shape)

# --- ORT reference ---
import onnxruntime as ort

def run(path, feeds, names=None):
    s = ort.InferenceSession(path, providers=['CPUExecutionProvider'])
    out = s.run(names, feeds)
    return [np.asarray(o) for o in out]

face_out = run(os.path.join(ROOT, 'onnx', 'mediapipe_face_landmarks.onnx'),
               {'input_12': face_in})
for i, o in enumerate(face_out):
    o.tofile(os.path.join(EVAL, f'ref_face_{i}.bin'))
    print(f'ref_face_{i}:', o.shape, float(o.min()), float(o.max()))

selfie_orig = run(os.path.join(ROOT, 'onnx', 'mediapipe_selfie_multiclass.onnx'),
                  {'input_29': selfie_in})
selfie_orig[0].tofile(os.path.join(EVAL, 'ref_selfie_orig.bin'))
print('ref_selfie_orig:', selfie_orig[0].shape, float(selfie_orig[0].min()), float(selfie_orig[0].max()))

selfie_surg = run(os.path.join(ROOT, 'work', 'selfie_surgery.onnx'),
                  {'input_29': selfie_in})
selfie_surg[0].tofile(os.path.join(EVAL, 'ref_selfie_surg.bin'))
print('ref_selfie_surg:', selfie_surg[0].shape, float(selfie_surg[0].min()), float(selfie_surg[0].max()))

nind_out = run(os.path.join(ROOT, 'onnx', 'nind_denoise_utnet_684.onnx'),
               {'input': nind_in})
nind_out[0].tofile(os.path.join(EVAL, 'ref_nind.bin'))
print('ref_nind:', nind_out[0].shape, float(nind_out[0].min()), float(nind_out[0].max()))

print('EVAL-PREP-DONE')
