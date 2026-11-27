"""Exports a Beat This! checkpoint to the ONNX file the app downloads.

Beat This! (Foscarin, Schlueter and Widmer, ISMIR 2024, https://github.com/CPJKU/beat_this)
publishes its code and weights under the MIT licence. The app never runs Python: it computes the
log-mel spectrogram itself and runs this file through ONNX Runtime. Only whoever publishes a new
model version needs this script.

    python -m pip install "git+https://github.com/CPJKU/beat_this@<commit>" onnx onnxscript
    python tools/models/export_beat_this.py --checkpoint final0 --out target/models/beat_this.onnx

The model reads `spect`, float32 `(1, frames, 128)`: log-mel frames at 50 a second, and writes
`beat` and `downbeat`, float32 `(1, frames)` logits. Run `verify_beat_this.py` on the result
before publishing it, then put its size and SHA-256 in `crates/analysis/src/beats.rs`.
"""

import argparse
import hashlib
import pathlib

import torch

from beat_this.inference import load_model

# 30 s at 50 frames a second, the length the model was trained on and the app feeds it.
CHUNK_FRAMES = 1500
MEL_BINS = 128


class Logits(torch.nn.Module):
    """Returns the two heads as a tuple, since ONNX outputs cannot be a dict."""

    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, spect):
        out = self.model(spect)
        return out["beat"], out["downbeat"]


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--checkpoint", default="final0")
    parser.add_argument("--out", type=pathlib.Path, required=True)
    args = parser.parse_args()

    torch.manual_seed(0)
    model = Logits(load_model(args.checkpoint, "cpu")).eval()
    example = torch.randn(1, CHUNK_FRAMES, MEL_BINS)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    # The TorchScript exporter: the dynamo one cannot yet decompose the rotary embeddings.
    torch.onnx.export(
        model,
        (example,),
        str(args.out),
        input_names=["spect"],
        output_names=["beat", "downbeat"],
        dynamic_axes={"spect": {1: "frames"}, "beat": {1: "frames"}, "downbeat": {1: "frames"}},
        dynamo=False,
        opset_version=17,
    )
    data = args.out.read_bytes()
    print(f"{args.out}: {len(data)} bytes, sha256 {hashlib.sha256(data).hexdigest()}")


if __name__ == "__main__":
    main()
