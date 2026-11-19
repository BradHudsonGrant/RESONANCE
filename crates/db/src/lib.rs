mod artwork;
mod collections;
mod filters;
mod identification;
mod identity;
mod jobs;
mod operations;
mod prep;
mod preview_settings;
mod renames;
mod search;
mod sources;
mod table_layout;
mod tags;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use artwork::{ARTWORK_FOLDER, ArtworkError, ArtworkStore, ProposedArtwork, StoredArtwork};
pub use collections::{CollectionSummary, TrackCollection};
pub use filters::SmartView;
pub use identification::{
    CachedResponse, IdentificationInput, IdentificationPass, StoredIdentification,
};
pub use identity::{
    IdentityMatch, PendingAsset, StoredAssetIdentity, StoredRelinkCandidate,
    StoredTrackRelinkTarget,
};
pub use operations::StoredOperation;
pub use prep::BatchTagOperation;
pub use renames::{RenameTarget, StoredRename};
use resonance_core::collections::CollectionNameError;
use resonance_core::library::{
    AudioFacts, AudioFormat, ScannedFile, TagField, is_asset_path_included,
};
use resonance_core::staging::effective_tags;
use rusqlite::{Connection, OptionalExtension, Row, params};
pub use search::{LibraryFacets, now_unix_ms};
pub use sources::{SourceAssetPresence, SourceConfig};
pub use tags::{AcceptReport, StagingQueueEntry, TagLayers, TagWriteTarget};

/// Numbered and forward-only. A database is brought up to date by running the ones it has not
/// seen, in order, and an old one is never edited.
const MIGRATIONS: [&str; 16] = [
    include_str!("../migrations/0001_library.sql"),
    include_str!("../migrations/0002_sources.sql"),
    include_str!("../migrations/0003_identity.sql"),
    include_str!("../migrations/0004_search_and_sort.sql"),
    include_str!("../migrations/0005_asset_fts_delete_trigger.sql"),
    include_str!("../migrations/0006_collections.sql"),
    include_str!("../migrations/0007_missing_files.sql"),
    include_str!("../migrations/0008_jobs.sql"),
    include_str!("../migrations/0009_identification.sql"),
    include_str!("../migrations/0010_tag_layers.sql"),
    include_str!("../migrations/0011_operations.sql"),
    include_str!("../migrations/0012_operation_renames.sql"),
    include_str!("../migrations/0013_job_pause.sql"),
    include_str!("../migrations/0014_artwork.sql"),
    include_str!("../migrations/0015_table_columns.sql"),
    include_str!("../migrations/0016_filters.sql"),
];

pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// The database's file name inside the app's data folder.
pub const FILE_NAME: &str = "library.db";

#[derive(Debug)]
pub enum DbError {
    Folder { path: PathBuf, source: io::Error },
    Sqlite(rusqlite::Error),
    NewerSchema { found: u32, known: u32 },
    UnknownName { column: &'static str, name: String },
    InvalidCollectionName(CollectionNameError),
    DuplicateCollectionName { name: String },
    DuplicateViewName { name: String },
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Folder { path, source } => write!(
                f,
                "The folder for the database, {}, could not be made: {source}",
                path.display()
            ),
            Self::Sqlite(source) => write!(f, "The database reported an error: {source}"),
            Self::NewerSchema { found, known } => write!(
                f,
                "The database is at schema version {found} and this build knows up to {known}. \
                 It was last opened by a newer version of Resonance."
            ),
            Self::UnknownName { column, name } => {
                write!(
                    f,
                    "The database holds \"{name}\" in {column}, which this build does not know"
                )
            }
            Self::InvalidCollectionName(error) => error.fmt(f),
            Self::DuplicateCollectionName { name } => {
                write!(f, "A collection named \"{name}\" already exists.")
            }
            Self::DuplicateViewName { name } => {
                write!(f, "A smart view named \"{name}\" already exists.")
            }
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(source: rusqlite::Error) -> Self {
        Self::Sqlite(source)
    }
}

impl From<CollectionNameError> for DbError {
    fn from(source: CollectionNameError) -> Self {
        Self::InvalidCollectionName(source)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(pub(crate) i64);

impl SourceId {
    pub const fn from_raw(id: i64) -> Self {
        Self(id)
    }

    pub const fn raw(self) -> i64 {
        self.0
    }
}

/// Enough to tell whether a file has changed since it was last scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub size_bytes: u64,
    pub modified_ns: i64,
}

/// A track as the library lists it: its file and where that file's source folder is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRow {
    pub track_id: i64,
    pub source_folder: String,
    pub file: ScannedFile,
    pub favourite: bool,
    pub collections: Vec<TrackCollection>,
    pub missing_since: Option<i64>,
    /// Values the user typed or accepted, which the track shows in place of the file's.
    pub user_tags: Vec<(TagField, String)>,
    /// The hash of the picture the track shows: the user's, else its file's front cover.
    pub artwork: Option<String>,
    /// When the track first came into the library, in Unix nanoseconds.
    pub added_ns: i64,
}

/// The connection that writes. Readers open their own.
pub struct Database {
    connection: Connection,
    artwork: Option<ArtworkStore>,
}

fn migrate(connection: &mut Connection) -> Result<(), DbError> {
    let found: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found > SCHEMA_VERSION {
        return Err(DbError::NewerSchema {
            found,
            known: SCHEMA_VERSION,
        });
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let transaction = connection.transaction()?;
        transaction.execute_batch(migration)?;
        if index == 3 {
            search::backfill_v4_metadata(&transaction)?;
        }
        if index == 14 {
            search::backfill_v15_columns(&transaction)?;
        }
        transaction.pragma_update(None, "user_version", index as u32 + 1)?;
        transaction.commit()?;
    }
    Ok(())
}

impl Database {
    /// Opens the database file, making it and its folder if they are missing, and brings its
    /// schema up to date.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(folder) = path
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
        {
            fs::create_dir_all(folder).map_err(|source| DbError::Folder {
                path: folder.to_owned(),
                source,
            })?;
        }
        let mut database = Self::prepared(Connection::open(path)?)?;
        database.artwork = Some(ArtworkStore::new(
            path.parent().unwrap_or(Path::new("")).join(ARTWORK_FOLDER),
        ));
        Ok(database)
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::prepared(Connection::open_in_memory()?)
    }

    fn prepared(mut connection: Connection) -> Result<Self, DbError> {
        // WAL lets the UI read while a scan writes. It returns the mode it ended up in.
        connection.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut connection)?;
        Ok(Self {
            connection,
            artwork: None,
        })
    }

    pub fn schema_version(&self) -> Result<u32, DbError> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// The source for a folder, added if this is the first time the folder is seen.
    pub fn source(&mut self, folder: &str) -> Result<SourceId, DbError> {
        let known = self
            .connection
            .query_row("SELECT id FROM source WHERE folder = ?1", [folder], |row| {
                row.get(0)
            })
            .optional()?;
        if let Some(id) = known {
            return Ok(SourceId(id));
        }
        self.connection
            .execute("INSERT INTO source (folder) VALUES (?1)", [folder])?;
        Ok(SourceId(self.connection.last_insert_rowid()))
    }

    /// The size and modified time each file in the source had when it was last scanned, by path.
    pub fn file_stamps(&self, source: SourceId) -> Result<HashMap<String, FileStamp>, DbError> {
        let mut statement = self
            .connection
            .prepare("SELECT path, size_bytes, modified_ns FROM asset WHERE source_id = ?1")?;
        let stamps = statement
            .query_map([source.0], |row| {
                Ok((
                    row.get(0)?,
                    FileStamp {
                        size_bytes: row.get(1)?,
                        modified_ns: row.get(2)?,
                    },
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(stamps)
    }

    /// Stores what a scan read. A path seen for the first time becomes a new track with one
    /// asset. A path seen before keeps its track and has its facts and file tags replaced.
    pub fn record_scan(&mut self, source: SourceId, files: &[ScannedFile]) -> Result<(), DbError> {
        let transaction = self.connection.transaction()?;
        {
            let (include_subfolders, excluded_subfolders, ignored_folders) =
                sources::source_inclusion_in_tx(&transaction, source)?;
            let max_added: i64 = transaction.query_row(
                "SELECT COALESCE(MAX(added_ns), 0) FROM track",
                [],
                |row| row.get(0),
            )?;
            let mut next_added_ns = search::now_unix_ns().max(max_added + 1);

            let mut find =
                transaction.prepare("SELECT id FROM asset WHERE source_id = ?1 AND path = ?2")?;
            let mut add_track = transaction.prepare("INSERT INTO track (added_ns) VALUES (?1)")?;
            let mut add_asset = transaction.prepare(
                "INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format,
                     duration_ms, sample_rate, channels, bit_depth, bitrate_kbps, included,
                     sort_title, sort_artist, tag_album, tag_genre, tag_label, tag_key, sort_key,
                     tag_bpm, tag_year, tag_catno, tag_comment, tag_composer, tag_remixer, tag_mix,
                     title_tagged)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                         ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
            )?;
            let mut update_asset = transaction.prepare(
                "UPDATE asset SET size_bytes = ?2, modified_ns = ?3, format = ?4, duration_ms = ?5,
                     sample_rate = ?6, channels = ?7, bit_depth = ?8, bitrate_kbps = ?9,
                     included = ?10, missing_since = NULL, sort_title = ?11, sort_artist = ?12,
                     tag_album = ?13, tag_genre = ?14, tag_label = ?15, tag_key = ?16,
                     sort_key = ?17, tag_bpm = ?18, tag_year = ?19, tag_catno = ?20,
                     tag_comment = ?21, tag_composer = ?22, tag_remixer = ?23, tag_mix = ?24,
                     title_tagged = ?25
                 WHERE id = ?1",
            )?;
            let mut clear_tags = transaction.prepare("DELETE FROM file_tag WHERE asset_id = ?1")?;
            let mut add_tag = transaction
                .prepare("INSERT INTO file_tag (asset_id, field, value) VALUES (?1, ?2, ?3)")?;
            let mut del_fts = transaction.prepare("DELETE FROM asset_fts WHERE rowid = ?1")?;
            let mut add_fts = transaction.prepare(
                "INSERT INTO asset_fts (rowid, title, artist, album, genre, label, comment, path)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;

            for file in files {
                let audio = file.audio;
                let duration_ms = audio.map(|audio| audio.duration_ms);
                let sample_rate = audio.and_then(|audio| audio.sample_rate);
                let channels = audio.and_then(|audio| audio.channels);
                let bit_depth = audio.and_then(|audio| audio.bit_depth);
                let bitrate_kbps = audio.and_then(|audio| audio.bitrate_kbps);
                let included = i64::from(is_asset_path_included(
                    &file.path,
                    include_subfolders,
                    &excluded_subfolders,
                    &ignored_folders,
                ));
                let known: Option<i64> = find
                    .query_row(params![source.0, file.path], |row| row.get(0))
                    .optional()?;
                let user = match known {
                    Some(asset) => tags::user_tags_of_asset(&transaction, asset)?,
                    None => Vec::new(),
                };
                let shown = (!user.is_empty()).then(|| effective_tags(&file.tags, &user));
                let shown_tags = shown.as_deref().unwrap_or(&file.tags);
                let meta = search::AssetSearchMeta::from_path_and_tags(&file.path, shown_tags);
                let asset = match known {
                    Some(asset) => {
                        update_asset.execute(params![
                            asset,
                            file.size_bytes,
                            file.modified_ns,
                            file.format.name(),
                            duration_ms,
                            sample_rate,
                            channels,
                            bit_depth,
                            bitrate_kbps,
                            included,
                            meta.title,
                            meta.artist,
                            meta.album,
                            meta.genre,
                            meta.label,
                            meta.tag_key,
                            meta.sort_key,
                            meta.tag_bpm,
                            meta.tag_year,
                            meta.catno,
                            meta.comment,
                            meta.composer,
                            meta.remixer,
                            meta.mix,
                            meta.title_tagged,
                        ])?;
                        clear_tags.execute([asset])?;
                        del_fts.execute([asset])?;
                        asset
                    }
                    None => {
                        add_track.execute([next_added_ns])?;
                        next_added_ns += 1;
                        let track = transaction.last_insert_rowid();
                        add_asset.execute(params![
                            track,
                            source.0,
                            file.path,
                            file.size_bytes,
                            file.modified_ns,
                            file.format.name(),
                            duration_ms,
                            sample_rate,
                            channels,
                            bit_depth,
                            bitrate_kbps,
                            included,
                            meta.title,
                            meta.artist,
                            meta.album,
                            meta.genre,
                            meta.label,
                            meta.tag_key,
                            meta.sort_key,
                            meta.tag_bpm,
                            meta.tag_year,
                            meta.catno,
                            meta.comment,
                            meta.composer,
                            meta.remixer,
                            meta.mix,
                            meta.title_tagged,
                        ])?;
                        transaction.last_insert_rowid()
                    }
                };
                for (field, value) in &file.tags {
                    add_tag.execute(params![asset, field.name(), value])?;
                }
                search::write_tag_values(&transaction, asset, shown_tags)?;
                search::write_missing_tags(&transaction, asset, shown_tags)?;
                add_fts.execute(params![
                    asset,
                    meta.title,
                    meta.artist.unwrap_or(""),
                    meta.album.unwrap_or(""),
                    meta.genre.unwrap_or(""),
                    meta.label.unwrap_or(""),
                    meta.comment.unwrap_or(""),
                    file.path,
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Every track with its file, in library order.
    pub fn tracks(&self) -> Result<Vec<TrackRow>, DbError> {
        let mut tags: HashMap<i64, Vec<(TagField, String)>> = HashMap::new();
        let mut statement = self
            .connection
            .prepare("SELECT asset_id, field, value FROM file_tag")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            tags.entry(row.get(0)?)
                .or_default()
                .push((tag_field(row.get(1)?)?, row.get(2)?));
        }

        let mut statement = self.connection.prepare(TRACKS_IN_ORDER)?;
        let mut rows = statement.query([])?;
        let mut tracks = Vec::new();
        while let Some(row) = rows.next()? {
            let (asset, mut track) = track_row(row)?;
            track.file.tags = tags.remove(&asset).unwrap_or_default();
            track.file.tags.sort();
            tracks.push(track);
        }
        collections::populate_track_collections(&self.connection, &mut tracks)?;
        tags::populate_user_tags(&self.connection, &mut tracks)?;
        artwork::populate_user_artwork(&self.connection, &mut tracks)?;
        Ok(tracks)
    }

    /// Up to `limit` tracks starting at `offset`, in library order. A table that shows a few
    /// dozen rows of a large library asks for the part in view.
    pub fn tracks_page(&self, offset: u32, limit: u32) -> Result<Vec<TrackRow>, DbError> {
        let mut statement = self
            .connection
            .prepare(&format!("{TRACKS_IN_ORDER} LIMIT ?1 OFFSET ?2"))?;
        let mut tags_of = self
            .connection
            .prepare("SELECT field, value FROM file_tag WHERE asset_id = ?1 ORDER BY field")?;
        let mut rows = statement.query([limit, offset])?;
        let mut tracks = Vec::new();
        while let Some(row) = rows.next()? {
            let (asset, mut track) = track_row(row)?;
            let mut tags = tags_of.query([asset])?;
            while let Some(tag) = tags.next()? {
                track.file.tags.push((tag_field(tag.get(0)?)?, tag.get(1)?));
            }
            track.file.tags.sort();
            tracks.push(track);
        }
        collections::populate_track_collections(&self.connection, &mut tracks)?;
        tags::populate_user_tags(&self.connection, &mut tracks)?;
        artwork::populate_user_artwork(&self.connection, &mut tracks)?;
        Ok(tracks)
    }

    pub fn track_count(&self) -> Result<u32, DbError> {
        Ok(self.connection.query_row(
            "SELECT count(*) FROM asset WHERE included = 1",
            [],
            |row| row.get(0),
        )?)
    }

    /// Returns the track row for `track_id` (preferring its primary included asset), or `None`.
    pub fn track_by_id(&self, track_id: i64) -> Result<Option<TrackRow>, DbError> {
        let asset_id: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM asset
                 WHERE track_id = ?1 AND included = 1
                 ORDER BY is_primary DESC, id ASC
                 LIMIT 1",
                [track_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(asset_id) = asset_id else {
            return Ok(None);
        };
        Ok(search::load_tracks_by_ids(&self.connection, &[asset_id])?
            .into_iter()
            .next())
    }
}

/// Library order: by source folder, then by path. Read sources by their folder index and each
/// source's assets by the (source, path) index, and the rows come out in this order with no
/// sort, so a page deep in a large library costs a walk along an index. `CROSS JOIN` holds
/// SQLite to that plan: left to choose, it sorted every row, and the last page of 100,000 took
/// 585 ms instead of 13 ms.
const TRACKS_IN_ORDER: &str = "SELECT asset.id, asset.track_id, source.folder, asset.path,
         asset.size_bytes, asset.modified_ns, asset.format, asset.duration_ms, asset.sample_rate,
         asset.channels, asset.bit_depth, asset.bitrate_kbps, track.favourite, asset.missing_since,
         asset.artwork_hash, track.added_ns
     FROM source
     CROSS JOIN asset ON asset.source_id = source.id
     JOIN track ON track.id = asset.track_id
     WHERE asset.included = 1
     ORDER BY source.folder, asset.path";

pub(crate) fn tag_field(name: String) -> Result<TagField, DbError> {
    TagField::from_name(&name).ok_or(DbError::UnknownName {
        column: "file_tag.field",
        name,
    })
}

/// A row of `TRACKS_IN_ORDER` as its asset id and its track, without tags.
pub(crate) fn track_row(row: &Row<'_>) -> Result<(i64, TrackRow), DbError> {
    let name: String = row.get(6)?;
    let format = AudioFormat::from_name(&name).ok_or(DbError::UnknownName {
        column: "asset.format",
        name,
    })?;
    let duration_ms: Option<u64> = row.get(7)?;
    let audio = match duration_ms {
        Some(duration_ms) => Some(AudioFacts {
            duration_ms,
            sample_rate: row.get(8)?,
            channels: row.get(9)?,
            bit_depth: row.get(10)?,
            bitrate_kbps: row.get(11)?,
        }),
        None => None,
    };
    let favourite_raw: i64 = row.get(12)?;
    let missing_since: Option<i64> = row.get(13)?;
    let artwork: Option<String> = row.get(14)?;
    let added_ns: i64 = row.get(15)?;
    let track = TrackRow {
        track_id: row.get(1)?,
        source_folder: row.get(2)?,
        file: ScannedFile {
            path: row.get(3)?,
            size_bytes: row.get(4)?,
            modified_ns: row.get(5)?,
            format,
            audio,
            tags: Vec::new(),
        },
        favourite: favourite_raw != 0,
        collections: Vec::new(),
        missing_since,
        user_tags: Vec::new(),
        artwork,
        added_ns,
    };
    Ok((row.get(0)?, track))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(connection: &Connection) -> Vec<(String, String)> {
        let mut statement = connection
            .prepare("SELECT name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")
            .expect("the schema query prepares");
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("the schema query runs")
            .collect::<Result<_, _>>()
            .expect("the schema rows read")
    }

    #[test]
    fn a_database_at_every_earlier_version_migrates_to_the_schema_a_new_one_gets() {
        let mut new = Connection::open_in_memory().expect("a database opens");
        migrate(&mut new).expect("a new database migrates");

        for version in 0..SCHEMA_VERSION {
            let mut old = Connection::open_in_memory().expect("a database opens");
            for migration in &MIGRATIONS[..version as usize] {
                old.execute_batch(migration)
                    .expect("an earlier migration runs");
            }
            old.pragma_update(None, "user_version", version)
                .expect("the version is stamped");

            migrate(&mut old).expect("the old database migrates");

            assert_eq!(schema(&old), schema(&new), "from version {version}");
            let stamped: u32 = old
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .expect("the version reads");
            assert_eq!(stamped, SCHEMA_VERSION);
        }
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_and_left_alone() {
        let mut newer = Connection::open_in_memory().expect("a database opens");
        newer
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .expect("the version is stamped");

        let refused = migrate(&mut newer);

        assert!(matches!(
            refused,
            Err(DbError::NewerSchema { found, known })
                if found == SCHEMA_VERSION + 1 && known == SCHEMA_VERSION
        ));
        assert!(schema(&newer).is_empty());
    }

    #[test]
    fn migrating_from_v1_preserves_existing_tracks_with_default_source_and_ignore_settings() {
        let mut v1 = Connection::open_in_memory().expect("a database opens");
        v1.pragma_update(None, "foreign_keys", true)
            .expect("foreign keys enable");
        v1.execute_batch(MIGRATIONS[0]).expect("v1 schema applies");
        v1.pragma_update(None, "user_version", 1u32)
            .expect("version 1 stamps");
        v1.execute("INSERT INTO source (folder) VALUES ('D:\\Music')", [])
            .expect("v1 source inserts");
        v1.execute("INSERT INTO track DEFAULT VALUES", [])
            .expect("v1 track inserts");
        v1.execute(
            "INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format)
             VALUES (1, 1, 'House/one.flac', 100, 200, 'flac')",
            [],
        )
        .expect("v1 asset inserts");

        migrate(&mut v1).expect("v1 migrates to current schema");
        let db = Database {
            connection: v1,
            artwork: None,
        };

        let sources = db.sources().expect("sources read");
        assert_eq!(sources.len(), 1);
        assert!(sources[0].include_subfolders);
        assert!(sources[0].watch);
        assert!(sources[0].excluded_subfolders.is_empty());
        assert_eq!(sources[0].track_count, 1);
        assert_eq!(db.track_count().expect("count reads"), 1);
        assert_eq!(
            db.ignored_folders().expect("ignored folders read"),
            resonance_core::library::DEFAULT_IGNORED_FOLDERS
        );
    }

    #[test]
    fn tracks_in_order_walks_indices_without_a_temp_sort() {
        let db = Database::open_in_memory().expect("a database opens");
        let mut stmt = db
            .connection
            .prepare(&format!(
                "EXPLAIN QUERY PLAN {TRACKS_IN_ORDER} LIMIT 200 OFFSET 50000"
            ))
            .expect("explain prepares");
        let steps: Vec<String> = stmt
            .query_map([], |row| row.get(3))
            .expect("explain runs")
            .collect::<Result<_, _>>()
            .expect("steps read");

        assert!(
            steps.iter().all(|step| !step.contains("TEMP B-TREE")),
            "unexpected temp sort in plan: {steps:?}"
        );
    }

    #[test]
    fn migrating_from_v3_backfills_search_and_normalises_tags() {
        use resonance_core::search::{FilterPill, TrackQuery};

        let mut v3 = Connection::open_in_memory().expect("a database opens");
        v3.pragma_update(None, "foreign_keys", true)
            .expect("foreign keys enable");
        for migration in &MIGRATIONS[..3] {
            v3.execute_batch(migration).expect("prior migration runs");
        }
        v3.pragma_update(None, "user_version", 3u32)
            .expect("version 3 stamps");

        v3.execute("INSERT INTO source (folder) VALUES ('D:\\Legacy')", [])
            .expect("source inserts");
        v3.execute("INSERT INTO track DEFAULT VALUES", [])
            .expect("track 1 inserts");
        v3.execute(
            "INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format)
             VALUES (1, 1, 'Trance/tiesto_cut.mp3', 100, 1000, 'mp3')",
            [],
        )
        .expect("asset 1 inserts");
        v3.execute_batch(
            "INSERT INTO file_tag (asset_id, field, value) VALUES
                (1, 'artist', 'Tiësto'),
                (1, 'bpm', '127,5'),
                (1, 'key', 'A minor'),
                (1, 'year', '2021-05-14');",
        )
        .expect("tags insert");

        migrate(&mut v3).expect("v3 migrates to v4");
        let db = Database {
            connection: v3,
            artwork: None,
        };

        // FTS folds diacritics and uses stem title fallback when title tag is absent.
        let found = db
            .query_tracks_page(
                &TrackQuery {
                    text: Some("tiesto".to_owned()),
                    filters: ["bpm 127-128", "key 8A", "year 2021"]
                        .map(|text| {
                            resonance_core::filter::parse_command(text).expect("filter reads")
                        })
                        .to_vec(),
                    ..TrackQuery::default()
                },
                0,
                10,
            )
            .expect("query runs");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file.title(), "tiesto_cut");

        // Legacy tracks with 1970 modified_ns are excluded from the 30-day Recent window.
        assert_eq!(
            db.query_track_count(&TrackQuery {
                pill: FilterPill::Recent,
                ..TrackQuery::default()
            })
            .expect("recent count runs"),
            0
        );
    }

    #[test]
    fn migrating_from_v4_adds_asset_delete_fts_trigger() {
        use resonance_core::search::TrackQuery;

        let mut v4 = Connection::open_in_memory().expect("a database opens");
        v4.pragma_update(None, "foreign_keys", true)
            .expect("foreign keys enable");
        for migration in &MIGRATIONS[..4] {
            v4.execute_batch(migration).expect("prior migration runs");
        }
        v4.pragma_update(None, "user_version", 4u32)
            .expect("version 4 stamps");

        v4.execute("INSERT INTO source (folder) VALUES ('D:\\Club')", [])
            .expect("source inserts");
        v4.execute("INSERT INTO track DEFAULT VALUES", [])
            .expect("track inserts");
        v4.execute(
            "INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format, sort_title)
             VALUES (1, 1, '01/ghost_check.flac', 100, 1000, 'flac', 'ghost check')",
            [],
        )
        .expect("asset inserts");
        v4.execute(
            "INSERT INTO asset_fts (rowid, title, artist, album, genre, label, path)
             VALUES (1, 'Ghost Check', 'Bicep', '', '', '', '01/ghost_check.flac')",
            [],
        )
        .expect("fts row inserts");

        migrate(&mut v4).expect("v4 migrates to v5");
        v4.execute("DELETE FROM asset WHERE id = 1", [])
            .expect("asset deletes");

        let fts_rows: i64 = v4
            .query_row("SELECT count(*) FROM asset_fts", [], |row| row.get(0))
            .expect("fts count reads");
        assert_eq!(fts_rows, 0);

        let db = Database {
            connection: v4,
            artwork: None,
        };
        assert_eq!(
            db.query_track_count(&TrackQuery {
                text: Some("ghost".to_owned()),
                ..TrackQuery::default()
            })
            .expect("fts query runs"),
            0
        );
    }

    #[test]
    fn migrating_from_v5_adds_collections_and_cascades_cleanly() {
        let mut v5 = Connection::open_in_memory().expect("in-memory sqlite opens");
        v5.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("foreign keys enable");
        for (i, sql) in MIGRATIONS[..5].iter().enumerate() {
            v5.execute_batch(sql).expect("prior migration runs");
            v5.pragma_update(None, "user_version", i64::try_from(i + 1).expect("fits"))
                .expect("user_version stamps");
        }
        v5.execute("INSERT INTO source (folder) VALUES ('D:\\Club')", [])
            .expect("source inserts");
        v5.execute("INSERT INTO track DEFAULT VALUES", [])
            .expect("track inserts");
        v5.execute(
            "INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format, sort_title)
             VALUES (1, 1, '01/opener.flac', 100, 1000, 'flac', 'opener')",
            [],
        )
        .expect("asset inserts");

        migrate(&mut v5).expect("v5 migrates to v6");

        let mut db = Database {
            connection: v5,
            artwork: None,
        };
        let col = db.create_collection("Warm Up").expect("collection creates");
        db.add_tracks_to_collection(col.id, &[1])
            .expect("track adds to collection");
        assert_eq!(db.collections().expect("lists")[0].track_count, 1);

        db.connection
            .execute("DELETE FROM asset WHERE track_id = 1", [])
            .expect("asset deletes");
        db.connection
            .execute("DELETE FROM track WHERE id = 1", [])
            .expect("track deletes");
        assert_eq!(db.collections().expect("lists")[0].track_count, 0);
    }

    #[test]
    fn track_by_id_returns_primary_included_track_or_none() {
        use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};

        let mut db = Database::open_in_memory().expect("in-memory sqlite opens");
        let source = db.source("D:\\Club").expect("source opens");
        let files = [ScannedFile {
            path: "deep/opener.flac".to_owned(),
            size_bytes: 1_234,
            modified_ns: 0,
            format: AudioFormat::Flac,
            audio: Some(AudioFacts {
                duration_ms: 185_000,
                sample_rate: Some(44_100),
                channels: Some(2),
                bit_depth: Some(16),
                bitrate_kbps: Some(900),
            }),
            tags: vec![(TagField::Title, "Opener".to_owned())],
        }];
        db.record_scan(source, &files).expect("scan writes");

        let found = db
            .track_by_id(1)
            .expect("lookup succeeds")
            .expect("track exists");
        assert_eq!(found.track_id, 1);
        assert_eq!(found.file.title(), "Opener");
        assert_eq!(found.source_folder, "D:\\Club");

        assert!(db.track_by_id(999).expect("lookup succeeds").is_none());

        db.connection
            .execute("UPDATE asset SET included = 0 WHERE track_id = 1", [])
            .expect("asset excludes");
        assert!(db.track_by_id(1).expect("lookup succeeds").is_none());
    }

    #[test]
    fn migrating_from_v6_adds_missing_since_and_filters_missing_tracks() {
        use resonance_core::search::TrackQuery;

        let mut v6 = Connection::open_in_memory().expect("in-memory sqlite opens");
        v6.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("foreign keys enable");
        for (i, sql) in MIGRATIONS[..6].iter().enumerate() {
            v6.execute_batch(sql).expect("prior migration runs");
            v6.pragma_update(None, "user_version", i64::try_from(i + 1).expect("fits"))
                .expect("user_version stamps");
        }
        v6.execute("INSERT INTO source (folder) VALUES ('D:\\Club')", [])
            .expect("source inserts");
        v6.execute("INSERT INTO track DEFAULT VALUES", [])
            .expect("track inserts");
        v6.execute(
            "INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format, sort_title)
             VALUES (1, 1, '01/missing_cut.flac', 100, 1000, 'flac', 'missing cut')",
            [],
        )
        .expect("asset inserts");

        migrate(&mut v6).expect("v6 migrates to v7");
        let mut db = Database {
            connection: v6,
            artwork: None,
        };
        assert_eq!(db.tracks().expect("tracks read")[0].missing_since, None);

        db.set_assets_missing_since(&[(1, Some(1_730_000_000_000))])
            .expect("marks missing");
        assert_eq!(
            db.tracks().expect("tracks read")[0].missing_since,
            Some(1_730_000_000_000)
        );
        assert_eq!(db.library_facets().expect("facets").missing_count, 1);
        assert_eq!(
            db.query_track_count(&TrackQuery {
                missing_only: true,
                ..TrackQuery::default()
            })
            .expect("missing count"),
            1
        );
    }

    #[test]
    fn migrating_from_v7_adds_job_table_and_recovers_interrupted_jobs() {
        use resonance_core::jobs::{JOB_KIND_SCAN_SOURCE, JobStatus, JobTier, NewJob};

        let mut v7 = Connection::open_in_memory().expect("in-memory sqlite opens");
        v7.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("foreign keys enable");
        for (i, sql) in MIGRATIONS[..7].iter().enumerate() {
            v7.execute_batch(sql).expect("prior migration runs");
            v7.pragma_update(None, "user_version", i64::try_from(i + 1).expect("fits"))
                .expect("user_version stamps");
        }
        v7.execute("INSERT INTO source (folder) VALUES ('D:\\Club')", [])
            .expect("source inserts");
        v7.execute("INSERT INTO track DEFAULT VALUES", [])
            .expect("track inserts");
        v7.execute(
            "INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format, sort_title)
             VALUES (1, 1, '01/opener.flac', 100, 1000, 'flac', 'opener')",
            [],
        )
        .expect("asset inserts");

        migrate(&mut v7).expect("v7 migrates");
        let mut db = Database {
            connection: v7,
            artwork: None,
        };
        assert_eq!(db.schema_version().expect("schema version"), SCHEMA_VERSION);
        assert_eq!(db.track_count().expect("tracks preserved"), 1);

        let queued = db
            .enqueue_job(&NewJob {
                kind: JOB_KIND_SCAN_SOURCE.to_owned(),
                tier: JobTier::Scan,
                label: "Scan Club".to_owned(),
                payload_json: r#"{"folder":"D:\\Club"}"#.to_owned(),
                total_items: Some(12),
            })
            .expect("job inserts");
        assert_eq!(queued.status, JobStatus::Queued);

        let claimed = db
            .claim_next_job(true)
            .expect("claim succeeds")
            .expect("job claimed");
        assert_eq!(claimed.id, queued.id);
        assert_eq!(claimed.status, JobStatus::Running);
        assert!(claimed.started_ns.is_some());

        let recovered = db.recover_interrupted_jobs().expect("recovery runs");
        assert_eq!(recovered, 1);
        let reset = db
            .job_by_id(queued.id)
            .expect("job reads")
            .expect("job exists");
        assert_eq!(reset.status, JobStatus::Queued);
        assert_eq!(reset.started_ns, None);
    }

    #[test]
    fn migrating_from_v14_fills_the_table_columns_from_what_tracks_show() {
        let mut v14 = Connection::open_in_memory().expect("a database opens");
        v14.pragma_update(None, "foreign_keys", true)
            .expect("foreign keys enable");
        for migration in &MIGRATIONS[..14] {
            v14.execute_batch(migration).expect("prior migration runs");
        }
        v14.pragma_update(None, "user_version", 14u32)
            .expect("version 14 stamps");
        v14.execute_batch(
            "INSERT INTO source (folder) VALUES ('club');
             INSERT INTO track DEFAULT VALUES;
             INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format, sort_title)
                 VALUES (1, 1, 'a.flac', 100, 1000, 'flac', 'a');
             INSERT INTO file_tag (asset_id, field, value) VALUES
                 (1, 'catno', ' LBL001 '), (1, 'mix', 'Dub'), (1, 'track_number', '7'),
                 (1, 'comment', 'Warm-up');
             INSERT INTO user_tag (track_id, field, value, updated_ns) VALUES (1, 'mix', 'Extended', 1);",
        )
        .expect("rows insert");

        migrate(&mut v14).expect("v14 migrates to v15");

        let row: (String, String, String) = v14
            .query_row(
                "SELECT tag_catno, tag_mix, tag_comment FROM asset WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("columns read");
        assert_eq!(
            row,
            (
                "LBL001".to_owned(),
                "Extended".to_owned(),
                "Warm-up".to_owned()
            )
        );
        let value: String = v14
            .query_row(
                "SELECT sort_value FROM asset_tag_value WHERE asset_id = 1 AND field = 'track_number'",
                [],
                |row| row.get(0),
            )
            .expect("tag value reads");
        assert_eq!(value, "000000000007");

        v14.execute("DELETE FROM asset WHERE id = 1", [])
            .expect("asset deletes");
        let left: i64 = v14
            .query_row("SELECT count(*) FROM asset_tag_value", [], |row| row.get(0))
            .expect("count reads");
        assert_eq!(left, 0);
    }
}
