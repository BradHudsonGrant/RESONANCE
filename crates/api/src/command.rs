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
    /// Computes pending byte hashes, decoded-audio hashes and Chromaprint fingerprints.
    Identify,
    /// Every configured source folder and the editable ignored-folder list.
    Sources,
    /// Direct subfolders inside a source (or under `parent`), for toggling subfolders on or off.
    SourceSubfolders {
        #[ts(type = "number")]
        source_id: i64,
        parent: Option<String>,
    },
    /// Updates a source's subfolder and watcher options.
    UpdateSource {
        #[ts(type = "number")]
        source_id: i64,
        include_subfolders: bool,
        watch: bool,
        excluded_subfolders: Vec<String>,
    },
    /// Removes a source and any tracks that came only from it.
    RemoveSource {
        #[ts(type = "number")]
        source_id: i64,
    },
    /// Replaces the editable list of folder names skipped at any depth.
    SetIgnoredFolders {
        folders: Vec<String>,
    },
    /// Puts a file on a deck, stopped at its start. The first load opens the audio device.
    MixLoad {
        deck: MixDeck,
        path: String,
    },
    /// Starts or stops a deck.
    MixPlay {
        deck: MixDeck,
        playing: bool,
    },
    /// The audio device and what each deck holds. Positions and meters come as binary frames.
    MixStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "command", content = "result", rename_all = "snake_case")]
#[ts(export)]
pub enum Response {
    AppInfo(AppInfo),
    Tracks(TrackPage),
    Scan(ScanReport),
    Identify(IdentifyReport),
    Sources(LibrarySources),
    SourceSubfolders(Vec<SourceSubfolder>),
    UpdateSource(LibrarySources),
    RemoveSource(LibrarySources),
    SetIgnoredFolders(LibrarySources),
    LibraryChanged(LibraryChanged),
    MixLoad(MixState),
    MixPlay(MixState),
    MixStatus(MixState),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LibrarySources {
    pub sources: Vec<LibrarySource>,
    pub ignored_folders: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LibrarySource {
    #[ts(type = "number")]
    pub id: i64,
    pub folder: String,
    pub include_subfolders: bool,
    pub watch: bool,
    pub watching_active: bool,
    pub excluded_subfolders: Vec<String>,
    pub track_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SourceSubfolder {
    pub path: String,
    pub name: String,
    pub enabled: bool,
    pub has_subfolders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LibraryChanged {
    #[ts(type = "number")]
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct IdentifyReport {
    pub processed: usize,
    pub decoded: usize,
    pub reconciled_moves: usize,
    pub skipped_unreadable: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MixDeck {
    A,
    B,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct MixState {
    /// Missing until the first load opens the device.
    pub device: Option<MixDevice>,
    pub decks: Vec<MixDeckState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct MixDevice {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub requested_buffer_frames: u32,
    /// The period the device runs its callback at, whatever was asked for.
    pub period_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct MixDeckState {
    pub deck: MixDeck,
    /// Changes with every load and matches the waveform frame for the same track. Zero while the
    /// deck is empty.
    pub generation: u32,
    pub path: Option<String>,
    pub duration_seconds: f64,
    pub playing: bool,
}
