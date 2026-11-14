# Native shell

## 13 November 2026, installed app

Machine: ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro.

The 3.35 MiB Windows installer installed the app at `%LOCALAPPDATA%\Resonance\resonance-app.exe`.

- The first launch showed an empty library.
- The scan form read `D:\Music\RESONANCE\fixtures-local` and reported `50 files: 50 added, 0 changed, 0 unchanged`. The table showed 50 tracks.
- Closing and reopening the installed app kept all 50 tracks.

Uninstall behaviour was not tested.

## Startup observation

The installed process was closed, then the registered app `com.resonance.app` was launched through UI automation. The accessibility tree was polled until the window showed both the 50-track count and the track grid. One timer covered the launch call through the first matching observation, within the same automation call.

That interval was **714 ms**, including automation overhead. This was one observation with a new process, a warm filesystem cache and the 50 fixture tracks.

The [cold-start budget](../architecture.md#performance-budgets) remains open. A disk-cold run and a 100,000-track library still need to be measured.

## 13 November 2026, v0.1.0 release installer

`Resonance_0.1.0_x64-setup.exe` (3.76 MiB, `sha256:83f5ccbf9a4e48ecbc6f78052a52f539e152dd842e97856b9761597b6d0a9007`) was downloaded from the published [`v0.1.0` pre-release](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.1.0) into `target/release-check` and installed in current-user mode at `%LOCALAPPDATA%\Resonance\resonance-app.exe` (13.44 MiB).

- Launched against a fresh data folder, the window showed the empty library state (`No tracks yet`).
- Scanning `D:\Music\RESONANCE\fixtures-local` through the scan form filled the track table with all 50 tracks (`50 tracks`, `aria-rowcount=51`).
- Closing and reopening the installed app kept all 50 tracks in the table.
