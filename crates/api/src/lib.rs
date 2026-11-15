pub mod command;
pub mod data_folder;
pub mod error;
mod identity;
pub mod mix_frame;
pub mod serve;
mod sources;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use command::{
    AppInfo, FavouriteState, IdentifyReport, LibraryFacets, LibraryTrack, MixDeck, MixDeckState,
    MixDevice, MixState, Request, Response, ScanReport, SortDirection, TrackAudio, TrackFilterPill,
    TrackFormat, TrackKey, TrackPage, TrackQuery, TrackSortField, UnreadableFile,
};
use error::ApiError;
use identity::{IdentityWorker, run_identify_pass, to_command_report};
use resonance_core::library::{AudioFormat, TagField};
use resonance_core::search::{
    FilterPill as CoreFilterPill, SortDirection as CoreSortDirection, TrackQuery as CoreTrackQuery,
    TrackSortField as CoreTrackSortField,
};
use resonance_db::{Database, TrackRow};
use resonance_engine::{DeckId, Mix, MixStatus};
use resonance_library::watch::SourceWatcher;
use tokio::sync::watch;

const REPORTED_UNREADABLE_FILES: usize = 100;

pub(crate) struct LibraryCore {
    pub(crate) library: Mutex<Database>,
    pub(crate) identify_pass: Mutex<()>,
    pub(crate) own_files: Mutex<Vec<PathBuf>>,
    pub(crate) watch_revisions: watch::Sender<u64>,
}

/// Everything the commands act on. The Tauri shell and the local socket each hold one.
pub struct Api {
    pub(crate) watcher: SourceWatcher,
    pub(crate) identity: Arc<IdentityWorker>,
    pub(crate) core: Arc<LibraryCore>,
    mix: Mix,
}

impl Api {
    pub fn new(library: Database) -> Self {
        Self::build(library, None)
    }

    pub fn with_watch_quiet_window(library: Database, quiet_window: Duration) -> Self {
        Self::build(library, Some(quiet_window))
    }

    fn build(library: Database, quiet_window: Option<Duration>) -> Self {
        let (watch_revisions, _) = watch::channel(0u64);
        let core = Arc::new(LibraryCore {
            library: Mutex::new(library),
            identify_pass: Mutex::new(()),
            own_files: Mutex::new(std::env::current_exe().into_iter().collect()),
            watch_revisions,
        });
        let identity = Arc::new(IdentityWorker::new(Arc::downgrade(&core)));
        let weak = Arc::downgrade(&core);
        let watcher_identity = Arc::downgrade(&identity);
        let callback = move |folder: &Path| {
            let Some(core) = weak.upgrade() else {
                return;
            };
            let own_files = core
                .own_files
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let changed = {
                let mut library = core.library.lock().unwrap_or_else(PoisonError::into_inner);
                resonance_library::scan::scan(&mut library, folder, &own_files)
                    .is_ok_and(|report| report.added > 0 || report.changed > 0)
            };
            if changed {
                core.watch_revisions.send_modify(|revision| *revision += 1);
                if let Some(worker) = watcher_identity.upgrade() {
                    worker.wake();
                }
            }
        };
        let watcher = match quiet_window {
            Some(window) => SourceWatcher::with_quiet_window(window, callback),
            None => SourceWatcher::new(callback),
        };
        let api = Self {
            watcher,
            identity,
            core,
            mix: Mix::new(),
        };
        {
            let guard = api
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            api.reconcile_watchers(&guard);
        }
        api.identity.wake();
        api
    }

    /// The decks, for a transport that streams their telemetry and waveforms.
    pub fn mix(&self) -> &Mix {
        &self.mix
    }

    /// Subscribes to library revisions bumped when a background source watcher finds new or
    /// modified files on disk.
    pub fn watch_revisions(&self) -> watch::Receiver<u64> {
        let mut receiver = self.core.watch_revisions.subscribe();
        receiver.mark_unchanged();
        receiver
    }

    pub(crate) fn own_files_snapshot(&self) -> Vec<PathBuf> {
        self.core
            .own_files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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
            Request::LibraryFacets => self.library_facets().map(Response::LibraryFacets),
            Request::Scan { folder } => self.scan(Path::new(&folder)).map(Response::Scan),
            Request::Identify => self.identify().map(Response::Identify),
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
            Request::MixLoad { deck, path } => self
                .mix
                .load(deck_id(deck), Path::new(&path))
                .map(|status| Response::MixLoad(mix_state(status)))
                .map_err(ApiError::audio_failed),
            Request::MixPlay { deck, playing } => self
                .mix
                .play(deck_id(deck), playing)
                .map(|status| Response::MixPlay(mix_state(status)))
                .map_err(ApiError::audio_failed),
            Request::MixStatus => Ok(Response::MixStatus(mix_state(self.mix.status()))),
        }
    }

    fn tracks(
        &self,
        offset: u32,
        limit: u32,
        query: Option<TrackQuery>,
    ) -> Result<TrackPage, ApiError> {
        let core_query = core_track_query(query);
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
        let total = if core_query.is_narrowed() {
            library
                .query_track_count(&core_query)
                .map_err(ApiError::library_unreadable)?
        } else {
            library_total
        };
        let rows = library
            .query_tracks_page(&core_query, offset, limit)
            .map_err(ApiError::library_unreadable)?;
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
        })
    }

    fn scan(&self, folder: &Path) -> Result<ScanReport, ApiError> {
        let own_files = self.own_files_snapshot();
        let report = {
            let mut library = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let report = resonance_library::scan::scan(&mut library, folder, &own_files)
                .map_err(ApiError::scan_failed)?;
            self.reconcile_watchers(&library);
            report
        };
        if report.added > 0 || report.changed > 0 {
            self.identity.wake();
        }
        Ok(ScanReport {
            source_folder: report.source_folder,
            found: report.found,
            added: report.added,
            changed: report.changed,
            unchanged: report.unchanged,
            unreadable_count: report.unreadable.len(),
            unreadable: report
                .unreadable
                .into_iter()
                .take(REPORTED_UNREADABLE_FILES)
                .map(|file| UnreadableFile {
                    path: file.path,
                    reason: file.reason,
                })
                .collect(),
        })
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

fn core_track_query(query: Option<TrackQuery>) -> CoreTrackQuery {
    let Some(query) = query else {
        return CoreTrackQuery::default();
    };
    CoreTrackQuery {
        text: query.text,
        pill: match query.pill.unwrap_or_default() {
            TrackFilterPill::All => CoreFilterPill::All,
            TrackFilterPill::Favourites => CoreFilterPill::Favourites,
            TrackFilterPill::Recent => CoreFilterPill::Recent,
        },
        bpm_min: query.bpm_min,
        bpm_max: query.bpm_max,
        keys: query.keys.unwrap_or_default(),
        year_min: query.year_min,
        year_max: query.year_max,
        label: query.label,
        formats: query
            .formats
            .unwrap_or_default()
            .into_iter()
            .map(|format| match format {
                TrackFormat::Mp3 => AudioFormat::Mp3,
                TrackFormat::Flac => AudioFormat::Flac,
                TrackFormat::Aiff => AudioFormat::Aiff,
                TrackFormat::Wav => AudioFormat::Wav,
                TrackFormat::M4a => AudioFormat::M4a,
            })
            .collect(),
        sort: match query.sort.unwrap_or_default() {
            TrackSortField::Index => CoreTrackSortField::Index,
            TrackSortField::Title => CoreTrackSortField::Title,
            TrackSortField::Artist => CoreTrackSortField::Artist,
            TrackSortField::Bpm => CoreTrackSortField::Bpm,
            TrackSortField::Key => CoreTrackSortField::Key,
            TrackSortField::Duration => CoreTrackSortField::Duration,
            TrackSortField::Added => CoreTrackSortField::Added,
        },
        direction: match query.direction.unwrap_or_default() {
            SortDirection::Asc => CoreSortDirection::Asc,
            SortDirection::Desc => CoreSortDirection::Desc,
        },
    }
}

fn library_track(row: &TrackRow) -> LibraryTrack {
    let file = &row.file;
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
        bpm: file.bpm(),
        key: file.key().map(|key| TrackKey {
            camelot: key.camelot(),
            name: key.name().to_owned(),
        }),
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
    }
}

#[cfg(test)]
mod tests {
    use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile};

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
                    keys: Some(vec!["11A".to_owned()]),
                    ..TrackQuery::default()
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
}
