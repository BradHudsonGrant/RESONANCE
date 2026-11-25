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
    /// Every column the filter builder offers, with its conditions.
    FilterFields,
    /// How each of the query's filter terms reads, and how many tracks it keeps on its own.
    FilterSummaries {
        query: TrackQuery,
    },
    /// Reads a command bar filter such as `/missing genre` or `/bitrate < 320`.
    ParseFilter {
        text: String,
    },
    /// Counts one use of a filter for the recommended quick filters.
    RecordFilterUse {
        term: FilterTerm,
    },
    /// The pinned and recommended quick filters.
    QuickFilters,
    ChangeQuickFilters {
        change: QuickFilterChange,
    },
    /// Every smart view in sidebar order.
    SmartViews,
    /// Saves the query's search text, pill and filters as a smart view.
    SaveSmartView {
        name: String,
        query: TrackQuery,
    },
    RenameSmartView {
        #[ts(type = "number")]
        view_id: i64,
        name: String,
    },
    DeleteSmartView {
        #[ts(type = "number")]
        view_id: i64,
    },
    /// The library table's columns, wrap setting and sort, and every column it can show.
    TableLayout,
    /// Changes the library table and keeps the change for next time.
    ChangeTableLayout {
        change: TableLayoutChange,
    },
    /// Reads a folder into the library without changing the files in it.
    Scan {
        folder: String,
    },
    /// Computes pending byte hashes, decoded-audio hashes and Chromaprint fingerprints.
    Identify,
    /// Looks tracks up online as an Activity job: the given tracks first, then every other track
    /// without an online result (or only `track_ids` when `only_selected` is true).
    IdentifyOnline {
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
        #[serde(default)]
        only_selected: bool,
    },
    /// Looks up a pasted release link (Discogs, Bandcamp, MusicBrainz, Deezer, Apple Music,
    /// SoundCloud, or YouTube) for one track.
    IdentifyWithUrl {
        #[ts(type = "number")]
        track_id: i64,
        url: String,
    },
    /// Active and recent background jobs for the Activity drawer.
    JobsSnapshot,
    /// Cancels a queued or running job.
    CancelJob {
        #[ts(type = "number")]
        job_id: i64,
    },
    /// Pauses a queued or running job. A running one stops at its next safe point.
    PauseJob {
        #[ts(type = "number")]
        job_id: i64,
    },
    /// Lets a paused job carry on from where it stopped.
    ResumeJob {
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
    /// The user moved to a row. With auto-preview on, the preview swaps to it.
    PreviewFocus {
        #[ts(type = "number")]
        track_id: i64,
    },
    /// Whether auto-preview is on and where previews start.
    PreviewSettings,
    SetPreviewSettings {
        settings: PreviewSettings,
    },
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
    /// Which provider keys are saved. The keys themselves never leave the credential store.
    ProviderKeys,
    /// Saves a provider key in the OS credential store, replacing any earlier one.
    SetProviderKey {
        provider: ProviderKeyKind,
        value: ProviderKeyValue,
    },
    /// Removes a provider key from the OS credential store. Steps that need it are skipped.
    ClearProviderKey {
        provider: ProviderKeyKind,
    },
    /// A page of tracks with proposals still to decide, in track order.
    StagingQueue {
        offset: u32,
        limit: u32,
    },
    /// Every staged field of one track: its value now, where that came from, and the proposal.
    StagingDiff {
        #[ts(type = "number")]
        track_id: i64,
    },
    /// Takes the proposals for `fields` into the user's values, even for fields the user holds.
    AcceptProposedFields {
        #[ts(type = "number")]
        track_id: i64,
        fields: Vec<TagName>,
    },
    /// Takes every proposal at or above `min_confidence` (0 to 1) that changes a field the user
    /// does not hold, on every track.
    AcceptAllAboveThreshold {
        min_confidence: f64,
    },
    /// Skips the track's pending proposals for `fields`, or all of them when it is empty.
    SkipProposal {
        #[ts(type = "number")]
        track_id: i64,
        #[serde(default)]
        fields: Vec<TagName>,
    },
    /// The picture a track shows and the release picture identification proposes for it.
    TrackArtwork {
        #[ts(type = "number")]
        track_id: i64,
    },
    /// Makes the proposed picture the track's picture. It reaches the file with the next tag
    /// write.
    AcceptProposedArtwork {
        #[ts(type = "number")]
        track_id: i64,
    },
    SkipProposedArtwork {
        #[ts(type = "number")]
        track_id: i64,
    },
    /// Makes a JPEG or PNG the user picked the track's picture, sent as base64.
    SetTrackArtwork {
        #[ts(type = "number")]
        track_id: i64,
        image_base64: String,
    },
    /// Sets the user's values on one or many tracks. Nothing is written to the files.
    SetUserTag {
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
        edits: Vec<UserTagEdit>,
    },
    /// What writing the user's values into the files would change, for `track_ids` or for every
    /// track with unwritten values when it is empty. Nothing on disk changes.
    PreviewTagWrites {
        #[serde(default)]
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
    },
    /// Writes what `PreviewTagWrites` lists into the files as one journaled operation.
    ApplyTagWrites {
        #[serde(default)]
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
    },
    /// Saves selected proposals. A null field list takes every unlocked field. An empty list
    /// takes only the cover when requested. Mix and subgenre are saved in the library.
    AcceptAndWrite {
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
        fields: Option<Vec<TagName>>,
        include_artwork: bool,
        #[serde(default)]
        #[ts(optional)]
        expected_review: Option<String>,
    },
    /// The journal of changes to files, newest first.
    RecentOperations {
        limit: u32,
    },
    /// Puts back what an applied operation changed.
    UndoOperation {
        #[ts(type = "number")]
        operation_id: i64,
    },
    /// What renaming the files in `scope` with `template` would do. Nothing on disk changes.
    /// `files` lists at most `limit` of them, problems first.
    PreviewRename {
        scope: RenameScope,
        template: String,
        limit: u32,
    },
    /// Renames the ready files in `scope` as one journaled operation. A large batch runs as an
    /// Activity job.
    ApplyRename {
        scope: RenameScope,
        template: String,
    },
    /// All track IDs in the filtered Prep view, optionally restricted to a Library selection.
    PrepTrackIds {
        #[serde(default)]
        #[ts(optional = nullable)]
        query: Option<TrackQuery>,
        #[serde(default)]
        #[ts(optional = nullable, type = "Array<number>")]
        track_ids: Option<Vec<i64>>,
        #[serde(default)]
        #[ts(optional = nullable)]
        stage: Option<PrepStageName>,
    },
    /// The order of the whole Prep view and how many of its tracks sit in each stage. With
    /// y_stage, tracks are grouped by stage instead of the query's own sort.
    PrepOrder {
        #[serde(default)]
        #[ts(optional = nullable)]
        query: Option<TrackQuery>,
        #[serde(default)]
        #[ts(optional = nullable, type = "Array<number>")]
        track_ids: Option<Vec<i64>>,
        #[serde(default)]
        #[ts(optional = nullable)]
        by_stage: Option<SortDirection>,
    },
    /// The Prep queue and staging diffs, either for explicit `track_ids`, a batch matching `query`
    /// sent from Library, pending proposals, or the whole library.
    PrepQueue {
        #[serde(default)]
        #[ts(optional = nullable)]
        query: Option<TrackQuery>,
        #[serde(default = "default_true")]
        proposals_only: bool,
        #[serde(default)]
        #[ts(optional = nullable, type = "Array<number>")]
        track_ids: Option<Vec<i64>>,
    },
    /// Accepts pending proposals on one or more tracks (such as a release group) in one
    /// transaction. When `fields` is empty, takes all pending unlocked changing fields.
    AcceptPrepItems {
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
        #[serde(default)]
        fields: Vec<TagName>,
        #[serde(default)]
        #[ts(optional = nullable)]
        candidate_index: Option<u32>,
        #[serde(default)]
        skip_remaining: bool,
        #[serde(default)]
        #[ts(optional = nullable)]
        include_artwork: Option<bool>,
    },
    /// Switches the active identification candidate for a Prep item and re-stages its proposals.
    SelectPrepCandidate {
        #[ts(type = "number")]
        track_id: i64,
        candidate_index: u32,
        #[serde(default)]
        #[ts(optional)]
        expected_review: Option<String>,
    },
    /// Stages or applies release artwork from a stored candidate without changing tag proposals
    /// or `selected_candidate`.
    UsePrepCandidateArtwork {
        #[ts(type = "number")]
        track_id: i64,
        candidate_index: u32,
        accept: bool,
        #[serde(default)]
        #[ts(optional)]
        expected_review: Option<String>,
    },
    /// Fetches and caches cover artwork for one stored Prep candidate without modifying proposals.
    FetchPrepCandidateArtwork {
        #[ts(type = "number")]
        track_id: i64,
        candidate_index: u32,
    },
    /// Sets aside the suggested cover on one track and keeps the file's own.
    SkipPrepArtwork {
        #[ts(type = "number")]
        track_id: i64,
    },
    /// Skips pending proposals on one or more tracks in one transaction.
    SkipPrepItems {
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
        #[serde(default)]
        fields: Vec<TagName>,
    },
    /// Previews how many tracks and fields a batch tag tool will update in the user tag layer.
    PreviewBatchTag {
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
        tool: BatchTagTool,
    },
    /// Applies a batch tag tool across `track_ids` (or a release folder) in the user tag layer.
    ApplyBatchTag {
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
        tool: BatchTagTool,
    },
    /// Reads identification behaviour settings.
    IdentificationSettings,
    /// Updates identification behaviour settings and rebuilds the active pipeline.
    SetIdentificationSettings {
        settings: IdentificationSettings,
    },
}

fn default_true() -> bool {
    true
}

/// Which files a rename covers: the selected tracks, or every track a filter or collection
/// shows. An empty query is the whole library.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum RenameScope {
    Tracks {
        #[ts(type = "Array<number>")]
        track_ids: Vec<i64>,
    },
    Query {
        query: TrackQuery,
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
    FilterFields(Vec<FilterField>),
    FilterSummaries(Vec<FilterSummary>),
    ParseFilter(DescribedFilter),
    RecordFilterUse(QuickFilters),
    QuickFilters(QuickFilters),
    ChangeQuickFilters(QuickFilters),
    SmartViews(Vec<SmartView>),
    SaveSmartView(Vec<SmartView>),
    RenameSmartView(Vec<SmartView>),
    DeleteSmartView(Vec<SmartView>),
    TableLayout(LibraryTableLayout),
    ChangeTableLayout(LibraryTableLayout),
    Scan(ScanReport),
    Identify(IdentifyReport),
    IdentifyOnline(ActivitySnapshot),
    IdentifyWithUrl(TrackIdentification),
    JobsSnapshot(ActivitySnapshot),
    CancelJob(ActivitySnapshot),
    PauseJob(ActivitySnapshot),
    ResumeJob(ActivitySnapshot),
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
    PreviewFocus(PreviewState),
    PreviewSettings(PreviewSettings),
    SetPreviewSettings(PreviewSettings),
    RelinkCandidates(Vec<RelinkCandidate>),
    PreviewRelink(Box<RelinkPreview>),
    ApplyRelink(Box<LibraryTrack>),
    ProviderKeys(Vec<ProviderKeyStatus>),
    SetProviderKey(ProviderKeyStatus),
    ClearProviderKey(ProviderKeyStatus),
    StagingQueue(StagingQueuePage),
    StagingDiff(TrackStagingDiff),
    AcceptProposedFields(TrackStagingDiff),
    AcceptAllAboveThreshold(AcceptedProposals),
    SkipProposal(TrackStagingDiff),
    TrackArtwork(TrackArtwork),
    AcceptProposedArtwork(TrackArtwork),
    SkipProposedArtwork(TrackArtwork),
    SetTrackArtwork(TrackArtwork),
    SetUserTag(UserTagReport),
    PreviewTagWrites(Vec<FileTagWrite>),
    ApplyTagWrites(TagWriteReport),
    AcceptAndWrite(AcceptedTagWriteReport),
    PreviewRename(RenamePreview),
    ApplyRename(RenameReport),
    RecentOperations(Vec<JournalOperation>),
    UndoOperation(UndoReport),
    PrepTrackIds(#[ts(type = "Array<number>")] Vec<i64>),
    PrepOrder(PrepOrderSnapshot),
    PrepQueue(PrepQueueSnapshot),
    AcceptPrepItems(AcceptedProposals),
    SelectPrepCandidate(Box<PrepItem>),
    UsePrepCandidateArtwork(Box<PrepItem>),
    FetchPrepCandidateArtwork(TrackCandidateArtwork),
    SkipPrepArtwork(Box<PrepItem>),
    SkipPrepItems(AcceptedProposals),
    PreviewBatchTag(AcceptedProposals),
    ApplyBatchTag(AcceptedProposals),
    IdentificationSettings(IdentificationSettings),
    SetIdentificationSettings(IdentificationSettings),
}

/// One field a tag write changes. `null` is no value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FieldChange {
    pub field: TagName,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// One file a tag write would change. `blocked` says why it cannot be written as things stand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FileTagWrite {
    #[ts(type = "number")]
    pub track_id: i64,
    pub path: String,
    pub fields: Vec<FieldChange>,
    /// Whether the write puts the user's picture in as the front cover.
    pub cover: bool,
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FailedTagWrite {
    #[ts(type = "number")]
    pub track_id: i64,
    pub path: String,
    pub reason: String,
}

/// `operation_id` is `null` when no file was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TagWriteReport {
    #[ts(type = "number | null")]
    pub operation_id: Option<i64>,
    pub written: u32,
    pub failed: Vec<FailedTagWrite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AcceptedTagWriteReport {
    #[serde(flatten)]
    #[ts(flatten)]
    pub files: TagWriteReport,
    pub library_fields: u32,
    #[serde(default)]
    pub reverted_fields: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OperationKindName {
    WriteTags,
    RenameFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RenameStatusName {
    Unchanged,
    Ready,
    Collision,
    MissingFields,
    TooLong,
    FileMissing,
}

/// One file in a rename preview. `new_name` is `null` when tags are missing or there is no
/// room for a name; `missing` lists the empty tags the template needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RenameFile {
    #[ts(type = "number")]
    pub track_id: i64,
    pub status: RenameStatusName,
    pub old_name: String,
    pub new_name: Option<String>,
    pub missing: Vec<TagName>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RenamePreview {
    pub total: u32,
    pub unchanged: u32,
    pub ready: u32,
    pub collision: u32,
    pub missing_fields: u32,
    pub too_long: u32,
    pub file_missing: u32,
    pub files: Vec<RenameFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FailedRename {
    #[ts(type = "number")]
    pub track_id: i64,
    pub path: String,
    pub reason: String,
}

/// `queued` is true when the batch went to Activity, and the counts are then zero.
/// `operation_id` is `null` when no file was renamed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RenameReport {
    #[ts(type = "number | null")]
    pub operation_id: Option<i64>,
    pub renamed: u32,
    pub failed: Vec<FailedRename>,
    pub queued: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OperationStateName {
    Pending,
    Applied,
    Failed,
    Undone,
}

/// One change to files in the journal. Only an `applied` one can be undone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct JournalOperation {
    #[ts(type = "number")]
    pub id: i64,
    pub kind: OperationKindName,
    pub state: OperationStateName,
    pub summary: String,
    pub can_undo: bool,
    #[ts(type = "number")]
    pub created_ms: i64,
    #[ts(type = "number | null")]
    pub applied_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub undone_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct UndoReport {
    pub operation: JournalOperation,
    pub restored: u32,
}

/// A tag field, as staging and tag edits name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TagName {
    Title,
    Artist,
    Mix,
    Album,
    AlbumArtist,
    Genre,
    Subgenre,
    Label,
    #[serde(rename = "catno")]
    CatNo,
    Year,
    Bpm,
    Key,
    TrackNumber,
    Comment,
    Composer,
    Remixer,
}

/// Which layer a value shows from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TagProvenance {
    File,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProposalState {
    Pending,
    Accepted,
    Skipped,
}

/// One field of a track's staging diff.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct StagedField {
    pub field: TagName,
    pub current_value: Option<String>,
    pub current_provenance: Option<TagProvenance>,
    pub proposed_value: Option<String>,
    pub provider: Option<String>,
    pub confidence: Option<f64>,
    /// True only when Chromaprint compared the audio and it matched.
    pub audio_matched: bool,
    pub status: Option<ProposalState>,
    /// The user typed or accepted this field, so identification never changes it.
    pub locked_by_user: bool,
    /// A pending proposal that would change what the track shows.
    pub changes: bool,
    /// True when writing tags to disk will modify this field in the file.
    pub dirty_on_disk: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TrackStagingDiff {
    #[ts(type = "number")]
    pub track_id: i64,
    pub fields: Vec<StagedField>,
}

/// A track's picture by hash. The page loads the bytes from `/artwork/<hash>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TrackArtwork {
    #[ts(type = "number")]
    pub track_id: i64,
    /// What the track shows: the user's picture, else its file's front cover.
    pub artwork: Option<String>,
    /// A release picture waiting for the user's decision.
    pub proposed: Option<ProposedTrackArtwork>,
    /// True when the user's cover has not yet been written to the file.
    pub dirty_on_disk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ProposedTrackArtwork {
    pub artwork: String,
    /// The provider's name as the page shows it.
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct StagingQueueItem {
    #[ts(type = "number")]
    pub track_id: i64,
    pub pending_fields: u32,
    /// The highest confidence among the pending proposals.
    pub confidence: f64,
    pub audio_matched: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct StagingQueuePage {
    pub total: u32,
    pub offset: u32,
    pub items: Vec<StagingQueueItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AcceptedProposals {
    pub tracks: u32,
    pub fields: u32,
}

/// `value` sets the user's value, an empty one blanking the file's. `null` hands the field back
/// to the file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export)]
pub struct UserTagEdit {
    pub field: TagName,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct UserTagReport {
    pub tracks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ProviderKeyKind {
    #[serde(rename = "acoustid")]
    AcoustId,
    #[serde(rename = "discogs")]
    Discogs,
    #[serde(rename = "audd")]
    AudD,
}

/// A key on its way into the credential store. Write-only: it is never serialised, and its
/// `Debug` output hides the value.
#[derive(Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, type = "string")]
pub struct ProviderKeyValue(pub String);

impl std::fmt::Debug for ProviderKeyValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderKeyValue(..)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ProviderKeyStatus {
    pub provider: ProviderKeyKind,
    pub configured: bool,
    /// Paid recognisers, shown apart from the free providers.
    pub paid: bool,
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
    /// True when the user paused this job.
    pub paused: bool,
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

/// What identification concluded for a track, and what each step did.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TrackIdentification {
    #[ts(type = "number")]
    pub track_id: i64,
    pub best: Option<IdentifiedRelease>,
    pub needs_url: bool,
    pub steps: Vec<IdentifyStepReport>,
}

/// The best candidate. `audio_matched` is true only when Chromaprint compared the audio.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct IdentifiedRelease {
    pub provider: String,
    pub confidence: f64,
    pub audio_matched: bool,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub mix: Option<String>,
    pub album: Option<String>,
    pub label: Option<String>,
    pub catno: Option<String>,
    pub year: Option<u16>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct IdentifyStepReport {
    /// 1 to 6, in the order the steps run.
    pub step: u8,
    pub provider: Option<String>,
    pub outcome: IdentifyStepOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum IdentifyStepOutcome {
    Read { fields: u32 },
    Ran { candidates: u32, best: Option<f64> },
    SkippedMissingKey { key: ProviderKeyKind },
    SkippedNoInput,
    SkippedConfident,
    Failed { reason: String },
    NeedsUrl,
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
    Album,
    Genre,
    Label,
    #[serde(rename = "catno")]
    CatNo,
    Year,
    Comment,
    Composer,
    Remixer,
    Mix,
    Format,
    Bitrate,
    SampleRate,
    BitDepth,
    Channels,
    FileSize,
    Modified,
    Path,
    /// A tag with no column of its own.
    Tag(TagName),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
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
    /// Filters on columns, joined by AND.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub filters: Option<Vec<FilterTerm>>,
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
    /// Orders tracks that tie on `sort`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub then_sort: Option<TrackSortField>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub then_direction: Option<SortDirection>,
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
    pub mix: Option<String>,
    pub subgenre: Option<String>,
    pub catno: Option<String>,
    pub composer: Option<String>,
    pub remixer: Option<String>,
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
    /// The hash of the picture the track shows, loaded from `/artwork/<hash>`.
    pub artwork: Option<String>,
    /// Unix epoch milliseconds when the track came into the library.
    #[ts(type = "number")]
    pub added_ms: i64,
    /// Unix epoch milliseconds when the file was last changed on disk.
    #[ts(type = "number")]
    pub modified_ms: i64,
}

/// What a filter asks of its column. Numbers are in seconds for Time, bytes for File size,
/// hertz for Sample rate and Unix milliseconds for dates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export)]
pub enum FilterCondition {
    Contains {
        value: String,
    },
    Is {
        value: String,
    },
    IsNot {
        value: String,
    },
    StartsWith {
        value: String,
    },
    AnyOf {
        values: Vec<String>,
    },
    NoneOf {
        values: Vec<String>,
    },
    /// Inclusive at both ends. An absent end has no bound.
    Range {
        #[serde(default)]
        #[ts(optional = nullable)]
        min: Option<f64>,
        #[serde(default)]
        #[ts(optional = nullable)]
        max: Option<f64>,
    },
    Below {
        value: f64,
    },
    Above {
        value: f64,
    },
    Missing,
    Present,
}

/// The name of a condition, without its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FilterConditionName {
    Contains,
    Is,
    IsNot,
    StartsWith,
    AnyOf,
    NoneOf,
    Range,
    Below,
    Above,
    Missing,
    Present,
}

/// One condition on one column, by the column's id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrackFilter {
    pub column: String,
    pub condition: FilterCondition,
}

/// One AND term of a query: one filter, or several of which any may match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FilterTerm {
    pub any_of: Vec<TrackFilter>,
}

/// What a column holds, which decides the conditions it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FilterFieldKind {
    Text,
    Number,
    Date,
    Choice,
    Picture,
    Location,
}

/// A value a choice column offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FilterChoice {
    pub value: String,
    pub label: String,
}

/// A column the filter builder offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FilterField {
    pub column: String,
    pub label: String,
    /// The column's name inside a sentence, such as "catalogue number".
    pub noun: String,
    pub group: TableColumnGroup,
    pub kind: FilterFieldKind,
    pub conditions: Vec<FilterConditionName>,
    /// The values a choice column holds. Empty for every other kind.
    pub choices: Vec<FilterChoice>,
}

/// A filter term with how it reads and the key that names it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct DescribedFilter {
    pub term: FilterTerm,
    pub label: String,
    pub key: String,
}

/// One active filter term as the bar shows it. `count` is the tracks it keeps on its own inside
/// the search, pill and collection, and is absent when it is the only filter, whose count is the
/// page's total.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct FilterSummary {
    pub label: String,
    pub key: String,
    pub count: Option<u32>,
}

/// The quick filters bar: pinned filters, then the recommended ones.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct QuickFilters {
    pub pinned: Vec<DescribedFilter>,
    pub recommended: Vec<DescribedFilter>,
    pub recommend: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum QuickFilterChange {
    /// Pins a filter at the end.
    Pin {
        term: FilterTerm,
    },
    /// Unpins a pinned filter, or stops a recommended one coming back.
    Remove {
        term: FilterTerm,
    },
    /// Puts a filter at a place in the bar, counting pinned then recommended. A recommended
    /// filter that moves is pinned.
    Move {
        term: FilterTerm,
        to: u32,
    },
    SetRecommend {
        on: bool,
    },
}

/// A saved set of filters, search text and pill.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SmartView {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub query: TrackQuery,
}

/// Where the column chooser lists a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TableColumnGroup {
    Track,
    Release,
    Audio,
    File,
    Tags,
}

/// A column the library table can show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableColumn {
    /// Stable name, such as `genre` or `tag:album_artist`.
    pub id: String,
    pub label: String,
    pub group: TableColumnGroup,
    pub shown: bool,
    /// The width the user set. Absent for a column at its default.
    pub width_px: Option<u16>,
    /// Absent for a column that shares the room the others leave.
    pub default_width_px: Option<u16>,
    /// The least room the column needs.
    pub floor_width_px: u16,
    pub numeric: bool,
    pub sortable: bool,
    /// Whether the user can hide it. The title stays.
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableSortKey {
    /// `index` for library order, else a column id.
    pub column: String,
    pub field: TrackSortField,
    pub direction: SortDirection,
}

/// The library table as the user left it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LibraryTableLayout {
    /// The shown columns, in order.
    pub columns: Vec<TableColumn>,
    /// Every column the chooser offers, shown or not, in chooser order.
    pub available: Vec<TableColumn>,
    pub wrap: bool,
    /// Primary sort first. Empty means library order.
    pub sort: Vec<TableSortKey>,
    /// Column ids in the order they give way on a narrow table.
    pub drop_order: Vec<String>,
    pub min_width_px: u16,
    pub max_width_px: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum TableLayoutChange {
    /// Shows a hidden column at the end, or hides a shown one.
    Toggle {
        column: String,
    },
    /// Puts a shown column at a place among the shown columns.
    Move {
        column: String,
        to: u32,
    },
    /// Sets a column's width, or puts back its default when absent.
    Resize {
        column: String,
        #[serde(default)]
        #[ts(optional = nullable)]
        width_px: Option<u16>,
    },
    SetWrap {
        wrap: bool,
    },
    /// A header click. `add` is a Shift click, for the secondary sort.
    Sort {
        column: String,
        add: bool,
    },
    Reset,
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
/// A key in each notation the user can choose to read keys in. Sort and filter use `camelot`.
pub struct TrackKey {
    pub camelot: String,
    pub open_key: String,
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
    /// The hash of the picture the playing track shows.
    pub artwork: Option<String>,
    pub playing: bool,
    #[ts(type = "number")]
    pub position_ms: u64,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub volume: f32,
    pub generation: u32,
}

/// Where a preview started by auto-preview begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum PreviewStartPoint {
    Beginning,
    ThirtyPercent,
    /// Held between 0 and `max_start_seconds`.
    Seconds {
        seconds: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PreviewSettings {
    pub auto_preview: bool,
    pub start: PreviewStartPoint,
    /// The furthest a fixed start can be set. The core ignores what is sent here.
    #[serde(default)]
    pub max_start_seconds: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MatchBasis {
    AudioFingerprint,
    AudioPreview,
    AudioRecogniser,
    UserUrl,
    EmbeddedUrl,
    CatalogAndDuration,
    TagsAndDuration,
    TagsOnly,
    LocalTags,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct MatchEvidence {
    pub basis: MatchBasis,
    pub providers: Vec<String>,
    pub primary_provider: String,
    #[ts(type = "number | null")]
    pub duration_delta_ms: Option<u64>,
    pub preview_similarity: Option<f32>,
    pub catno_matched: bool,
    pub isrc_matched: bool,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct MatchCandidateSummary {
    pub index: u32,
    pub provider: String,
    pub providers: Vec<String>,
    pub confidence: f64,
    pub audio_matched: bool,
    pub artist: Option<String>,
    /// The title with the mix in brackets, as a file carries it.
    pub title: Option<String>,
    pub mix: Option<String>,
    pub album: Option<String>,
    pub label: Option<String>,
    pub catno: Option<String>,
    pub year: Option<u16>,
    pub genre: Option<String>,
    pub subgenre: Option<String>,
    pub track_number: Option<String>,
    #[ts(type = "number | null")]
    pub duration_delta_ms: Option<u64>,
    pub url: Option<String>,
    pub has_artwork: bool,
    pub artwork_provider: Option<String>,
    pub artwork_hash: Option<String>,
    pub evidence: MatchEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CandidateArtworkState {
    pub index: u32,
    pub has_artwork: bool,
    pub artwork_provider: Option<String>,
    pub artwork_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TrackCandidateArtwork {
    #[ts(type = "number")]
    pub track_id: i64,
    pub candidates: Vec<CandidateArtworkState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IdentificationSettings {
    pub verify_audio_previews: bool,
}

/// How Prep groups tracks that belong to the same release or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrepReleaseGroup {
    pub key: String,
    pub title: String,
    pub subtitle: Option<String>,
}

/// One track in the Prep queue, with its 13-field staging diff and artwork ready to render.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PrepItem {
    pub track: LibraryTrack,
    /// The tags a DJ notices that the track has no value for yet.
    pub missing: Vec<TagName>,
    pub review_token: String,
    pub fields: Vec<StagedField>,
    pub artwork: TrackArtwork,
    pub pending_fields: u32,
    pub provider: Option<String>,
    pub confidence: f64,
    pub audio_matched: bool,
    pub needs_url: bool,
    /// Confidences of every pending unlocked changing field on this track, for live threshold
    /// previews.
    pub changeable_confidences: Vec<f64>,
    pub release: Option<PrepReleaseGroup>,
    pub evidence: Option<MatchEvidence>,
    pub candidates: Vec<MatchCandidateSummary>,
    pub selected_candidate: u32,
    pub candidates_expanded: bool,
    pub steps: Vec<IdentifyStepReport>,
    pub has_unwritten: bool,
    pub online_checked: bool,
    pub stage: PrepStageName,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PrepOrderSnapshot {
    #[ts(type = "Array<number>")]
    pub ids: Vec<i64>,
    /// How many of the view's tracks sit in each stage.
    pub stages: PrepStageCounts,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PrepQueueSnapshot {
    pub items: Vec<PrepItem>,
    pub unwritten_tracks: u32,
    pub last_operation: Option<JournalOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PrepStageName {
    Review,
    NotSearched,
    NoMatch,
    Done,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrepStageCounts {
    pub review: u32,
    pub not_searched: u32,
    pub no_match: u32,
    pub done: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TextCaseName {
    Title,
    Sentence,
    Upper,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum BatchTagTool {
    SetFields {
        edits: Vec<UserTagEdit>,
    },
    FindAndReplace {
        #[serde(default)]
        fields: Vec<TagName>,
        find: String,
        replace: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    FixCase {
        #[serde(default)]
        fields: Vec<TagName>,
        text_case: TextCaseName,
    },
    ApplyReleaseFields {
        #[ts(type = "number")]
        source_track_id: i64,
        #[serde(default)]
        fields: Vec<TagName>,
    },
}
