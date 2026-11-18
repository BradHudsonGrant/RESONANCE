# Build times

Clean and incremental builds of the Rust workspace in the dev profile. `just build-times` measures them in its own folder under `target/`, so it never throws away a working build. Each figure is the median of three runs, with the runs in brackets.

Machine: Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro, rustc 1.100.0. Real-time antivirus scanning is on and the build folder is inside the repo with no exclusion.

## 18 November 2026, M2 at v0.3.0

| Build | Time |
| --- | --- |
| Clean dev build | 131.1 s (138.8, 130.7, 131.1) |
| After a change in `core` | 4.6 s (4.6, 4.4, 4.6) |

The workspace is now eight crates plus the Tauri shell, with Tauri, `symphonia`, `cpal`, `rusty-chromaprint`, `rubato` and `notify` under it. A change in `core` rebuilds every crate that depends on it and relinks the binaries. The edit-and-reload loop for UI work needs no Rust build, so these figures only bear on core changes.

## 13 November 2026, M0 with three crates

| Build | Time |
| --- | --- |
| Clean dev build | 24.9 s (26.1, 24.9, 24.3) |
| After a change in `core` | 1.2 s (1.2, 1.2, 1.2) |

The workspace is `core`, `api` and `cli`. A change in `core` rebuilds all three and links the binary, so it is the slowest incremental case.

### rust-lld and sccache

Both were tried on the same day and neither was kept.

| Configuration | Clean dev build | After a change in `core` |
| --- | --- | --- |
| Default linker (`link.exe`) | 24.9 s | 1.2 s |
| `rust-lld` as the linker | 23.8 s | 1.1 s |
| `sccache` 0.19.0, empty cache | 27.1 s | 1.3 s |
| `sccache` 0.19.0, filled cache | 14.9 s (14.7, 15.1) | 1.3 s |

`rust-lld` saves one second on a clean build, which is inside the spread between runs of the default (24.3 to 26.1 s). To separate the incremental figures, each linker ran twelve incremental builds, twice. The medians were 1,212 and 1,218 ms with `link.exe`, and 1,240 and 1,219 ms with `rust-lld`. There is no difference at this size, so the default linker stays.

`sccache` takes 10 s off a clean build once its cache is filled. The build that happens all day is the incremental one, and that is no faster, because the workspace's own crates build incrementally and `sccache` does not cache those. A clean build with an empty cache is 2 s slower. It is also one more tool to install on every machine and in CI.

Measure both again when the Tauri shell joins the workspace ([#20](https://github.com/BradHudsonGrant/RESONANCE/issues/20)). Its dependency tree is several times this size, and link time grows with it.
