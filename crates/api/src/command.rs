use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(tag = "command", content = "params", rename_all = "snake_case")]
#[ts(export)]
pub enum Request {
    AppInfo,
    /// A page of matching library tracks, for a virtualised table.
    Tracks {
        offset: u32,
        limit: u32,
        #[serde(default)]
        #[ts(optional = nullable)]
        query: Option<TrackQuery>,
    },
    /// Marks or unmarks a track as a favourite.
    SetTrackFavourite {
        #[ts(type = "number")]
        track_id: i64,
        favourite: bool,
    },
    /// Every manual collection in sidebar display order, with its included track count.
    Collections,
    /// Creates a manual collection at the end of the list.
    CreateCollection {
        name: String,
    },
    /// Renames a manual collection.
    RenameCollection {
        #[ts(type = "number")]
        collection_id: i64,
        name: String,
    },
    /// Deletes a manual collection without deleting its tracks or files on disk.
    DeleteCollection {
        #[ts(type = "number")]
        collection_id: i64,
    },
    /// Adds one or more tracks to a manual collection.
    AddTracksToCollection {
        #[ts(type = "number")]
        collection_id: i64,
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
    },
    /// Removes one or more tracks from a manual collection.
    RemoveTracksFromCollection {
        #[ts(type = "number")]
        collection_id: i64,
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
    },
    /// Distinct labels and BPM/year bounds across the library, for filter controls.
    LibraryFacets,
    /// Reads a folder into the library without changing the files in it.
    Scan {
        folder: String,
    },
    /// Computes pending byte hashes, decoded-audio hashes and Chromaprint fingerprints.
    Identify,
    /// Active and recent background jobs for the Activity drawer.
    JobsSnapshot,
    /// Cancels a queued or running job.
    CancelJob {
        #[ts(type = "number")]
        job_id: i64,
    },
    /// Re-queues a failed or cancelled job.
    RetryJob {
        #[ts(type = "number")]
        job_id: i64,
    },
    /// Clears completed, failed and cancelled jobs from recent Activity history.
    ClearFinishedJobs,
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
    /// Loads a library track onto the single-track preview channel and plays from `start_ms`.
    PreviewLoadAndPlay {
        #[ts(type = "number")]
        track_id: i64,
        #[serde(default)]
        #[ts(optional = nullable, type = "number")]
        start_ms: Option<u64>,
    },
    /// Toggles preview playback between playing and paused (restarting from 0 at EOF).
    PreviewTogglePlay,
    /// Pauses preview playback if playing.
    PreviewPause,
    /// Seeks the loaded preview track to `position_ms`.
    PreviewSeek {
        #[ts(type = "number")]
        position_ms: u64,
    },
    /// Sets the linear output gain for preview playback (`0.0..=1.0`).
    PreviewSetVolume {
        volume: f32,
    },
    /// Current state of the single-track preview player.
    PreviewStatus,
    /// Discovers ranked audio-matched candidate files across sources (or inside `search_folder`)
    /// for relinking `track_id`.
    RelinkCandidates {
        #[ts(type = "number")]
        track_id: i64,
        #[serde(default)]
        #[ts(optional = nullable)]
        search_folder: Option<String>,
    },
    /// Computes audio identity on `candidate_path` and previews match evidence before relinking.
    PreviewRelink {
        #[ts(type = "number")]
        track_id: i64,
        candidate_path: String,
    },
    /// Repoints `track_id` to `candidate_path`, merging any duplicate scanned track row and
    /// clearing `missing_since`. Requires `force = true` when `PreviewRelink` reports
    /// `DifferentRecording`.
    ApplyRelink {
        #[ts(type = "number")]
        track_id: i64,
        candidate_path: String,
        #[serde(default)]
        force: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "command", content = "result", rename_all = "snake_case")]
#[ts(export)]
pub enum Response {
    AppInfo(AppInfo),
    Tracks(TrackPage),
    SetTrackFavourite(FavouriteState),
    Collections(Vec<LibraryCollection>),
    CreateCollection(LibraryCollection),
    RenameCollection(LibraryCollection),
    DeleteCollection(Vec<LibraryCollection>),
    AddTracksToCollection(LibraryCollection),
    RemoveTracksFromCollection(LibraryCollection),
    LibraryFacets(LibraryFacets),
    Scan(ScanReport),
    Identify(IdentifyReport),
    JobsSnapshot(ActivitySnapshot),
    CancelJob(ActivitySnapshot),
    RetryJob(ActivitySnapshot),
    ClearFinishedJobs(ActivitySnapshot),
    ActivityChanged(ActivityChanged),
    Sources(LibrarySources),
    SourceSubfolders(Vec<SourceSubfolder>),
    UpdateSource(LibrarySources),
    RemoveSource(LibrarySources),
    SetIgnoredFolders(LibrarySources),
    LibraryChanged(LibraryChanged),
    MixLoad(MixState),
    MixPlay(MixState),
    MixStatus(MixState),
    PreviewLoadAndPlay(PreviewState),
    PreviewTogglePlay(PreviewState),
    PreviewPause(PreviewState),
    PreviewSeek(PreviewState),
    PreviewSetVolume(PreviewState),
    PreviewStatus(PreviewState),
    RelinkCandidates(Vec<RelinkCandidate>),
    PreviewRelink(Box<RelinkPreview>),
    ApplyRelink(Box<LibraryTrack>),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ActivityChanged {
    #[ts(type = "number")]
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ActivityJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ActivityJobTier {
    Interactive,
    Metadata,
    Analysis,
    OnDemand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ActivityJob {
    #[ts(type = "number")]
    pub id: i64,
    pub kind: String,
    pub tier: ActivityJobTier,
    pub status: ActivityJobStatus,
    pub label: String,
    #[ts(type = "number")]
    pub done_items: u64,
    #[ts(type = "number | null")]
    pub total_items: Option<u64>,
    pub error_message: Option<String>,
    /// True when this job is waiting or paused because a Mix deck is playing.
    pub paused_for_mix: bool,
    #[ts(type = "number")]
    pub created_ms: i64,
    #[ts(type = "number | null")]
    pub started_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub finished_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ActivitySnapshot {
    pub active: Vec<ActivityJob>,
    pub recent: Vec<ActivityJob>,
    pub mix_backoff_active: bool,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TrackFilterPill {
    #[default]
    All,
    Favourites,
    Recent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TrackSortField {
    #[default]
    Index,
    Title,
    Artist,
    Bpm,
    Key,
    Duration,
    Added,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum TrackFormat {
    Mp3,
    Flac,
    Aiff,
    Wav,
    M4a,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrackQuery {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub text: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub pill: Option<TrackFilterPill>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub bpm_min: Option<f64>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub bpm_max: Option<f64>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub keys: Option<Vec<String>>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub year_min: Option<i32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub year_max: Option<i32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub label: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub formats: Option<Vec<TrackFormat>>,
    #[serde(default)]
    #[ts(optional = nullable, type = "number | null")]
    pub collection_id: Option<i64>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub missing_only: Option<bool>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub sort: Option<TrackSortField>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub direction: Option<SortDirection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FavouriteState {
    #[ts(type = "number")]
    pub track_id: i64,
    pub favourite: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct LibraryFacets {
    pub labels: Vec<String>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub missing_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TrackPage {
    /// Matching tracks for the current query.
    pub total: u32,
    /// Total included tracks in the library before search or filters.
    pub library_total: u32,
    pub offset: u32,
    pub tracks: Vec<LibraryTrack>,
}

/// One track as the library table and the inspector show it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct LibraryTrack {
    #[ts(type = "number")]
    pub id: i64,
    pub favourite: bool,
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
    pub collections: Vec<TrackCollection>,
    /// Unix epoch milliseconds when the file was found missing on disk, if missing.
    #[ts(type = "number | null")]
    pub missing_since: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LibraryCollection {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    #[ts(type = "number")]
    pub position: i64,
    pub track_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TrackCollection {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
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

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PreviewState {
    #[ts(type = "number | null")]
    pub track_id: Option<i64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub playing: bool,
    #[ts(type = "number")]
    pub position_ms: u64,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub volume: f32,
    pub generation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum RelinkMatchEvidence {
    ExactBytes,
    ExactAudio,
    SameRecording { similarity: f32 },
    DifferentRecording { similarity: Option<f32> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RelinkFileSummary {
    pub path: String,
    pub format: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[ts(type = "number | null")]
    pub duration_ms: Option<u64>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub bitrate_kbps: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelinkCandidate {
    pub candidate_path: String,
    #[ts(type = "number | null")]
    pub existing_track_id: Option<i64>,
    pub evidence: RelinkMatchEvidence,
    pub file: RelinkFileSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelinkPreview {
    #[ts(type = "number")]
    pub track_id: i64,
    pub evidence: RelinkMatchEvidence,
    pub requires_confirmation: bool,
    pub current: RelinkFileSummary,
    pub candidate: RelinkFileSummary,
    #[ts(type = "number | null")]
    pub merges_existing_track_id: Option<i64>,
}
