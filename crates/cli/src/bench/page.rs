//! What `ui/dev/bench.ts` writes to `target/bench/page.json`. The field names follow the page's
//! own measurement types, such as `ScrollMeasurement` and `MixMeasurement`.

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResults {
    /// The day of the run as the page driver wrote it, such as "13 November 2026".
    pub date: String,
    pub node: String,
    pub desktop: Desktop,
    pub mix: MixPage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Desktop {
    pub webview: String,
    pub gpu: String,
    pub viewport: Viewport,
    pub tracks: u32,
    pub first_launch: Launch,
    pub launches: Vec<Launch>,
    pub startup_longest_task_ms: f64,
    pub idle_memory: Memory,
    pub scroll: Vec<Scroll>,
    pub wave: Vec<Wave>,
    pub traced_longest_task_ms: f64,
    pub after_runs_memory: Memory,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub device_pixel_ratio: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub navigation_start_ms: f64,
    pub filled_ms: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub processes: u32,
    pub working_set_bytes: u64,
    pub private_working_set_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scroll {
    pub refresh_ms: f64,
    pub fling: FrameTimes,
    pub jumps: FrameTimes,
    pub filled_after_jumps_ms: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameTimes {
    pub frames: u32,
    pub p50_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub missed_frames: u32,
    pub frames_with_blank_rows: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wave {
    pub frames: u32,
    pub median_ms: f64,
    pub p99_ms: f64,
    pub longest_ms: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixPage {
    pub browser: String,
    pub gpu: String,
    pub viewport: Viewport,
    pub canvas: Size,
    pub tracks: [PathBuf; 2],
    pub runs: Vec<MixRun>,
    pub traced_longest_task_ms: f64,
}

#[derive(Debug, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixRun {
    pub frames: u32,
    pub refresh_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub missed_frames: u32,
    pub view_max_ms: f64,
    pub audio: MixAudio,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixAudio {
    pub callbacks: u64,
    pub underruns: u64,
    pub late_callbacks: u64,
    pub starved_callbacks: u64,
    pub stream_errors: u64,
    pub longest_callback_ms: f64,
}
