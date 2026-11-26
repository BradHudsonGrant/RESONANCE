pub mod artwork;
mod auto_preview;
mod collections;
pub mod command;
mod copies;
pub mod data_folder;
mod delete;
pub mod error;
mod external;
mod filters;
mod identity;
mod jobs;
pub mod mix_frame;
mod online;
mod prep;
mod providers;
mod quality;
mod relink;
mod renames;
mod replace;
pub mod serve;
mod sources;
mod staging;
mod table_layout;

pub use table_layout::default_table_layout;
mod tag_writes;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::Duration;

use command::{
    AppInfo, FavouriteState, IdentifyReport, LibraryFacets, LibraryTrack, MixDeck, MixDeckState,
    MixDevice, MixState, PreviewState, Request, Response, ScanReport, TrackAudio, TrackCollection,
    TrackFilterPill, TrackKey, TrackPage, TrackQuery,
};
use error::ApiError;
use identity::{run_identify_pass, to_command_report};
use jobs::{enqueue_identify_if_needed, execute_job, scan_source_job};
use resonance_core::key::Key;
use resonance_core::library::{ScannedFile, TagField};
use resonance_core::search::{FilterPill as CoreFilterPill, TrackQuery as CoreTrackQuery};
use resonance_core::staging::effective_tags;
use resonance_db::{Database, TrackRow};
use resonance_engine::{DeckId, Mix, MixStatus, PreviewStatus};
use resonance_identify::credentials::CredentialStore;
use resonance_identify::http::{HttpClient, NetworkClient};
use resonance_identify::pipeline::Pipeline;
use resonance_jobs::JobRunner;
use resonance_library::watch::SourceWatcher;
use sources::reconcile_watchers_for;
use tokio::sync::watch;

pub use online::LibraryAudioFingerprinter;

pub(crate) const REPORTED_UNREADABLE_FILES: usize = 100;

#[derive(Clone)]
struct PreviewTrackMeta {
    generation: u32,
    path: PathBuf,
    track_id: i64,
    title: String,
    artist: Option<String>,
    artwork: Option<String>,
    db_duration_ms: Option<u64>,
}

pub(crate) struct LibraryCore {
    pub(crate) library: Arc<Mutex<Database>>,
    pub(crate) identify_pass: Mutex<()>,
    pub(crate) local_pass_dirty: AtomicBool,
    pub(crate) own_files: Mutex<Vec<PathBuf>>,
    pub(crate) watch_revisions: watch::Sender<u64>,
    pub(crate) scan_outcomes: Mutex<HashMap<i64, Result<ScanReport, ApiError>>>,
    pub(crate) jobs: OnceLock<Weak<JobRunner>>,
    /// Rebuilt when the credential store or HTTP client changes.
    pub(crate) pipeline: Mutex<Arc<Pipeline>>,
}

/// Everything the commands act on. The Tauri shell and the local socket each hold one.
pub struct Api {
    pub(crate) watcher: Arc<SourceWatcher>,
    pub(crate) jobs: Arc<JobRunner>,
    mix: Arc<Mix>,
    pub(crate) core: Arc<LibraryCore>,
    preview_meta: Mutex<Option<PreviewTrackMeta>>,
    credentials: Arc<dyn CredentialStore>,
    http: Arc<dyn HttpClient>,
}

impl Api {
    pub fn new(library: Database) -> Self {
        Self::build(library, None, false)
    }

    pub fn with_watch_quiet_window(library: Database, quiet_window: Duration) -> Self {
        Self::build(library, Some(quiet_window), false)
    }

    #[doc(hidden)]
    pub fn new_silent(library: Database) -> Self {
        Self::build(library, None, true)
    }

    fn build(mut library: Database, quiet_window: Option<Duration>, silent: bool) -> Self {
        // Before the watcher or any job can touch a file. A problem is noted on the operation
        // itself, and a database that cannot be read shows up on the first request.
        let _ = resonance_library::journal::recover(&mut library);
        let (watch_revisions, _) = watch::channel(0u64);
        let database = Arc::new(Mutex::new(library));
        let credentials = providers::default_credential_store();
        let http: Arc<dyn HttpClient> = Arc::new(NetworkClient::new());
        let core = Arc::new(LibraryCore {
            library: Arc::clone(&database),
            identify_pass: Mutex::new(()),
            local_pass_dirty: AtomicBool::new(true),
            own_files: Mutex::new(std::env::current_exe().into_iter().collect()),
            watch_revisions,
            scan_outcomes: Mutex::new(HashMap::new()),
            jobs: OnceLock::new(),
            pipeline: Mutex::new(online::pipeline(&http, &database, &credentials)),
        });
        let mix = Arc::new(if silent {
            Mix::new_silent()
        } else {
            Mix::new()
        });

        let watcher_core = Arc::downgrade(&core);
        let callback = move |folder: &Path| {
            let Some(core) = watcher_core.upgrade() else {
                return;
            };
            let Some(jobs) = core.jobs.get().and_then(Weak::upgrade) else {
                return;
            };
            if let Ok(new_job) = scan_source_job(folder) {
                let _ = jobs.enqueue_coalesced(&new_job);
            }
        };
        let watcher = Arc::new(match quiet_window {
            Some(window) => SourceWatcher::with_quiet_window(window, callback),
            None => SourceWatcher::new(callback),
        });

        let job_core = Arc::downgrade(&core);
        let job_watcher = Arc::downgrade(&watcher);
        let mix_for_probe = Arc::downgrade(&mix);
        let runner = JobRunner::start_with_mix_probe(
            database,
            move || {
                mix_for_probe
                    .upgrade()
                    .is_some_and(|mix| mix.any_deck_playing())
            },
            move |exec| execute_job(exec, &job_core, &job_watcher),
        );
        let jobs = match runner {
            Ok(runner) => Arc::new(runner),
            Err(_) => {
                // `recover_interrupted_jobs` only fails if the database itself is unreadable;
                // fall back to an in-memory runner handle so `Api::new` stays infallible and
                // subsequent commands report the underlying database error cleanly.
                let fallback_db = Arc::new(Mutex::new(
                    Database::open_in_memory()
                        .unwrap_or_else(|_| unreachable!("in-memory SQLite database opens")),
                ));
                Arc::new(
                    JobRunner::start(fallback_db, |_| resonance_jobs::JobOutcome::Interrupted)
                        .unwrap_or_else(|_| unreachable!("in-memory runner starts")),
                )
            }
        };
        let _ = core.jobs.set(Arc::downgrade(&jobs));

        let api = Self {
            watcher,
            jobs,
            mix,
            core,
            preview_meta: Mutex::new(None),
            credentials,
            http,
        };
        {
            let guard = api
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            api.reconcile_watchers(&guard);
        }
        enqueue_identify_if_needed(&api.core, &api.jobs);
        api
    }

    /// The decks, for a transport that streams their telemetry and waveforms.
    pub fn mix(&self) -> &Mix {
        &self.mix
    }

    /// Subscribes to library revisions bumped when a background source watcher or chunked scan
    /// commits new or modified files.
    pub fn watch_revisions(&self) -> watch::Receiver<u64> {
        let mut receiver = self.core.watch_revisions.subscribe();
        receiver.mark_unchanged();
        receiver
    }

    /// Subscribes to Activity job queue revisions bumped when jobs start, advance or finish.
    pub fn activity_revisions(&self) -> watch::Receiver<u64> {
        self.jobs.revisions()
    }

    pub(crate) fn own_files_snapshot(&self) -> Vec<PathBuf> {
        self.core
            .own_files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Keeps provider keys somewhere other than the OS credential store, for tests.
    pub fn with_credential_store(mut self, store: Arc<dyn CredentialStore>) -> Self {
        self.credentials = store;
        self.rebuild_pipeline();
        self
    }

    /// Answers provider requests from somewhere other than the network, for tests.
    pub fn with_http_client(mut self, http: Arc<dyn HttpClient>) -> Self {
        self.http = http;
        self.rebuild_pipeline();
        self
    }

    fn rebuild_pipeline(&self) {
        let rebuilt = online::pipeline(&self.http, &self.core.library, &self.credentials);
        *self
            .core
            .pipeline
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = rebuilt;
    }

    /// Keeps a source scan out of the folder holding the app's data.
    pub fn with_data_folder(self, folder: PathBuf) -> Self {
        {
            let mut own = self
                .core
                .own_files
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            own.push(folder);
        }
        {
            let guard = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            self.reconcile_watchers(&guard);
        }
        self
    }

    pub fn handle(&self, request: Request) -> Result<Response, ApiError> {
        match request {
            Request::AppInfo => Ok(Response::AppInfo(AppInfo {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            })),
            Request::Tracks {
                offset,
                limit,
                query,
            } => self.tracks(offset, limit, query).map(Response::Tracks),
            Request::SetTrackFavourite {
                track_id,
                favourite,
            } => self
                .set_track_favourite(track_id, favourite)
                .map(Response::SetTrackFavourite),
            Request::Collections => self.collections().map(Response::Collections),
            Request::CreateCollection { name } => self
                .create_collection(&name)
                .map(Response::CreateCollection),
            Request::RenameCollection {
                collection_id,
                name,
            } => self
                .rename_collection(collection_id, &name)
                .map(Response::RenameCollection),
            Request::DeleteCollection { collection_id } => self
                .delete_collection(collection_id)
                .map(Response::DeleteCollection),
            Request::AddTracksToCollection {
                collection_id,
                track_ids,
            } => self
                .add_tracks_to_collection(collection_id, &track_ids)
                .map(Response::AddTracksToCollection),
            Request::RemoveTracksFromCollection {
                collection_id,
                track_ids,
            } => self
                .remove_tracks_from_collection(collection_id, &track_ids)
                .map(Response::RemoveTracksFromCollection),
            Request::ImportPlaylist { path } => {
                self.import_playlist(&path).map(Response::ImportPlaylist)
            }
            Request::ReadPlaylistAgain { collection_id } => self
                .read_playlist_again(collection_id)
                .map(Response::ReadPlaylistAgain),
            Request::FindPlaylists => self.find_playlists().map(Response::FindPlaylists),
            Request::ExportCollection {
                collection_id,
                path,
            } => self
                .export_collection(collection_id, &path)
                .map(Response::ExportCollection),
            Request::ExternalChanges => self.external_changes().map(Response::ExternalChanges),
            Request::ApplyExternalChanges { library_id } => self
                .apply_external_changes(library_id)
                .map(Response::ApplyExternalChanges),
            Request::LibraryFacets => self.library_facets().map(Response::LibraryFacets),
            Request::FilterFields => Ok(Response::FilterFields(self.filter_fields())),
            Request::FilterSummaries { query } => {
                self.filter_summaries(query).map(Response::FilterSummaries)
            }
            Request::ParseFilter { text } => self.parse_filter(&text).map(Response::ParseFilter),
            Request::RecordFilterUse { term } => {
                self.record_filter_use(term).map(Response::RecordFilterUse)
            }
            Request::QuickFilters => self.quick_filters().map(Response::QuickFilters),
            Request::ChangeQuickFilters { change } => self
                .change_quick_filters(change)
                .map(Response::ChangeQuickFilters),
            Request::SmartViews => self.smart_views().map(Response::SmartViews),
            Request::SaveSmartView { name, query } => self
                .save_smart_view(&name, query)
                .map(Response::SaveSmartView),
            Request::RenameSmartView { view_id, name } => self
                .rename_smart_view(view_id, &name)
                .map(Response::RenameSmartView),
            Request::DeleteSmartView { view_id } => self
                .delete_smart_view(view_id)
                .map(Response::DeleteSmartView),
            Request::TableLayout => self.table_layout().map(Response::TableLayout),
            Request::ChangeTableLayout { change } => self
                .change_table_layout(change)
                .map(Response::ChangeTableLayout),
            Request::Scan { folder } => self.scan(Path::new(&folder)).map(Response::Scan),
            Request::Identify => self.identify().map(Response::Identify),
            Request::IdentifyOnline {
                track_ids,
                only_selected,
            } => self
                .identify_online(&track_ids, only_selected)
                .map(Response::IdentifyOnline),
            Request::IdentifyWithUrl { track_id, url } => self
                .identify_with_url(track_id, &url)
                .map(Response::IdentifyWithUrl),
            Request::JobsSnapshot => self.jobs_snapshot().map(Response::JobsSnapshot),
            Request::CancelJob { job_id } => self.cancel_job(job_id).map(Response::CancelJob),
            Request::PauseJob { job_id } => self.pause_job(job_id).map(Response::PauseJob),
            Request::ResumeJob { job_id } => self.resume_job(job_id).map(Response::ResumeJob),
            Request::RetryJob { job_id } => self.retry_job(job_id).map(Response::RetryJob),
            Request::ClearFinishedJobs => {
                self.clear_finished_jobs().map(Response::ClearFinishedJobs)
            }
            Request::Sources => self.sources().map(Response::Sources),
            Request::SourceSubfolders { source_id, parent } => self
                .source_subfolders(source_id, parent.as_deref())
                .map(Response::SourceSubfolders),
            Request::UpdateSource {
                source_id,
                include_subfolders,
                watch,
                excluded_subfolders,
            } => self
                .update_source(source_id, include_subfolders, watch, &excluded_subfolders)
                .map(Response::UpdateSource),
            Request::RemoveSource { source_id } => {
                self.remove_source(source_id).map(Response::RemoveSource)
            }
            Request::SetIgnoredFolders { folders } => self
                .set_ignored_folders(&folders)
                .map(Response::SetIgnoredFolders),
            Request::MixLoad { deck, path } => {
                let status = self
                    .mix
                    .load(deck_id(deck), Path::new(&path))
                    .map_err(ApiError::audio_failed)?;
                self.jobs.set_mix_live(self.mix.any_deck_playing());
                Ok(Response::MixLoad(mix_state(status)))
            }
            Request::MixPlay { deck, playing } => {
                let status = self
                    .mix
                    .play(deck_id(deck), playing)
                    .map_err(ApiError::audio_failed)?;
                self.jobs.set_mix_live(self.mix.any_deck_playing());
                Ok(Response::MixPlay(mix_state(status)))
            }
            Request::MixStatus => Ok(Response::MixStatus(mix_state(self.mix.status()))),
            Request::PreviewLoadAndPlay { track_id, start_ms } => self
                .preview_load_and_play(track_id, start_ms)
                .map(Response::PreviewLoadAndPlay),
            Request::PreviewTogglePlay => self
                .mix
                .preview_toggle_play()
                .map(|status| Response::PreviewTogglePlay(self.preview_state(status)))
                .map_err(ApiError::audio_failed),
            Request::PreviewPause => self
                .mix
                .preview_pause()
                .map(|status| Response::PreviewPause(self.preview_state(status)))
                .map_err(ApiError::audio_failed),
            Request::PreviewSeek { position_ms } => self
                .mix
                .preview_seek(position_ms)
                .map(|status| Response::PreviewSeek(self.preview_state(status)))
                .map_err(ApiError::audio_failed),
            Request::PreviewSetVolume { volume } => self
                .mix
                .preview_set_volume(volume)
                .map(|status| Response::PreviewSetVolume(self.preview_state(status)))
                .map_err(ApiError::audio_failed),
            Request::PreviewStatus => Ok(Response::PreviewStatus(
                self.preview_state(self.mix.preview_status()),
            )),
            Request::PreviewFocus { track_id } => {
                self.preview_focus(track_id).map(Response::PreviewFocus)
            }
            Request::PreviewSettings => self.preview_settings().map(Response::PreviewSettings),
            Request::SetPreviewSettings { settings } => self
                .set_preview_settings(settings)
                .map(Response::SetPreviewSettings),
            Request::RelinkCandidates {
                track_id,
                search_folder,
            } => self
                .relink_candidates(track_id, search_folder.as_deref())
                .map(Response::RelinkCandidates),
            Request::PreviewRelink {
                track_id,
                candidate_path,
            } => self
                .preview_relink(track_id, &candidate_path)
                .map(Box::new)
                .map(Response::PreviewRelink),
            Request::ApplyRelink {
                track_id,
                candidate_path,
                force,
            } => self
                .apply_relink(track_id, &candidate_path, force)
                .map(Box::new)
                .map(Response::ApplyRelink),
            Request::ProviderKeys => self.provider_keys().map(Response::ProviderKeys),
            Request::SetProviderKey { provider, value } => self
                .set_provider_key(provider, &value)
                .map(Response::SetProviderKey),
            Request::ClearProviderKey { provider } => self
                .clear_provider_key(provider)
                .map(Response::ClearProviderKey),
            Request::StagingQueue { offset, limit } => self
                .staging_queue(offset, limit)
                .map(Response::StagingQueue),
            Request::StagingDiff { track_id } => {
                self.staging_diff(track_id).map(Response::StagingDiff)
            }
            Request::AcceptProposedFields { track_id, fields } => self
                .accept_proposed_fields(track_id, &fields)
                .map(Response::AcceptProposedFields),
            Request::AcceptAllAboveThreshold { min_confidence } => self
                .accept_all_above_threshold(min_confidence)
                .map(Response::AcceptAllAboveThreshold),
            Request::SkipProposal { track_id, fields } => self
                .skip_proposal(track_id, &fields)
                .map(Response::SkipProposal),
            Request::TrackCopies { track_id } => {
                self.track_copies(track_id).map(Response::TrackCopies)
            }
            Request::DeleteTracks {
                track_ids,
                for_good,
            } => self
                .delete_tracks(&track_ids, for_good)
                .map(Response::DeleteTracks),
            Request::CompareReplacement {
                track_id,
                path,
                template,
            } => self
                .compare_replacement(track_id, &path, template.as_deref())
                .map(|comparison| Response::CompareReplacement(Box::new(comparison))),
            Request::ApplyReplacement {
                track_id,
                path,
                template,
                choice,
            } => self
                .apply_replacement(track_id, &path, template.as_deref(), choice)
                .map(Response::ApplyReplacement),
            Request::FfmpegStatus => Ok(Response::FfmpegStatus(self.ffmpeg_status())),
            Request::ConvertTrack { track_id, preset } => self
                .convert_track(track_id, preset)
                .map(Response::ConvertTrack),
            Request::RepairTrack { track_id } => {
                self.repair_track(track_id).map(Response::RepairTrack)
            }
            Request::KeepCopy { track_id, asset_id } => {
                self.keep_copy(track_id, asset_id).map(Response::KeepCopy)
            }
            Request::TrackQuality { track_id } => {
                self.track_quality(track_id).map(Response::TrackQuality)
            }
            Request::TrackArtwork { track_id } => {
                self.track_artwork(track_id).map(Response::TrackArtwork)
            }
            Request::AcceptProposedArtwork { track_id } => self
                .accept_proposed_artwork(track_id)
                .map(Response::AcceptProposedArtwork),
            Request::SkipProposedArtwork { track_id } => self
                .skip_proposed_artwork(track_id)
                .map(Response::SkipProposedArtwork),
            Request::SetTrackArtwork {
                track_id,
                image_base64,
            } => self
                .set_track_artwork(track_id, &image_base64)
                .map(Response::SetTrackArtwork),
            Request::SetUserTag { track_ids, edits } => self
                .set_user_tag(&track_ids, &edits)
                .map(Response::SetUserTag),
            Request::PreviewTagWrites { track_ids } => self
                .preview_tag_writes(&track_ids)
                .map(Response::PreviewTagWrites),
            Request::ApplyTagWrites { track_ids } => self
                .apply_tag_writes(&track_ids)
                .map(Response::ApplyTagWrites),
            Request::AcceptAndWrite {
                track_ids,
                fields,
                include_artwork,
                expected_review,
            } => self
                .accept_and_write(
                    &track_ids,
                    fields.as_deref(),
                    include_artwork,
                    expected_review.as_deref(),
                )
                .map(Response::AcceptAndWrite),
            Request::RecentOperations { limit } => self
                .recent_operations(limit)
                .map(Response::RecentOperations),
            Request::UndoOperation { operation_id } => self
                .undo_operation(operation_id)
                .map(Response::UndoOperation),
            Request::PreviewRename {
                scope,
                template,
                limit,
            } => self
                .preview_rename(scope, &template, limit)
                .map(Response::PreviewRename),
            Request::ApplyRename { scope, template } => self
                .apply_rename(scope, &template)
                .map(Response::ApplyRename),
            Request::PrepTrackIds {
                query,
                track_ids,
                stage,
            } => self
                .prep_track_ids(query, track_ids, stage)
                .map(Response::PrepTrackIds),
            Request::PrepOrder {
                query,
                track_ids,
                by_stage,
            } => self
                .prep_order(query, track_ids, by_stage)
                .map(Response::PrepOrder),
            Request::PrepQueue {
                query,
                proposals_only,
                track_ids,
            } => self
                .prep_queue(query, proposals_only, track_ids)
                .map(Response::PrepQueue),
            Request::AcceptPrepItems {
                track_ids,
                fields,
                candidate_index,
                skip_remaining,
                include_artwork,
            } => self
                .accept_prep_items(
                    &track_ids,
                    &fields,
                    candidate_index,
                    skip_remaining,
                    include_artwork,
                )
                .map(Response::AcceptPrepItems),
            Request::SelectPrepCandidate {
                track_id,
                candidate_index,
                expected_review,
            } => self
                .select_prep_candidate(track_id, candidate_index, expected_review.as_deref())
                .map(Box::new)
                .map(Response::SelectPrepCandidate),
            Request::UsePrepCandidateArtwork {
                track_id,
                candidate_index,
                accept,
                expected_review,
            } => self
                .use_prep_candidate_artwork(
                    track_id,
                    candidate_index,
                    accept,
                    expected_review.as_deref(),
                )
                .map(Box::new)
                .map(Response::UsePrepCandidateArtwork),
            Request::FetchPrepCandidateArtwork {
                track_id,
                candidate_index,
            } => self
                .fetch_prep_candidate_artwork(track_id, candidate_index)
                .map(Response::FetchPrepCandidateArtwork),
            Request::SkipPrepArtwork { track_id } => self
                .skip_prep_artwork(track_id)
                .map(Box::new)
                .map(Response::SkipPrepArtwork),
            Request::SkipPrepItems { track_ids, fields } => self
                .skip_prep_items(&track_ids, &fields)
                .map(Response::SkipPrepItems),
            Request::PreviewBatchTag { track_ids, tool } => self
                .preview_batch_tag(&track_ids, tool)
                .map(Response::PreviewBatchTag),
            Request::ApplyBatchTag { track_ids, tool } => self
                .apply_batch_tag(&track_ids, tool)
                .map(Response::ApplyBatchTag),
            Request::IdentificationSettings => self
                .identification_settings()
                .map(Response::IdentificationSettings),
            Request::SetIdentificationSettings { settings } => self
                .set_identification_settings(settings)
                .map(Response::SetIdentificationSettings),
        }
    }

    fn tracks(
        &self,
        offset: u32,
        limit: u32,
        query: Option<TrackQuery>,
    ) -> Result<TrackPage, ApiError> {
        let core_query = core_track_query(query)?;
        // A panic while the lock was held leaves nothing half-written: reads hold it, and a
        // write is one transaction that SQLite rolls back.
        let library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let library_total = library
            .track_count()
            .map_err(ApiError::library_unreadable)?;
        let (rows, total) = if core_query.is_narrowed() {
            library
                .query_tracks_and_total(&core_query, offset, limit)
                .map_err(ApiError::library_unreadable)?
        } else {
            let rows = library
                .query_tracks_page(&core_query, offset, limit)
                .map_err(ApiError::library_unreadable)?;
            (rows, library_total)
        };
        Ok(TrackPage {
            total,
            library_total,
            offset,
            tracks: rows.iter().map(library_track).collect(),
        })
    }

    fn set_track_favourite(
        &self,
        track_id: i64,
        favourite: bool,
    ) -> Result<FavouriteState, ApiError> {
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let found = library
            .set_track_favourite(track_id, favourite)
            .map_err(ApiError::library_unreadable)?;
        if !found {
            return Err(ApiError::bad_request(format!(
                "Track {track_id} is no longer in the library."
            )));
        }
        Ok(FavouriteState {
            track_id,
            favourite,
        })
    }

    fn library_facets(&self) -> Result<LibraryFacets, ApiError> {
        let library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let facets = library
            .library_facets()
            .map_err(ApiError::library_unreadable)?;
        Ok(LibraryFacets {
            labels: facets.labels,
            bpm_min: facets.bpm_min,
            bpm_max: facets.bpm_max,
            year_min: facets.year_min,
            year_max: facets.year_max,
            missing_count: facets.missing_count,
        })
    }

    fn scan(&self, folder: &Path) -> Result<ScanReport, ApiError> {
        self.run_scan_job(folder)
    }

    fn identify(&self) -> Result<IdentifyReport, ApiError> {
        let report = run_identify_pass(&self.core, None)?;
        if report.reconciled_moves > 0 {
            self.core
                .watch_revisions
                .send_modify(|revision| *revision += 1);
        }
        Ok(to_command_report(report))
    }

    fn preview_load_and_play(
        &self,
        track_id: i64,
        start_ms: Option<u64>,
    ) -> Result<PreviewState, ApiError> {
        let row = {
            let library = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            library
                .track_by_id(track_id)
                .map_err(ApiError::library_unreadable)?
        };
        let Some(row) = row else {
            return Err(ApiError::bad_request(format!(
                "Track {track_id} is no longer in the library."
            )));
        };
        self.preview_play_row(track_id, &row, start_ms.unwrap_or(0))
    }

    fn preview_play_row(
        &self,
        track_id: i64,
        row: &TrackRow,
        start_ms: u64,
    ) -> Result<PreviewState, ApiError> {
        let path = resonance_library::identity::full_asset_path(&row.source_folder, &row.file.path);
        let shown = shown_file(row);
        let title = shown.title().to_owned();
        let artist = shown.tag(TagField::Artist).map(str::to_owned);
        let artwork = row.artwork.clone();
        let db_duration_ms = row.file.audio.map(|audio| audio.duration_ms);
        let status = self
            .mix
            .preview_load_and_play(&path, start_ms, db_duration_ms)
            .map_err(ApiError::audio_failed)?;
        let mut meta = self
            .preview_meta
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let should_store = meta
            .as_ref()
            .is_none_or(|existing| status.generation >= existing.generation);
        if should_store {
            *meta = Some(PreviewTrackMeta {
                generation: status.generation,
                path,
                track_id,
                title,
                artist,
                artwork,
                db_duration_ms,
            });
        }
        Ok(preview_state_from(&meta, status))
    }

    fn preview_state(&self, status: PreviewStatus) -> PreviewState {
        let meta = self
            .preview_meta
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        preview_state_from(&meta, status)
    }
}

fn preview_state_from(meta: &Option<PreviewTrackMeta>, status: PreviewStatus) -> PreviewState {
    let matched = status.path.as_deref().and_then(|loaded_path| {
        meta.as_ref()
            .filter(|entry| entry.path.as_path() == loaded_path)
    });
    let duration_ms = if status.duration_ms > 0 {
        status.duration_ms
    } else {
        matched.and_then(|entry| entry.db_duration_ms).unwrap_or(0)
    };
    PreviewState {
        track_id: matched.map(|entry| entry.track_id),
        title: matched.map(|entry| entry.title.clone()),
        artist: matched.and_then(|entry| entry.artist.clone()),
        artwork: matched.and_then(|entry| entry.artwork.clone()),
        playing: status.playing,
        position_ms: status.position_ms,
        duration_ms,
        volume: status.volume,
        generation: status.generation,
    }
}

fn deck_id(deck: MixDeck) -> DeckId {
    match deck {
        MixDeck::A => DeckId::A,
        MixDeck::B => DeckId::B,
    }
}

fn mix_state(status: MixStatus) -> MixState {
    MixState {
        device: status.device.map(|device| MixDevice {
            name: device.name,
            sample_rate: device.sample_rate,
            channels: device.channels,
            requested_buffer_frames: device.requested_buffer_frames,
            period_frames: device.period_frames,
        }),
        decks: status
            .decks
            .into_iter()
            .map(|deck| MixDeckState {
                deck: match deck.deck {
                    DeckId::A => MixDeck::A,
                    DeckId::B => MixDeck::B,
                },
                generation: deck.generation,
                path: deck.path.map(|path| path.to_string_lossy().into_owned()),
                duration_seconds: deck.duration_seconds,
                playing: deck.playing,
            })
            .collect(),
    }
}

/// The query the core runs. Filters that do not suit their columns are refused.
pub(crate) fn core_track_query(query: Option<TrackQuery>) -> Result<CoreTrackQuery, ApiError> {
    let Some(query) = query else {
        return Ok(CoreTrackQuery::default());
    };
    Ok(CoreTrackQuery {
        text: query.text,
        pill: match query.pill.unwrap_or_default() {
            TrackFilterPill::All => CoreFilterPill::All,
            TrackFilterPill::Favourites => CoreFilterPill::Favourites,
            TrackFilterPill::Recent => CoreFilterPill::Recent,
        },
        filters: filters::core_terms(query.filters.unwrap_or_default())?,
        collection_id: query.collection_id,
        missing_only: query.missing_only.unwrap_or(false),
        sort: table_layout::core_sort_field(query.sort.unwrap_or_default()),
        direction: table_layout::core_direction(query.direction.unwrap_or_default()),
        then_sort: query.then_sort.map(|field| {
            (
                table_layout::core_sort_field(field),
                table_layout::core_direction(query.then_direction.unwrap_or_default()),
            )
        }),
    })
}

/// The track's file with the user's values in place of the file's tags.
fn shown_file(row: &TrackRow) -> ScannedFile {
    let mut file = row.file.clone();
    if !row.user_tags.is_empty() {
        file.tags = effective_tags(&row.file.tags, &row.user_tags);
    }
    file
}

pub fn track_key(key: Key) -> TrackKey {
    TrackKey {
        camelot: key.camelot(),
        open_key: key.open_key(),
        name: key.name().to_owned(),
    }
}

pub(crate) fn library_track(row: &TrackRow) -> LibraryTrack {
    let shown = shown_file(row);
    let file = &shown;
    let tag = |field| file.tag(field).map(str::to_owned);
    LibraryTrack {
        id: row.track_id,
        favourite: row.favourite,
        title: file.title().to_owned(),
        artist: tag(TagField::Artist),
        album: tag(TagField::Album),
        album_artist: tag(TagField::AlbumArtist),
        genre: tag(TagField::Genre),
        year: tag(TagField::Year),
        track_number: tag(TagField::TrackNumber),
        label: tag(TagField::Label),
        comment: tag(TagField::Comment),
        mix: tag(TagField::Mix),
        subgenre: tag(TagField::Subgenre),
        catno: tag(TagField::CatNo),
        composer: tag(TagField::Composer),
        remixer: tag(TagField::Remixer),
        bpm: file.bpm(),
        key: file.key().map(track_key),

        key_tag: tag(TagField::Key),
        format: file.format.name().to_owned(),
        location: file
            .location(Path::new(&row.source_folder))
            .to_string_lossy()
            .into_owned(),
        size_bytes: file.size_bytes,
        audio: file.audio.map(|audio| TrackAudio {
            duration_ms: audio.duration_ms,
            sample_rate: audio.sample_rate,
            channels: audio.channels,
            bit_depth: audio.bit_depth,
            bitrate_kbps: audio.bitrate_kbps,
        }),
        collections: row
            .collections
            .iter()
            .map(|collection| TrackCollection {
                id: collection.id,
                name: collection.name.clone(),
            })
            .collect(),
        missing_since: row.missing_since,
        artwork: row.artwork.clone(),
        quality: row.quality.as_ref().map(quality::quality_flag),
        added_ms: row.added_ns / 1_000_000,
        modified_ms: file.modified_ns / 1_000_000,
    }
}

#[cfg(test)]
mod tests {
    use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile};

    use crate::command::{SortDirection, TrackSortField};

    use super::*;

    fn api_with(files: &[ScannedFile]) -> Api {
        let mut database = Database::open_in_memory().unwrap();
        let source = database.source("music").unwrap();
        database.record_scan(source, files).unwrap();
        Api::new(database)
    }

    fn file(path: &str, tags: &[(TagField, &str)]) -> ScannedFile {
        ScannedFile {
            path: path.to_owned(),
            size_bytes: 9_000_000,
            modified_ns: 0,
            format: AudioFormat::Flac,
            audio: Some(AudioFacts {
                duration_ms: 301_000,
                sample_rate: Some(44_100),
                channels: Some(2),
                bit_depth: Some(16),
                bitrate_kbps: Some(900),
            }),
            tags: tags
                .iter()
                .map(|(field, value)| (*field, (*value).to_owned()))
                .collect(),
        }
    }

    fn page(api: &Api, offset: u32, limit: u32) -> TrackPage {
        match api
            .handle(Request::Tracks {
                offset,
                limit,
                query: None,
            })
            .unwrap()
        {
            Response::Tracks(page) => page,
            other => panic!("expected tracks, got {other:?}"),
        }
    }

    #[test]
    fn app_info_reports_the_version_of_the_core() {
        let response = Api::new(Database::open_in_memory().unwrap())
            .handle(Request::AppInfo)
            .unwrap();

        assert_eq!(
            response,
            Response::AppInfo(AppInfo {
                version: env!("CARGO_PKG_VERSION").to_owned()
            })
        );
    }

    #[test]
    fn tracks_gives_the_page_asked_for_and_the_size_of_the_library() {
        let api = api_with(&[
            file("a.flac", &[]),
            file("b.flac", &[]),
            file("c.flac", &[]),
        ]);

        let middle = page(&api, 1, 1);

        assert_eq!(middle.total, 3);
        assert_eq!(middle.offset, 1);
        assert_eq!(middle.tracks.len(), 1);
        assert_eq!(middle.tracks[0].title, "b");
        assert!(page(&api, 3, 10).tracks.is_empty());
    }

    #[test]
    fn a_listed_track_carries_its_tags_tempo_key_and_location() {
        let api = api_with(&[file(
            "House/blue.flac",
            &[
                (TagField::Title, "Blue Hour"),
                (TagField::Artist, "Ada Marsh"),
                (TagField::Bpm, "124,50"),
                (TagField::Key, "F#m"),
            ],
        )]);

        let track = &page(&api, 0, 10).tracks[0];

        assert_eq!(track.title, "Blue Hour");
        assert_eq!(track.artist.as_deref(), Some("Ada Marsh"));
        assert_eq!(track.bpm, Some(124.5));
        assert_eq!(
            track.key,
            Some(TrackKey {
                camelot: "11A".to_owned(),
                open_key: "4m".to_owned(),
                name: "F#m".to_owned()
            })
        );
        assert_eq!(track.key_tag.as_deref(), Some("F#m"));
        assert_eq!(track.format, "flac");
        assert_eq!(
            Path::new(&track.location),
            Path::new("music").join("House").join("blue.flac")
        );
        assert_eq!(
            track.audio.as_ref().map(|audio| audio.duration_ms),
            Some(301_000)
        );
    }

    #[test]
    fn an_empty_library_lists_nothing() {
        let api = api_with(&[]);

        let empty = page(&api, 0, 200);

        assert_eq!(empty.total, 0);
        assert!(empty.tracks.is_empty());
    }

    #[tokio::test]
    async fn renaming_a_watched_file_keeps_its_track_id_and_pushes_a_library_revision() {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp/api_watched_rename");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();

        let original = folder.join("opener.flac");
        resonance_fixtures::signal::Signal::tone_at_loudness(-12.0, 4.0, 44_100)
            .write_flac(&original)
            .unwrap();

        let api = Api::with_watch_quiet_window(
            Database::open_in_memory().unwrap(),
            Duration::from_millis(60),
        );
        let mut revisions = api.watch_revisions();

        api.handle(Request::Scan {
            folder: folder.to_string_lossy().into_owned(),
        })
        .unwrap();
        match api.handle(Request::Identify).unwrap() {
            Response::Identify(report) => assert_eq!(report.skipped_unreadable, 0),
            other => panic!("expected identify report, got {other:?}"),
        }
        assert!(
            api.core
                .library
                .lock()
                .unwrap()
                .has_identified_assets()
                .unwrap()
        );

        let before = page(&api, 0, 10);
        assert_eq!(before.total, 1);
        let original_track_id = before.tracks[0].id;

        let renamed = folder.join("opener-club-edit.flac");
        std::fs::rename(&original, &renamed).unwrap();

        tokio::time::timeout(Duration::from_secs(5), revisions.changed())
            .await
            .expect("watcher fired within timeout")
            .expect("watch channel open");

        let after = page(&api, 0, 10);
        assert_eq!(after.total, 1);
        assert_eq!(after.tracks[0].id, original_track_id);
        assert!(
            after.tracks[0].location.ends_with("opener-club-edit.flac"),
            "unexpected location: {}",
            after.tracks[0].location
        );
    }

    #[test]
    fn tracks_query_filters_sorts_facets_and_favourites_work_through_the_api() {
        let api = api_with(&[
            file(
                "a.flac",
                &[
                    (TagField::Title, "Zebra"),
                    (TagField::Bpm, "128"),
                    (TagField::Key, "Am"),
                    (TagField::Label, "Virgin"),
                    (TagField::Year, "2020"),
                ],
            ),
            file(
                "b.flac",
                &[
                    (TagField::Title, "Alpha"),
                    (TagField::Bpm, "124"),
                    (TagField::Key, "11A"),
                    (TagField::Label, "Anjunadeep"),
                    (TagField::Year, "2023"),
                ],
            ),
        ]);

        let sorted = match api
            .handle(Request::Tracks {
                offset: 0,
                limit: 10,
                query: Some(TrackQuery {
                    sort: Some(TrackSortField::Title),
                    direction: Some(SortDirection::Asc),
                    ..TrackQuery::default()
                }),
            })
            .unwrap()
        {
            Response::Tracks(page) => page,
            other => panic!("expected tracks, got {other:?}"),
        };
        assert_eq!(sorted.library_total, 2);
        assert_eq!(sorted.total, 2);
        assert_eq!(sorted.tracks[0].title, "Alpha");
        assert!(!sorted.tracks[0].favourite);

        let fav = api
            .handle(Request::SetTrackFavourite {
                track_id: sorted.tracks[0].id,
                favourite: true,
            })
            .unwrap();
        assert_eq!(
            fav,
            Response::SetTrackFavourite(FavouriteState {
                track_id: sorted.tracks[0].id,
                favourite: true,
            })
        );

        let filtered = match api
            .handle(Request::Tracks {
                offset: 0,
                limit: 10,
                query: Some(TrackQuery {
                    pill: Some(TrackFilterPill::Favourites),
                    ..filters::filtered(&["key 11A"])
                }),
            })
            .unwrap()
        {
            Response::Tracks(page) => page,
            other => panic!("expected tracks, got {other:?}"),
        };
        assert_eq!(filtered.library_total, 2);
        assert_eq!(filtered.total, 1);
        assert!(filtered.tracks[0].favourite);

        let facets = match api.handle(Request::LibraryFacets).unwrap() {
            Response::LibraryFacets(facets) => facets,
            other => panic!("expected facets, got {other:?}"),
        };
        assert_eq!(facets.labels, vec!["Anjunadeep", "Virgin"]);
        assert_eq!(facets.bpm_min, Some(124.0));
        assert_eq!(facets.bpm_max, Some(128.0));
        assert_eq!(facets.year_min, Some(2020));
        assert_eq!(facets.year_max, Some(2023));
    }

    #[test]
    fn preview_commands_load_seek_toggle_volume_and_pause_for_mix_deck() {
        use resonance_fixtures::signal::Signal;

        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/api-preview");
        std::fs::create_dir_all(&folder).unwrap();
        let wav_path = folder.join("warm_up.wav");
        Signal::tone(440.0, 2.0, -6.0, 44_100)
            .write_wav(&wav_path)
            .unwrap();

        let api = Api::new_silent(Database::open_in_memory().unwrap());
        api.handle(Request::Scan {
            folder: folder.to_string_lossy().into_owned(),
        })
        .unwrap();
        let track = page(&api, 0, 10).tracks.into_iter().next().unwrap();

        let missing = api.handle(Request::PreviewLoadAndPlay {
            track_id: 999_999,
            start_ms: None,
        });
        assert!(missing.is_err());

        let Response::PreviewSetVolume(vol_state) = api
            .handle(Request::PreviewSetVolume { volume: 0.55 })
            .unwrap()
        else {
            panic!("expected PreviewSetVolume");
        };
        assert!((vol_state.volume - 0.55).abs() < 1e-6);

        let Response::PreviewLoadAndPlay(loaded) = api
            .handle(Request::PreviewLoadAndPlay {
                track_id: track.id,
                start_ms: Some(400),
            })
            .unwrap()
        else {
            panic!("expected PreviewLoadAndPlay");
        };
        assert_eq!(loaded.track_id, Some(track.id));
        assert_eq!(loaded.title.as_deref(), Some(track.title.as_str()));
        assert!(loaded.playing);
        assert_eq!(loaded.duration_ms, 2_000);
        assert!(loaded.position_ms >= 390);
        assert!((loaded.volume - 0.55).abs() < 1e-6);

        let Response::PreviewSeek(sought) = api
            .handle(Request::PreviewSeek { position_ms: 1_250 })
            .unwrap()
        else {
            panic!("expected PreviewSeek");
        };
        assert_eq!(sought.track_id, Some(track.id));
        assert!(sought.playing);
        assert!(sought.position_ms >= 1_240);

        let Response::PreviewPause(paused) = api.handle(Request::PreviewPause).unwrap() else {
            panic!("expected PreviewPause");
        };
        assert!(!paused.playing);
        assert_eq!(paused.track_id, Some(track.id));

        let Response::PreviewTogglePlay(resumed) = api.handle(Request::PreviewTogglePlay).unwrap()
        else {
            panic!("expected PreviewTogglePlay");
        };
        assert!(resumed.playing);
        assert_eq!(resumed.track_id, Some(track.id));

        api.handle(Request::MixLoad {
            deck: MixDeck::A,
            path: wav_path.to_string_lossy().into_owned(),
        })
        .unwrap();
        api.handle(Request::MixPlay {
            deck: MixDeck::A,
            playing: true,
        })
        .unwrap();

        let Response::PreviewStatus(after_mix) = api.handle(Request::PreviewStatus).unwrap() else {
            panic!("expected PreviewStatus");
        };
        assert!(!after_mix.playing);
        assert_eq!(after_mix.track_id, Some(track.id));
    }

    #[test]
    fn preview_opens_tracks_inside_folders_ending_with_a_space() {
        use resonance_fixtures::signal::Signal;

        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/api-preview-space");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let canonical_root = std::fs::canonicalize(&root).unwrap();
        let spaced_dir = canonical_root.join("Garage ");
        std::fs::create_dir_all(&spaced_dir).unwrap();
        Signal::tone(330.0, 0.5, -6.0, 44_100)
            .write_wav(&spaced_dir.join("space_cut.wav"))
            .unwrap();

        let api = Api::new_silent(Database::open_in_memory().unwrap());
        api.handle(Request::Scan {
            folder: root.to_string_lossy().into_owned(),
        })
        .unwrap();
        let track = page(&api, 0, 10).tracks.into_iter().next().unwrap();

        let Response::PreviewLoadAndPlay(loaded) = api
            .handle(Request::PreviewLoadAndPlay {
                track_id: track.id,
                start_ms: None,
            })
            .unwrap()
        else {
            panic!("expected PreviewLoadAndPlay");
        };
        assert_eq!(loaded.track_id, Some(track.id));
        assert!(loaded.playing);
    }

    #[test]
    fn relink_commands_surface_missing_tracks_rank_candidates_and_enforce_force_on_mismatch() {
        use command::RelinkMatchEvidence;
        use resonance_fixtures::signal::Signal;

        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/api-missing-relink");
        let _ = std::fs::remove_dir_all(&root);
        let source_dir = root.join("Club");
        let outside_dir = root.join("Outside");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&outside_dir).unwrap();

        let orig = source_dir.join("anthem.wav");
        let moved_transcode = outside_dir.join("anthem.aiff");
        let different = outside_dir.join("other.wav");
        let tone_a = Signal::tone(220.0, 4.0, -6.0, 44_100);
        let tone_b = Signal::tone(523.25, 9.0, -6.0, 44_100);
        tone_a.write_wav(&orig).unwrap();
        tone_a.write_aiff(&moved_transcode).unwrap();
        tone_b.write_wav(&different).unwrap();

        let api = Api::new_silent(Database::open_in_memory().unwrap());
        api.handle(Request::Scan {
            folder: source_dir.to_string_lossy().into_owned(),
        })
        .unwrap();
        api.handle(Request::Identify).unwrap();
        let initial = page(&api, 0, 10).tracks.into_iter().next().unwrap();
        assert_eq!(initial.missing_since, None);

        std::fs::remove_file(&orig).unwrap();
        api.handle(Request::Scan {
            folder: source_dir.to_string_lossy().into_owned(),
        })
        .unwrap();

        let Response::Tracks(missing_page) = api
            .handle(Request::Tracks {
                offset: 0,
                limit: 10,
                query: Some(TrackQuery {
                    missing_only: Some(true),
                    ..TrackQuery::default()
                }),
            })
            .unwrap()
        else {
            panic!("expected Tracks");
        };
        assert_eq!(missing_page.total, 1);
        assert!(missing_page.tracks[0].missing_since.is_some());

        let Response::RelinkCandidates(candidates) = api
            .handle(Request::RelinkCandidates {
                track_id: initial.id,
                search_folder: Some(outside_dir.to_string_lossy().into_owned()),
            })
            .unwrap()
        else {
            panic!("expected RelinkCandidates");
        };
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].evidence, RelinkMatchEvidence::ExactAudio);

        let Response::PreviewRelink(bad_preview) = api
            .handle(Request::PreviewRelink {
                track_id: initial.id,
                candidate_path: different.to_string_lossy().into_owned(),
            })
            .unwrap()
        else {
            panic!("expected PreviewRelink");
        };
        assert!(bad_preview.requires_confirmation);
        assert!(
            api.handle(Request::ApplyRelink {
                track_id: initial.id,
                candidate_path: different.to_string_lossy().into_owned(),
                force: false,
            })
            .is_err()
        );

        let Response::ApplyRelink(relinked) = api
            .handle(Request::ApplyRelink {
                track_id: initial.id,
                candidate_path: moved_transcode.to_string_lossy().into_owned(),
                force: false,
            })
            .unwrap()
        else {
            panic!("expected ApplyRelink");
        };
        assert_eq!(relinked.id, initial.id);
        assert_eq!(relinked.missing_since, None);
        assert_eq!(relinked.format, "aiff");
    }
}
