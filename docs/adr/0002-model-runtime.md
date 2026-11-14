# 0002: Model runtime

Date: 12 November 2026. Status: accepted as the default.

## Context

Analysis needs neural models: beats and downbeats first, then stems and embeddings. The app has to install and run without Python. The development laptop has an RTX 5090 Laptop GPU with 24 GB of VRAM, and most users will have less or none.

## Decision

- Models run in-process on ONNX Runtime through the `ort` crate.
- `ort` is pinned to an exact version. It is 2.0.0-rc.15 today, still a release candidate, wrapping ONNX Runtime 1.31. It needs Rust 1.92 and the laptop has 1.93.1.
- Execution providers: CUDA or TensorRT on NVIDIA, CPU everywhere as the fallback, CoreML when macOS arrives. Check whether DirectML is still supported before using it for other Windows GPUs.
- STFT and mel front ends are written in Rust where a model needs one, so the exported graph stays simple.
- Python lives only in `tools/models/`: an export script and a parity test that compares ONNX output with the reference implementation on fixed inputs.
- Every model goes through the registry: id, version, source URL, checksum, licence, whether commercial use is allowed, and the input spec. Weights download on first use into the app data folder and are never in git.

## Why

ONNX Runtime is the one runtime that covers CPU, CUDA, TensorRT, CoreML and DirectML from the same model file. The models the roadmap starts with already have ONNX exports: Beat This! and HTDemucs.

In-process calls keep tensors on the GPU between steps and avoid a second process to install, start and watch.

## What follows from it

- On the RTX card, go for the fastest path that stays correct: TensorRT with cached engines, half precision where a parity test shows no loss, I/O binding, batched windows. Benchmark each against plain CUDA and keep what measurably wins.
- The laptop has the NVIDIA driver (621.14) and no CUDA toolkit or cuDNN. The CUDA and TensorRT providers need those libraries beside the app, and they are large downloads. M5 starts by working out the download size before fetching anything.
- The old app ran Beat This! through the `beat-this` crate on the pure-Rust `rten` runtime, with no system dependencies. Its database holds per-beat grids for 431 tracks made that way, so that path works on this machine. Keep it in mind as the CPU path for beat tracking if ONNX Runtime's CPU provider turns out slower or heavier to ship. Measure both in M5.
- One heavy GPU job runs at a time. It yields to anything the user is waiting on and backs off while Mix is live.
- Accuracy is reported with numbers, on the real library and on public datasets, with the hardware recorded.

## What would change it

If a model a milestone depends on can't be exported after a two-day spike, it runs in an optional Python worker behind the same analyser trait. That needs its own ADR, and the default install stays free of Python.

If `ort` 2.0 has not reached a stable release by M5, or an update breaks the pinned API, weigh `rten` for the models it can run.
