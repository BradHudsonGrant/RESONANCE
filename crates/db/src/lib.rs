mod collections;
mod identity;
mod search;
mod sources;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use collections::{CollectionSummary, TrackCollection};
pub use identity::{IdentityMatch, PendingAsset, StoredAssetIdentity};
use resonance_core::collections::CollectionNameError;
use resonance_core::library::{
    AudioFacts, AudioFormat, ScannedFile, TagField, is_asset_path_included,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
pub use search::LibraryFacets;
pub use sources::SourceConfig;

/// Numbered and forward-only. A database is brought up to date by running the ones it has not
/// seen, in order, and an old one is never edited.
const MIGRATIONS: [&str; 6] = [
    include_str!("../migrations/0001_library.sql"),
    include_str!("../migrations/0002_sources.sql"),
    include_str!("../migrations/0003_identity.sql"),
    include_str!("../migrations/0004_search_and_sort.sql"),
    include_str!("../migrations/0005_asset_fts_delete_trigger.sql"),
    include_str!("../migrations/0006_collections.sql"),
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
}

/// The connection that writes. Readers open their own.
pub struct Database {
    connection: Connection,
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
        Self::prepared(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::prepared(Connection::open_in_memory()?)
    }

    fn prepared(mut connection: Connection) -> Result<Self, DbError> {
        // WAL lets the UI read while a scan writes. It returns the mode it ended up in.
        connection.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut connection)?;
        Ok(Self { connection })
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
                     tag_bpm, tag_year)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                         ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            )?;
            let mut update_asset = transaction.prepare(
                "UPDATE asset SET size_bytes = ?2, modified_ns = ?3, format = ?4, duration_ms = ?5,
                     sample_rate = ?6, channels = ?7, bit_depth = ?8, bitrate_kbps = ?9,
                     included = ?10, sort_title = ?11, sort_artist = ?12, tag_album = ?13,
                     tag_genre = ?14, tag_label = ?15, tag_key = ?16, sort_key = ?17,
                     tag_bpm = ?18, tag_year = ?19
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
                let meta = search::AssetSearchMeta::from_path_and_tags(&file.path, &file.tags);

                let known: Option<i64> = find
                    .query_row(params![source.0, file.path], |row| row.get(0))
                    .optional()?;
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
                        ])?;
                        transaction.last_insert_rowid()
                    }
                };
                for (field, value) in &file.tags {
                    add_tag.execute(params![asset, field.name(), value])?;
                }
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
         asset.channels, asset.bit_depth, asset.bitrate_kbps, track.favourite
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
        let db = Database { connection: v1 };

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
        let db = Database { connection: v3 };

        // FTS folds diacritics and uses stem title fallback when title tag is absent.
        let found = db
            .query_tracks_page(
                &TrackQuery {
                    text: Some("tiesto".to_owned()),
                    bpm_min: Some(127.0),
                    bpm_max: Some(128.0),
                    keys: vec!["8A".to_owned()],
                    year_min: Some(2021),
                    year_max: Some(2021),
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

        let db = Database { connection: v4 };
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

        let mut db = Database { connection: v5 };
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
}
