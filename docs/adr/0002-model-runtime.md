# 0002: Model runtime

Date: 12 November 2026. Status: accepted, settled for the first analysis work on 26 November 2026.

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

## Settled on 26 November 2026

- `ort` `=2.0.0-rc.15` (MIT OR Apache-2.0, released 19 October 2026) is still the newest release, so it is pinned with `=`. It wraps ONNX Runtime 1.31.
- Its build script fetches a prebuilt ONNX Runtime and checks its SHA-256. `.cargo/config.toml` points `ORT_CACHE_DIR` at `target/ort-cache` so the download stays in the project. It is 89 MB on x86-64 Windows, with the CUDA and TensorRT providers, and 11 MB on x86-64 Linux. ONNX Runtime links statically; the GPU providers are DLLs that land beside the binary.
- The fetch runs over rustls, which brings `webpki-roots` (CDLA-Permissive-2.0). Only the build script reaches it, so `deny.toml` allows that licence for that one crate. The app keeps the OS certificate store.
- A session tries TensorRT, with its engine and timing caches in `<data folder>/models/trt-cache`, then CUDA, then the CPU, and keeps the first that opens the model. Only x86-64 Windows builds the GPU providers for now. Other targets run on the CPU.
- The laptop has no CUDA 13, cuDNN 9 or TensorRT libraries yet, so both GPU providers fail to load and sessions run on the CPU. Those libraries are several GB and are fetched when the first model that needs the GPU lands.
- One job holds the GPU at a time, through `JobExecution::gpu_permit` in `crates/jobs`.
- Weights go to `<data folder>/models/<id>/<version>/model.onnx`. A download is written under a temporary name and only takes the real one once its size and SHA-256 match the registry. A model whose licence rules out commercial use is refused unless non-commercial models are turned on. That switch goes into Settings with the first such model.
- The app does not link `crates/analysis` yet. When it does, the installer has to carry the provider DLLs and ONNX Runtime's notices.

## Beat This! on 27 November 2026

- The beat tracker is `Beat This!` `final0` (MIT code and weights, `CPJKU/beat_this`). `tools/models/export_beat_this.py` exports it with opset 17 and a variable frame count. It uses the TorchScript exporter because the dynamo one cannot yet decompose the rotary embeddings.
- `tools/models/verify_beat_this.py` runs the checkpoint and the ONNX file on the same fixed log-mel tensors, a full 1,500-frame chunk and a 437-frame one. The worst logit difference is 5.7e-6, against a limit of 1e-4. The Rust log-mel front end matches torchaudio within 1e-3 on the script's test tone.
- The authors publish only the PyTorch checkpoint, so the ONNX file is a release asset on this repository under its own tag, `models-beat-this-final0`, kept apart from app releases. It is 83,162,655 bytes, and the registry checks its size and SHA-256 before keeping it.
- `the_model_finds_the_grid_of_a_made_up_track` in `crates/analysis/tests/beats.rs` runs the whole tracker on that file when `RESONANCE_BEAT_THIS_ONNX` names a copy. CI does not download it.

## What would change it

If a model a milestone depends on can't be exported after a two-day spike, it runs in an optional Python worker behind the same analyser trait. That needs its own ADR, and the default install stays free of Python.

If `ort` 2.0 has not reached a stable release by M5, or an update breaks the pinned API, weigh `rten` for the models it can run.
