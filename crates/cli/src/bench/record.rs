//! The record `resonance bench` writes: every budget in `docs/architecture.md` with its result,
//! then the figures behind each one.

use std::fmt;

use super::audio::AudioRun;
use super::machine::{Machine, Versions};
use super::page::{FrameTimes, Memory, PageResults};

const COLD_START_MS: f64 = 1_500.0;
const FRAME_MS: f64 = 16.0;
const MIX_P99_MS: f64 = 8.0;
const UI_THREAD_MS: f64 = 16.0;
const AUDIO_BUFFER_FRAMES: u32 = 256;
const AUDIO_SECONDS: f64 = 3_600.0;
const IDLE_MEMORY_BYTES: u64 = 400_000_000;
const WAVE_MS: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Met,
    Missed,
    /// Nothing failed, but the run was shorter than the budget asks for.
    NotYetShown,
    NotMeasurable,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Met => "Met",
            Self::Missed => "Missed",
            Self::NotYetShown => "Not yet shown",
            Self::NotMeasurable => "Not measurable yet",
        })
    }
}

/// One row of the summary.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub budget: &'static str,
    pub verdict: Verdict,
    pub measured: String,
}

pub struct Record<'a> {
    pub machine: &'a Machine,
    pub versions: &'a Versions,
    pub page: &'a PageResults,
    pub audio: &'a AudioRun,
}

impl Record<'_> {
    pub fn outcomes(&self) -> Vec<Outcome> {
        vec![
            self.cold_start(),
            self.table(),
            self.mix(),
            Outcome {
                budget: "Text search, under 50 ms",
                verdict: Verdict::NotMeasurable,
                measured: "Search is built in M2".to_owned(),
            },
            self.ui_thread(),
            self.audio(),
            self.memory(),
            self.wave(),
        ]
    }

    fn cold_start(&self) -> Outcome {
        let launches = &self.page.desktop.launches;
        let longest = launches.iter().map(|l| l.filled_ms).fold(0.0, f64::max);
        Outcome {
            budget: "Cold start to a usable library, under 1.5 s",
            verdict: met_if(!launches.is_empty() && longest < COLD_START_MS),
            measured: format!(
                "{} at the longest of {} launches, {} tracks",
                seconds(longest),
                launches.len(),
                thousands(u64::from(self.page.desktop.tracks))
            ),
        }
    }

    fn table(&self) -> Outcome {
        let passes: Vec<&FrameTimes> = self
            .page
            .desktop
            .scroll
            .iter()
            .flat_map(|run| [&run.fling, &run.jumps])
            .collect();
        let frames: u32 = passes.iter().map(|pass| pass.frames).sum();
        let missed: u32 = passes.iter().map(|pass| pass.missed_frames).sum();
        let longest = passes.iter().map(|pass| pass.max_ms).fold(0.0, f64::max);
        Outcome {
            budget: "Library table, 100,000 rows, fast scroll: holds the refresh rate, no frame over 16 ms",
            verdict: met_if(frames > 0 && missed == 0 && longest <= FRAME_MS),
            measured: format!(
                "{} missed frames in {}, longest {}",
                thousands(missed.into()),
                thousands(frames.into()),
                ms(longest)
            ),
        }
    }

    fn mix(&self) -> Outcome {
        let runs = &self.page.mix.runs;
        let frames: u32 = runs.iter().map(|run| run.frames).sum();
        let missed: u32 = runs.iter().map(|run| run.missed_frames).sum();
        let p99 = runs.iter().map(|run| run.p99_ms).fold(0.0, f64::max);
        let canvas = &self.page.mix.canvas;
        Outcome {
            budget: "Mix view: holds the refresh rate, 99th percentile under 8 ms at 120 Hz",
            verdict: met_if(frames > 0 && missed == 0 && p99 < MIX_P99_MS),
            measured: format!(
                "99th percentile {}, {} missed frames in {}, {} by {} canvas",
                ms(p99),
                thousands(missed.into()),
                thousands(frames.into()),
                canvas.width,
                canvas.height
            ),
        }
    }

    fn ui_thread(&self) -> Outcome {
        let (longest, during) =
            self.tasks().into_iter().fold(
                (0.0, ""),
                |worst, task| if task.0 > worst.0 { task } else { worst },
            );
        Outcome {
            budget: "UI thread never blocked for more than 16 ms",
            verdict: met_if(longest <= UI_THREAD_MS),
            measured: format!("Longest task {}, {during}", ms(longest)),
        }
    }

    fn tasks(&self) -> [(f64, &'static str); 3] {
        [
            (
                self.page.desktop.startup_longest_task_ms,
                "while the desktop app starts",
            ),
            (
                self.page.desktop.traced_longest_task_ms,
                "while the table scrolls and the wave draws",
            ),
            (
                self.page.mix.traced_longest_task_ms,
                "while the Mix view plays",
            ),
        ]
    }

    fn audio(&self) -> Outcome {
        let audio = self.audio;
        let verdict = if audio.device.period_frames > AUDIO_BUFFER_FRAMES || audio.dropouts() > 0 {
            Verdict::Missed
        } else if audio.seconds < AUDIO_SECONDS {
            Verdict::NotYetShown
        } else {
            Verdict::Met
        };
        Outcome {
            budget: "Audio stable at a 256-sample buffer or smaller, no dropouts in an hour",
            verdict,
            measured: format!(
                "{}-frame period, {} dropouts in {} s",
                audio.device.period_frames,
                audio.dropouts(),
                audio.seconds.round()
            ),
        }
    }

    fn memory(&self) -> Outcome {
        let idle = &self.page.desktop.idle_memory;
        Outcome {
            budget: "Idle memory with the library open, under 400 MB",
            verdict: met_if(
                idle.processes > 0 && idle.private_working_set_bytes < IDLE_MEMORY_BYTES,
            ),
            measured: format!(
                "{} private working set, {} working set",
                megabytes(idle.private_working_set_bytes),
                megabytes(idle.working_set_bytes)
            ),
        }
    }

    fn wave(&self) -> Outcome {
        let runs = &self.page.desktop.wave;
        let frames: u32 = runs.iter().map(|run| run.frames).sum();
        let longest = runs.iter().map(|run| run.longest_ms).fold(0.0, f64::max);
        Outcome {
            budget: "Wave component, under 1 ms of UI-thread time per frame",
            verdict: met_if(frames > 0 && longest < WAVE_MS),
            measured: format!(
                "Longest {} in {} frames",
                ms(longest),
                thousands(frames.into())
            ),
        }
    }
}

impl fmt::Display for Record<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let page = self.page;
        let desktop = &page.desktop;
        let mix = &page.mix;
        let audio = self.audio;
        let refresh = desktop.scroll.first().map_or(0.0, |run| run.refresh_ms);

        writeln!(f, "# Budgets at v{}\n", self.versions.resonance)?;
        writeln!(
            f,
            "Every budget in [architecture.md](../architecture.md#performance-budgets), measured on {} by `just bench`. It runs `ui/dev/bench.ts` for everything that needs a window, then `resonance bench` for the audio and this record. [ADR 0011](../adr/0011-bench-in-the-page-and-the-cli.md) says how each figure is taken.\n",
            page.date
        )?;
        writeln!(f, "| Budget | Result | Measured |")?;
        writeln!(f, "| --- | --- | --- |")?;
        for outcome in self.outcomes() {
            writeln!(
                f,
                "| {} | {} | {} |",
                outcome.budget, outcome.verdict, outcome.measured
            )?;
        }

        writeln!(f, "\n## Machine and versions\n")?;
        writeln!(f, "| | |")?;
        writeln!(f, "| --- | --- |")?;
        writeln!(
            f,
            "| Processor | {}, {} logical cores |",
            self.machine.processor, self.machine.logical_cores
        )?;
        writeln!(
            f,
            "| Memory | {:.0} GB |",
            self.machine.memory_bytes as f64 / 1e9
        )?;
        writeln!(f, "| System | {} |", self.machine.system)?;
        writeln!(f, "| Graphics in the desktop app | {} |", desktop.gpu)?;
        writeln!(f, "| Graphics in Edge | {} |", mix.gpu)?;
        writeln!(
            f,
            "| Display | {} a frame, about {:.0} Hz |",
            ms(refresh),
            if refresh > 0.0 { 1000.0 / refresh } else { 0.0 }
        )?;
        writeln!(
            f,
            "| Audio device | {}, {} kHz |",
            audio.device.name,
            f64::from(audio.device.sample_rate) / 1000.0
        )?;
        writeln!(
            f,
            "| Resonance | {}, commit {} |",
            self.versions.resonance, self.versions.commit
        )?;
        writeln!(f, "| Rust | {} |", self.versions.rustc)?;
        writeln!(f, "| WebView2 | {} |", desktop.webview)?;
        writeln!(f, "| Edge | {} |", mix.browser)?;
        writeln!(f, "| Node | {} |", page.node)?;

        writeln!(f, "\n## Cold start\n")?;
        writeln!(
            f,
            "From starting `resonance-app.exe` to the first filled rows of a {}-track library, with its files already in the disk cache. The table marks the moment its first rows fill, and the bench reads the mark three seconds later, so attaching to the page adds nothing. The marked rows are painted in the frame that follows.\n",
            thousands(desktop.tracks.into())
        )?;
        writeln!(f, "| Launch | To the page starting | To filled rows |")?;
        writeln!(f, "| --- | --- | --- |")?;
        writeln!(
            f,
            "| First, with a new webview profile | {} | {} |",
            seconds(desktop.first_launch.navigation_start_ms),
            seconds(desktop.first_launch.filled_ms)
        )?;
        for (index, launch) in desktop.launches.iter().enumerate() {
            writeln!(
                f,
                "| {} | {} | {} |",
                index + 1,
                seconds(launch.navigation_start_ms),
                seconds(launch.filled_ms)
            )?;
        }
        writeln!(
            f,
            "\nThe budget is judged on the numbered launches. The first launch after an install also makes the webview's profile."
        )?;

        writeln!(f, "\n## Library table\n")?;
        writeln!(
            f,
            "Five runs of `measureTableScroll()` in the desktop app at {} by {} CSS pixels and a device pixel ratio of {:.2}, as in [library-table.md](library-table.md). Each pass is a fling or a run of jumps. A frame is missed when it lasts more than one and a half display frames.\n",
            desktop.viewport.width, desktop.viewport.height, desktop.viewport.device_pixel_ratio
        )?;
        writeln!(
            f,
            "| Run | Pass | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |"
        )?;
        writeln!(f, "| --- | --- | --- | --- | --- | --- | --- |")?;
        for (index, run) in desktop.scroll.iter().enumerate() {
            for (pass, times) in [("Fling", &run.fling), ("Jumps", &run.jumps)] {
                writeln!(
                    f,
                    "| {} | {pass} | {} | {} | {} | {} | {} |",
                    index + 1,
                    ms(times.p50_ms),
                    ms(times.p99_ms),
                    ms(times.max_ms),
                    times.missed_frames,
                    times.frames_with_blank_rows
                )?;
            }
        }
        let filled: Vec<String> = desktop
            .scroll
            .iter()
            .map(|run| format!("{:.1}", run.filled_after_jumps_ms))
            .collect();
        writeln!(
            f,
            "\nAfter the last jump, every rendered row was filled in {} ms.",
            filled.join(", ")
        )?;

        writeln!(f, "\n## Mix view\n")?;
        writeln!(
            f,
            "Five runs of `measureMix()` in a headed Edge window with a page of {} by {} CSS pixels at a device pixel ratio of {:.2} and a canvas of {} by {}, while both decks played two MP3s from `fixtures-local`. The method is in [mix-view.md](mix-view.md). The view runs against `resonance serve` for now, so it is measured in the browser and not in the desktop app.\n",
            mix.viewport.width,
            mix.viewport.height,
            mix.viewport.device_pixel_ratio,
            mix.canvas.width,
            mix.canvas.height
        )?;
        writeln!(
            f,
            "| Run | Refresh | 99th percentile | Longest | Missed | View, longest | Callbacks | Underruns | Longest callback |"
        )?;
        writeln!(f, "| --- | --- | --- | --- | --- | --- | --- | --- | --- |")?;
        for (index, run) in mix.runs.iter().enumerate() {
            writeln!(
                f,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                index + 1,
                ms(run.refresh_ms),
                ms(run.p99_ms),
                ms(run.max_ms),
                run.missed_frames,
                ms(run.view_max_ms),
                thousands(run.audio.callbacks),
                run.audio.underruns,
                ms(run.audio.longest_callback_ms)
            )?;
        }
        let audio_problems: u64 = mix
            .runs
            .iter()
            .map(|run| {
                run.audio.late_callbacks + run.audio.starved_callbacks + run.audio.stream_errors
            })
            .sum();
        writeln!(
            f,
            "\nLate callbacks, starved callbacks and stream errors across the runs: {audio_problems}."
        )?;

        writeln!(f, "\n## UI thread\n")?;
        writeln!(
            f,
            "The longest task on the page's main thread in a Chromium trace. Tracing adds a little to every task, so these are upper bounds.\n"
        )?;
        writeln!(f, "| While | Longest task |")?;
        writeln!(f, "| --- | --- |")?;
        let [starting, scrolling, mixing] = self.tasks();
        writeln!(
            f,
            "| The desktop app starts, traced for its first 5 s | {} |",
            ms(starting.0)
        )?;
        writeln!(
            f,
            "| The desktop app scrolls the table once and draws the wave for 600 frames | {} |",
            ms(scrolling.0)
        )?;
        writeln!(
            f,
            "| The Mix view in Edge draws 1,200 frames with both decks playing | {} |",
            ms(mixing.0)
        )?;

        writeln!(f, "\n## Audio\n")?;
        writeln!(
            f,
            "Both decks played the same two MP3s through the default output device for {:.0} s, with no page open. The counts come from the audio callback, as in [mix-view.md](mix-view.md).\n",
            audio.seconds
        )?;
        writeln!(f, "| | |")?;
        writeln!(f, "| --- | --- |")?;
        writeln!(
            f,
            "| Period | {} frames, {} asked for |",
            audio.device.period_frames, audio.device.requested_buffer_frames
        )?;
        writeln!(f, "| Device buffer | {} frames |", audio.buffer_frames)?;
        writeln!(f, "| Callbacks | {} |", thousands(audio.callbacks))?;
        writeln!(
            f,
            "| Frames asked for in a callback | {} to {} |",
            audio.fewest_callback_frames, audio.most_callback_frames
        )?;
        writeln!(
            f,
            "| Underruns | {} ({} frames) |",
            audio.underruns, audio.missing_frames
        )?;
        writeln!(f, "| Late callbacks | {} |", audio.late_callbacks)?;
        writeln!(f, "| Starved callbacks | {} |", audio.starved_callbacks)?;
        writeln!(f, "| Stream errors | {} |", audio.stream_errors)?;
        writeln!(
            f,
            "| Longest callback | {} |",
            ms(audio.longest_callback_ms)
        )?;

        writeln!(f, "\n## Memory\n")?;
        writeln!(
            f,
            "The desktop app and every process under it: the webview's browser, GPU, renderer and helper processes. The private working set counts each process's own pages and leaves out pages it shares. The working set adds shared pages once for every process that maps them.\n"
        )?;
        writeln!(
            f,
            "| When | Processes | Private working set | Working set |"
        )?;
        writeln!(f, "| --- | --- | --- | --- |")?;
        memory_row(
            f,
            "Idle with the library open, 10 s after launch",
            &desktop.idle_memory,
        )?;
        memory_row(
            f,
            "After the scroll and wave runs",
            &desktop.after_runs_memory,
        )?;

        writeln!(f, "\n## Wave\n")?;
        writeln!(
            f,
            "Five runs of `measureWave()` in the desktop app, as in [wave.md](wave.md). The page's timer reads in steps of 0.1 ms, so 0.0 means under 0.1 ms.\n"
        )?;
        writeln!(f, "| Run | Median | 99th percentile | Longest |")?;
        writeln!(f, "| --- | --- | --- | --- |")?;
        for (index, run) in desktop.wave.iter().enumerate() {
            writeln!(
                f,
                "| {} | {} | {} | {} |",
                index + 1,
                ms(run.median_ms),
                ms(run.p99_ms),
                ms(run.longest_ms)
            )?;
        }
        Ok(())
    }
}

fn memory_row(f: &mut fmt::Formatter<'_>, when: &str, memory: &Memory) -> fmt::Result {
    writeln!(
        f,
        "| {when} | {} | {} | {} |",
        memory.processes,
        megabytes(memory.private_working_set_bytes),
        megabytes(memory.working_set_bytes)
    )
}

fn met_if(met: bool) -> Verdict {
    if met { Verdict::Met } else { Verdict::Missed }
}

fn ms(value: f64) -> String {
    format!("{value:.1} ms")
}

fn seconds(milliseconds: f64) -> String {
    format!("{:.2} s", milliseconds / 1000.0)
}

fn megabytes(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1e6)
}

fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use resonance_engine::Device;

    use super::*;

    fn page(json: &str) -> PageResults {
        serde_json::from_str(json).unwrap()
    }

    const PAGE: &str = r#"{
      "date": "13 November 2026",
      "node": "v24.13.1",
      "desktop": {
        "webview": "157.0.4341.46",
        "gpu": "ANGLE (Intel)",
        "viewport": { "width": 1440, "height": 900, "devicePixelRatio": 1.25 },
        "tracks": 100000,
        "firstLaunch": { "navigationStartMs": 639.0, "filledMs": 737.0 },
        "launches": [
          { "navigationStartMs": 400.0, "filledMs": 500.0 },
          { "navigationStartMs": 410.0, "filledMs": 520.0 }
        ],
        "startupLongestTaskMs": 27.0,
        "idleMemory": { "processes": 7, "workingSetBytes": 455000000, "privateWorkingSetBytes": 210000000 },
        "scroll": [{
          "rows": 100000,
          "refreshMs": 6.4,
          "fling": { "frames": 600, "p50Ms": 6.4, "p99Ms": 6.5, "maxMs": 18.7, "missedFrames": 2, "framesWithBlankRows": 0 },
          "jumps": { "frames": 600, "p50Ms": 6.4, "p99Ms": 6.5, "maxMs": 6.6, "missedFrames": 0, "framesWithBlankRows": 530 },
          "filledAfterJumpsMs": 30.5
        }],
        "wave": [{ "frames": 600, "width": 960, "height": 240, "points": "medium", "medianMs": 0.0, "p99Ms": 0.3, "longestMs": 0.4 }],
        "tracedLongestTaskMs": 5.4,
        "afterRunsMemory": { "processes": 7, "workingSetBytes": 470000000, "privateWorkingSetBytes": 230000000 }
      },
      "mix": {
        "browser": "157.0.4341.46",
        "gpu": "ANGLE (Intel)",
        "viewport": { "width": 1440, "height": 900, "devicePixelRatio": 1.25 },
        "canvas": { "width": 1640, "height": 820 },
        "tracks": ["D:/music/a.mp3", "D:/music/b.mp3"],
        "runs": [{
          "frames": 1200, "seconds": 7.7, "refreshMs": 6.4, "p50Ms": 6.4, "p99Ms": 6.6, "maxMs": 6.8,
          "missedFrames": 0, "framesOver8Ms": 0, "viewP50Ms": 0.1, "viewP99Ms": 0.3, "viewMaxMs": 0.4,
          "audio": {
            "sampleRate": 48000, "requestedBufferFrames": 256, "periodFrames": 480, "bufferFrames": 1056,
            "fewestCallbackFrames": 480, "mostCallbackFrames": 480, "callbacks": 770, "underruns": 0,
            "missingFrames": 0, "lateCallbacks": 0, "starvedCallbacks": 0, "streamErrors": 0,
            "longestCallbackMs": 0.19
          },
          "playhead": { "readings": 460, "p50ErrorMs": 0.1, "p99ErrorMs": 0.35, "maxErrorMs": 0.6 }
        }],
        "tracedLongestTaskMs": 3.2
      }
    }"#;

    fn audio(period_frames: u32, underruns: u64, seconds: f64) -> AudioRun {
        AudioRun {
            device: Device {
                name: "Speakers".to_owned(),
                sample_rate: 48_000,
                channels: 2,
                requested_buffer_frames: 256,
                period_frames,
            },
            buffer_frames: 1_056,
            seconds,
            callbacks: 18_000,
            underruns,
            missing_frames: underruns * 480,
            late_callbacks: 0,
            starved_callbacks: 0,
            stream_errors: 0,
            fewest_callback_frames: period_frames,
            most_callback_frames: period_frames,
            longest_callback_ms: 0.38,
        }
    }

    fn machine() -> Machine {
        Machine {
            processor: "Intel(R) Core(TM) Ultra 9 285H".to_owned(),
            logical_cores: 16,
            memory_bytes: 64_000_000_000,
            system: "Windows 11 Pro, build 26300".to_owned(),
        }
    }

    fn versions() -> Versions {
        Versions {
            resonance: "0.1.0".to_owned(),
            commit: "020fa48".to_owned(),
            rustc: "rustc 1.100.0".to_owned(),
        }
    }

    fn verdicts(page: &PageResults, audio: &AudioRun) -> Vec<(&'static str, Verdict)> {
        let (machine, versions) = (machine(), versions());
        let record = Record {
            machine: &machine,
            versions: &versions,
            page,
            audio,
        };
        record
            .outcomes()
            .into_iter()
            .map(|outcome| (outcome.budget, outcome.verdict))
            .collect()
    }

    #[test]
    fn every_budget_in_the_architecture_has_a_verdict() {
        let page = page(PAGE);
        let results = verdicts(&page, &audio(480, 0, 180.0));

        assert_eq!(
            results
                .iter()
                .map(|(_, verdict)| *verdict)
                .collect::<Vec<_>>(),
            [
                Verdict::Met,
                Verdict::Missed,
                Verdict::Met,
                Verdict::NotMeasurable,
                Verdict::Missed,
                Verdict::Missed,
                Verdict::Met,
                Verdict::Met,
            ]
        );
    }

    #[test]
    fn audio_is_met_only_at_256_frames_with_no_dropouts_for_an_hour() {
        let page = page(PAGE);
        let audio_verdict = |run: AudioRun| verdicts(&page, &run)[5].1;

        assert_eq!(audio_verdict(audio(256, 0, 3_600.0)), Verdict::Met);
        assert_eq!(audio_verdict(audio(256, 0, 180.0)), Verdict::NotYetShown);
        assert_eq!(audio_verdict(audio(256, 1, 3_600.0)), Verdict::Missed);
        assert_eq!(audio_verdict(audio(480, 0, 3_600.0)), Verdict::Missed);
    }

    #[test]
    fn the_record_states_each_result_with_its_numbers() {
        let page = page(PAGE);
        let (machine, versions, audio) = (machine(), versions(), audio(480, 0, 180.0));
        let text = Record {
            machine: &machine,
            versions: &versions,
            page: &page,
            audio: &audio,
        }
        .to_string();

        assert!(text.starts_with("# Budgets at v0.1.0\n"));
        assert!(text.contains("| Cold start to a usable library, under 1.5 s | Met | 0.52 s at the longest of 2 launches, 100,000 tracks |"));
        assert!(text.contains("| Missed | 2 missed frames in 1,200, longest 18.7 ms |"));
        assert!(text.contains("| Longest task 27.0 ms, while the desktop app starts |"));
        assert!(text.contains("| Missed | 480-frame period, 0 dropouts in 180 s |"));
        assert!(
            text.contains(
                "| Idle with the library open, 10 s after launch | 7 | 210 MB | 455 MB |"
            )
        );
        assert!(text.contains("| First, with a new webview profile | 0.64 s | 0.74 s |"));
        assert!(
            !text.contains("a.mp3"),
            "track names stay out of the record"
        );
    }

    #[test]
    fn thousands_are_grouped() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_200), "1,200");
        assert_eq!(thousands(100_000), "100,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
