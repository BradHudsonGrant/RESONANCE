use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "command", content = "params", rename_all = "snake_case")]
#[ts(export)]
pub enum Request {
    AppInfo,
    /// A page of the library in library order, for a table that shows the part in view.
    Tracks {
        offset: u32,
        limit: u32,
    },
    /// Reads a folder into the library without changing the files in it.
    Scan {
        folder: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "command", content = "result", rename_all = "snake_case")]
#[ts(export)]
pub enum Response {
    AppInfo(AppInfo),
    Tracks(TrackPage),
    Scan(ScanReport),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ScanReport {
    pub source_folder: String,
    pub found: usize,
    pub added: usize,
    pub changed: usize,
    pub unchanged: usize,
    /// The total, including files beyond the bounded list shown in the interface.
    pub unreadable_count: usize,
    /// The first hundred failures, so a damaged source cannot make an unbounded reply.
    pub unreadable: Vec<UnreadableFile>,
}

/// A file the scan passed over or could not read as audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct UnreadableFile {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TrackPage {
    /// How many tracks the whole library holds when the page was read.
    pub total: u32,
    pub offset: u32,
    pub tracks: Vec<LibraryTrack>,
}

/// One track as the library table and the inspector show it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct LibraryTrack {
    #[ts(type = "number")]
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<String>,
    pub track_number: Option<String>,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub bpm: Option<f64>,
    pub key: Option<TrackKey>,
    /// The key tag as the file has it, which may not be a key at all.
    pub key_tag: Option<String>,
    pub format: String,
    pub location: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    /// Missing when the file could not be read as audio.
    pub audio: Option<TrackAudio>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TrackKey {
    pub camelot: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TrackAudio {
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub bit_depth: Option<u8>,
    pub bitrate_kbps: Option<u32>,
}
