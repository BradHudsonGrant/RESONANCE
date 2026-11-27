"""Checks an exported Beat This! ONNX file against the PyTorch checkpoint it came from.

    python tools/models/verify_beat_this.py --checkpoint final0 --onnx target/models/beat_this.onnx

Both run on the same fixed log-mel tensors, a full 1,500-frame chunk and a short one, and every
logit must agree within 1e-4. It also prints the log-mel of a fixed test tone, which the Rust
front end's parity test in `crates/analysis/src/beats.rs` checks against.
"""

import argparse
import math
import pathlib

import numpy as np
import onnxruntime
import torch

from beat_this.inference import load_model
from beat_this.preprocessing import LogMelSpect

TOLERANCE = 1e-4


def test_tone():
    """Two seconds at 22,050 Hz: 220 Hz at half scale plus 3 kHz at a quarter."""
    n = np.arange(44_100, dtype=np.float64)
    tone = 0.5 * np.sin(2 * math.pi * 220 * n / 22_050) + 0.25 * np.sin(2 * math.pi * 3_000 * n / 22_050)
    return torch.tensor(tone, dtype=torch.float32)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--checkpoint", default="final0")
    parser.add_argument("--onnx", type=pathlib.Path, required=True)
    args = parser.parse_args()

    model = load_model(args.checkpoint, "cpu")
    session = onnxruntime.InferenceSession(str(args.onnx), providers=["CPUExecutionProvider"])
    generator = torch.Generator().manual_seed(0)
    worst = 0.0
    for frames in (1500, 437):
        spect = torch.rand(1, frames, 128, generator=generator) * 4
        with torch.inference_mode():
            expected = model(spect)
        beat, downbeat = session.run(None, {"spect": spect.numpy()})
        for name, found in (("beat", beat), ("downbeat", downbeat)):
            diff = float(np.max(np.abs(found - expected[name].numpy())))
            worst = max(worst, diff)
            print(f"{frames} frames, {name}: max abs diff {diff:.2e}")
    print(f"worst {worst:.2e}, {'pass' if worst < TOLERANCE else 'FAIL'}")

    mel = LogMelSpect()(test_tone()).numpy()
    print(f"test tone: {mel.shape[0]} frames")
    for frame in (10, 50):
        values = ", ".join(f"({frame}, {b}, {mel[frame, b]:.6f})" for b in range(0, 128, 16))
        print(values)
    if worst >= TOLERANCE:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
