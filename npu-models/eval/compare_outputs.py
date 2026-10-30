#!/usr/bin/env python3
"""Compare on-device .om outputs against ORT CPU reference (same real input).

Metrics: max_abs / mean_abs (and PSNR for NIND, argmax agreement for selfie).
"""
import os
import numpy as np

EVAL = os.path.dirname(os.path.abspath(__file__))

def load(name, shape, dtype=np.float32):
    return np.fromfile(os.path.join(EVAL, name), dtype=dtype).reshape(shape)

def stats(a, b, label):
    d = np.abs(a.astype(np.float64) - b.astype(np.float64))
    print(f"  {label:34s} max_abs={d.max():.6g}  mean_abs={d.mean():.6g}")
    return d.max(), d.mean()

print('== face_landmarks (input: real photo, /255 NHWC) ==')
ref = load('ref_face_0.bin', (1, 1, 1, 1434))
for dev in ('x90', 'xe90'):
    out = load(f'{dev}face_out0.bin', (1, 1, 1, 1434))
    mx, mn = stats(out, ref, f'{dev} landmarks (px, 0..256)')
    print(f"    -> normalized max {mx/256*100:.3f}% of frame, mean {mn/256*100:.3f}%")
ref_s = load('ref_face_1.bin', (1,)); 
for dev in ('x90', 'xe90'):
    out = load(f'{dev}face_out1.bin', (1,))
    stats(out, ref_s, f'{dev} face score')
ref_p = load('ref_face_2.bin', (1,))
for dev in ('x90', 'xe90'):
    out = load(f'{dev}face_out2.bin', (1,))
    stats(out, ref_p, f'{dev} presence')

print('== selfie_multiclass (logits [1,256,256,6]) ==')
ref_orig = load('ref_selfie_orig.bin', (1, 256, 256, 6))
ref_surg = load('ref_selfie_surg.bin', (1, 256, 256, 6))
print('  (surgery-only error on CPU, for reference:)')
stats(ref_surg, ref_orig, 'surgery vs original (ORT)')
for dev in ('x90', 'xe90'):
    out = load(f'{dev}selfie_out0.bin', (1, 256, 256, 6))
    stats(out, ref_orig, f'{dev} .om vs ORT original')
    stats(out, ref_surg, f'{dev} .om vs ORT surgered')
    am_dev = out.argmax(axis=3); am_ref = ref_orig.argmax(axis=3)
    agree = (am_dev == am_ref).mean()
    # ignore background-dominated agreement: also report per-class
    print(f'    -> argmax class agreement {agree*100:.3f}%')
    for c in range(6):
        sel = am_ref == c
        if sel.sum() > 0:
            print(f'       class {c}: ref px {int(sel.sum()):6d}  agree {(am_dev[sel]==c).mean()*100:6.2f}%')

print('== nind denoise ([1,3,504,504], 0..1 range) ==')
ref = load('ref_nind.bin', (1, 3, 504, 504))
for dev in ('x90', 'xe90'):
    out = load(f'{dev}nind_out0.bin', (1, 3, 504, 504))
    mx, mn = stats(out, ref, f'{dev} .om vs ORT')
    mse = float(((out.astype(np.float64) - ref.astype(np.float64)) ** 2).mean())
    psnr = 10 * np.log10(1.0 / mse) if mse > 0 else float('inf')
    print(f'    -> PSNR {psnr:.2f} dB (vs 8-bit-visible 48dB / 12-bit 72dB)')

print('COMPARE-DONE')
