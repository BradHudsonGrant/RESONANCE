//! Cover pictures: the folder that keeps each picture once, and the file, user and proposed
//! layers that say which picture a track shows.
//!
//! The track shows the user's picture when there is one, else the one in its primary file.
//! Identification only ever proposes; a proposal reaches the user layer when it is accepted, and
//! a file only when the user writes tags.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{IN_CHUNK, placeholders};
use resonance_core::artwork::{ArtworkRefusal, ImageKind, check_artwork, is_artwork_hash};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::search::now_unix_ns;
use crate::tags::primary_asset;
use crate::{Database, DbError, SourceId, TrackRow};

/// The folder beside the database that holds the pictures.
pub const ARTWORK_FOLDER: &str = "artwork";

/// A picture in the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtwork {
    /// BLAKE3 of the bytes, in lowercase hex.
    pub hash: String,
    pub kind: ImageKind,
    pub byte_len: u64,
}

/// Result of looking up an external cover address in `artwork_source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachedArtworkSource {
    Stored(StoredArtwork),
    Unavailable,
}

#[derive(Debug)]
pub enum ArtworkError {
    Refused(ArtworkRefusal),
    Io(io::Error),
}

impl fmt::Display for ArtworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refusal) => f.write_str(refusal.message()),
            Self::Io(source) => write!(f, "The picture could not be stored: {source}"),
        }
    }
}

impl std::error::Error for ArtworkError {}

/// The folder of pictures, each named `<hash>.<extension>`. Writing the same picture twice keeps
/// one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkStore {
    folder: PathBuf,
}

impl ArtworkStore {
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn path_of(&self, hash: &str, kind: ImageKind) -> PathBuf {
        self.folder.join(format!("{hash}.{}", kind.extension()))
    }

    /// Checks the bytes are a picture the library keeps and stores them unless the same picture
    /// is already there.
    pub fn put(&self, bytes: &[u8]) -> Result<StoredArtwork, ArtworkError> {
        let kind = check_artwork(bytes).map_err(ArtworkError::Refused)?;
        let hash = blake3::hash(bytes).to_hex().to_string();
        let stored = StoredArtwork {
            byte_len: bytes.len() as u64,
            hash,
            kind,
        };
        let path = self.path_of(&stored.hash, kind);
        if fs::metadata(&path).is_ok_and(|metadata| metadata.len() == stored.byte_len) {
            return Ok(stored);
        }
        fs::create_dir_all(&self.folder).map_err(ArtworkError::Io)?;
        // Written whole beside the final name and then renamed, so a reader never sees half a
        // picture. Another thread storing the same picture writes its own temporary file.
        let temporary = self.folder.join(format!(
            "{}.{}.{:?}.tmp",
            stored.hash,
            kind.extension(),
            std::thread::current().id()
        ));
        let written = fs::File::create(&temporary)
            .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
            .and_then(|()| fs::rename(&temporary, &path));
        if let Err(error) = written {
            let _ = fs::remove_file(&temporary);
            // The rename can fail on Windows while a reader holds the file another writer put
            // there first. That file is the same picture.
            if !fs::metadata(&path).is_ok_and(|metadata| metadata.len() == stored.byte_len) {
                return Err(ArtworkError::Io(error));
            }
        }
        Ok(stored)
    }

    /// The stored file for a hash and its kind, or `None` when the name is not a hash or no such
    /// picture is stored.
    pub fn find(&self, hash: &str) -> Option<(PathBuf, ImageKind)> {
        if !is_artwork_hash(hash) {
            return None;
        }
        ImageKind::ALL
            .into_iter()
            .map(|kind| (self.path_of(hash, kind), kind))
            .find(|(path, _)| path.is_file())
    }

    pub fn read(&self, artwork: &StoredArtwork) -> io::Result<Vec<u8>> {
        fs::read(self.path_of(&artwork.hash, artwork.kind))
    }
}

/// A release picture identification found for a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedArtwork {
    pub artwork: StoredArtwork,
    pub provider: String,
    pub source_url: String,
    pub pending: bool,
}

/// Complete artwork layer for one track: shown cover, pending/decided proposal, and dirty flag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackArtworkState {
    pub shown: Option<StoredArtwork>,
    pub proposed: Option<ProposedArtwork>,
    pub dirty_on_disk: bool,
}

fn stored_artwork(connection: &Connection, hash: &str) -> Result<Option<StoredArtwork>, DbError> {
    let row: Option<(String, u64)> = connection
        .prepare_cached("SELECT mime, byte_len FROM artwork WHERE hash = ?1")?
        .query_row([hash], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?;
    let Some((mime, byte_len)) = row else {
        return Ok(None);
    };
    let kind = ImageKind::from_mime(&mime).ok_or(DbError::UnknownName {
        column: "artwork.mime",
        name: mime,
    })?;
    Ok(Some(StoredArtwork {
        hash: hash.to_owned(),
        kind,
        byte_len,
    }))
}

fn insert_artwork(tx: &Transaction<'_>, artwork: &StoredArtwork) -> Result<(), DbError> {
    tx.prepare_cached("INSERT OR IGNORE INTO artwork (hash, mime, byte_len) VALUES (?1, ?2, ?3)")?
        .execute(params![artwork.hash, artwork.kind.mime(), artwork.byte_len])?;
    Ok(())
}

/// The user's picture for the track that is not in its file yet.
pub(crate) fn unwritten_artwork(
    connection: &Connection,
    track_id: i64,
) -> Result<Option<StoredArtwork>, DbError> {
    let hash: Option<String> = connection
        .prepare_cached(
            "SELECT artwork_hash FROM user_artwork WHERE track_id = ?1 AND dirty_on_disk = 1",
        )?
        .query_row([track_id], |row| row.get(0))
        .optional()?;
    match hash {
        Some(hash) => stored_artwork(connection, &hash),
        None => Ok(None),
    }
}

pub(crate) fn tracks_with_unwritten_artwork(
    connection: &Connection,
) -> Result<HashSet<i64>, DbError> {
    Ok(connection
        .prepare_cached("SELECT track_id FROM user_artwork WHERE dirty_on_disk = 1")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?)
}

/// Stores the picture a rewritten file now holds and whether that is the user's picture.
pub(crate) fn record_file_artwork_in_tx(
    tx: &Transaction<'_>,
    asset_id: i64,
    track_id: i64,
    artwork: Option<&StoredArtwork>,
) -> Result<(), DbError> {
    if let Some(artwork) = artwork {
        insert_artwork(tx, artwork)?;
    }
    let hash = artwork.map(|artwork| artwork.hash.as_str());
    tx.prepare_cached("UPDATE asset SET artwork_hash = ?2, artwork_read = 1 WHERE id = ?1")?
        .execute(params![asset_id, hash])?;
    tx.prepare_cached(
        "UPDATE user_artwork SET dirty_on_disk = (artwork_hash IS NOT ?2) WHERE track_id = ?1",
    )?
    .execute(params![track_id, hash])?;
    Ok(())
}

/// Puts the user's picture in place of the file's on rows read from the asset table.
pub(crate) fn populate_user_artwork(
    connection: &Connection,
    tracks: &mut [TrackRow],
) -> Result<(), DbError> {
    if tracks.is_empty() {
        return Ok(());
    }
    let any: bool = connection
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM user_artwork)")?
        .query_row([], |row| row.get(0))?;
    if !any {
        return Ok(());
    }
    let track_ids: Vec<i64> = tracks.iter().map(|track| track.track_id).collect();
    let user_hashes = user_cover_hashes(connection, &track_ids)?;
    for track in tracks {
        if let Some(hash) = user_hashes.get(&track.track_id) {
            track.artwork = Some(hash.clone());
        }
    }
    Ok(())
}

/// Each track's own chosen cover, for the tracks in `track_ids` that have one.
fn user_cover_hashes(
    connection: &Connection,
    track_ids: &[i64],
) -> Result<HashMap<i64, String>, DbError> {
    if track_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut unique_ids = track_ids.to_vec();
    unique_ids.sort_unstable();
    unique_ids.dedup();
    let mut out = HashMap::new();
    for chunk in unique_ids.chunks(IN_CHUNK) {
        let in_list = placeholders(chunk.len());
        let sql = format!(
            "SELECT track_id, artwork_hash FROM user_artwork WHERE track_id IN ({in_list})"
        );
        let mut stmt = connection.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
        while let Some(row) = rows.next()? {
            out.insert(row.get(0)?, row.get(1)?);
        }
    }
    Ok(out)
}

impl Database {
    /// Where this library keeps its pictures: beside the database file, or nowhere for a
    /// database in memory that was not given a folder.
    pub fn artwork_store(&self) -> Option<ArtworkStore> {
        self.artwork.clone()
    }

    pub fn set_artwork_folder(&mut self, folder: impl Into<PathBuf>) {
        self.artwork = Some(ArtworkStore::new(folder));
    }

    pub fn artwork(&self, hash: &str) -> Result<Option<StoredArtwork>, DbError> {
        stored_artwork(&self.connection, hash)
    }

    /// The paths in a source whose pictures have not been read yet.
    pub fn paths_without_artwork_read(&self, source: SourceId) -> Result<HashSet<String>, DbError> {
        Ok(self
            .connection
            .prepare_cached("SELECT path FROM asset WHERE source_id = ?1 AND artwork_read = 0")?
            .query_map([source.0], |row| row.get(0))?
            .collect::<Result<_, _>>()?)
    }

    /// Stores the front cover each scanned file holds, `None` for a file with none.
    pub fn record_scanned_artwork(
        &mut self,
        source: SourceId,
        files: &[(String, Option<StoredArtwork>)],
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        for (path, artwork) in files {
            if let Some(artwork) = artwork {
                insert_artwork(&tx, artwork)?;
            }
            tx.prepare_cached(
                "UPDATE asset SET artwork_hash = ?3, artwork_read = 1
                 WHERE source_id = ?1 AND path = ?2",
            )?
            .execute(params![
                source.0,
                path,
                artwork.as_ref().map(|artwork| artwork.hash.as_str())
            ])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The picture the track shows: the user's, else its primary file's.
    pub fn track_artwork(&self, track_id: i64) -> Result<Option<StoredArtwork>, DbError> {
        let user: Option<String> = self
            .connection
            .prepare_cached("SELECT artwork_hash FROM user_artwork WHERE track_id = ?1")?
            .query_row([track_id], |row| row.get(0))
            .optional()?;
        let hash = match user {
            Some(hash) => Some(hash),
            None => match primary_asset(&self.connection, track_id)? {
                Some(asset_id) => self
                    .connection
                    .prepare_cached("SELECT artwork_hash FROM asset WHERE id = ?1")?
                    .query_row([asset_id], |row| row.get(0))?,
                None => None,
            },
        };
        match hash {
            Some(hash) => stored_artwork(&self.connection, &hash),
            None => Ok(None),
        }
    }

    /// Makes `artwork` the track's picture in the user layer. Returns false for a track that
    /// does not exist.
    pub fn set_user_artwork(
        &mut self,
        track_id: i64,
        artwork: &StoredArtwork,
    ) -> Result<bool, DbError> {
        let tx = self.connection.transaction()?;
        let set = set_user_artwork_in_tx(&tx, track_id, artwork)?;
        tx.commit()?;
        Ok(set)
    }

    pub fn proposed_artwork(&self, track_id: i64) -> Result<Option<ProposedArtwork>, DbError> {
        let row: Option<(String, String, String, String)> = self
            .connection
            .prepare_cached(
                "SELECT artwork_hash, provider, source_url, status FROM proposed_artwork
                 WHERE track_id = ?1",
            )?
            .query_row([track_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .optional()?;
        let Some((hash, provider, source_url, status)) = row else {
            return Ok(None);
        };
        Ok(
            stored_artwork(&self.connection, &hash)?.map(|artwork| ProposedArtwork {
                artwork,
                provider,
                source_url,
                pending: status == "pending",
            }),
        )
    }

    /// Cached cover outcome for `source_url`, returning `Unavailable` only when a failed/refused
    /// fetch was recorded within `max_negative_age`. Falls back to `proposed_artwork` for rows
    /// created before `artwork_source` existed.
    pub fn cached_artwork_source(
        &self,
        source_url: &str,
        max_negative_age: Duration,
    ) -> Result<Option<CachedArtworkSource>, DbError> {
        let cached: Option<(Option<String>, i64)> = self
            .connection
            .prepare_cached(
                "SELECT artwork_hash, fetched_ns FROM artwork_source WHERE source_url = ?1",
            )?
            .query_row([source_url], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?;
        match cached {
            Some((Some(hash), _)) => {
                if let Some(artwork) = stored_artwork(&self.connection, &hash)? {
                    return Ok(Some(CachedArtworkSource::Stored(artwork)));
                }
            }
            Some((None, fetched_ns)) => {
                let max_ns = i64::try_from(max_negative_age.as_nanos()).unwrap_or(i64::MAX);
                let fresh_since = now_unix_ns().saturating_sub(max_ns);
                if !max_negative_age.is_zero() && fetched_ns >= fresh_since {
                    return Ok(Some(CachedArtworkSource::Unavailable));
                }
            }
            None => {}
        }
        let hash: Option<String> = self
            .connection
            .prepare_cached("SELECT artwork_hash FROM proposed_artwork WHERE source_url = ?1")?
            .query_row([source_url], |row| row.get(0))
            .optional()?;
        match hash {
            Some(hash) => {
                Ok(stored_artwork(&self.connection, &hash)?.map(CachedArtworkSource::Stored))
            }
            None => Ok(None),
        }
    }

    /// Single-track artwork state using cached queries.
    pub fn track_artwork_state(&self, track_id: i64) -> Result<TrackArtworkState, DbError> {
        let user_row: Option<(String, bool)> = self
            .connection
            .prepare_cached(
                "SELECT artwork_hash, dirty_on_disk FROM user_artwork WHERE track_id = ?1",
            )?
            .query_row([track_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?;
        let dirty_on_disk = user_row.as_ref().is_some_and(|(_, dirty)| *dirty);
        let shown_hash = match user_row {
            Some((hash, _)) => Some(hash),
            None => self
                .connection
                .prepare_cached(
                    "SELECT artwork_hash FROM asset
                     WHERE track_id = ?1
                     ORDER BY included DESC, is_primary DESC, id ASC LIMIT 1",
                )?
                .query_row([track_id], |row| row.get::<_, Option<String>>(0))
                .optional()?
                .flatten(),
        };
        let shown = match shown_hash {
            Some(hash) => stored_artwork(&self.connection, &hash)?,
            None => None,
        };
        let proposed = self.proposed_artwork(track_id)?;
        Ok(TrackArtworkState {
            shown,
            proposed,
            dirty_on_disk,
        })
    }

    /// Stored `user_artwork.artwork_hash` for each track in `track_ids` that has one.
    pub fn user_cover_hashes_by_track_ids(
        &self,
        track_ids: &[i64],
    ) -> Result<HashMap<i64, String>, DbError> {
        user_cover_hashes(&self.connection, track_ids)
    }

    /// Batch artwork state (`track_artwork`, `proposed_artwork`, `has_unwritten_artwork`) for
    /// `track_ids`, loaded in chunked `IN` queries.
    pub fn artwork_states_by_track_ids(
        &self,
        track_ids: &[i64],
    ) -> Result<HashMap<i64, TrackArtworkState>, DbError> {
        if track_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut unique_ids = track_ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();

        let mut user_by_track: HashMap<i64, (String, bool)> = HashMap::new();
        let mut proposed_raw_by_track: HashMap<i64, (String, String, String, bool)> =
            HashMap::new();
        let mut primary_hash_by_track: HashMap<i64, String> = HashMap::new();

        for chunk in unique_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());

            let user_sql = format!(
                "SELECT track_id, artwork_hash, dirty_on_disk
                 FROM user_artwork WHERE track_id IN ({in_list})"
            );
            let mut user_stmt = self.connection.prepare(&user_sql)?;
            let mut user_rows =
                user_stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = user_rows.next()? {
                user_by_track.insert(row.get(0)?, (row.get(1)?, row.get(2)?));
            }

            let asset_sql = format!(
                "SELECT track_id, artwork_hash
                 FROM asset WHERE track_id IN ({in_list})
                 ORDER BY track_id ASC, included DESC, is_primary DESC, id ASC"
            );
            let mut asset_stmt = self.connection.prepare(&asset_sql)?;
            let mut asset_rows =
                asset_stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            let mut prev_track: Option<i64> = None;
            while let Some(row) = asset_rows.next()? {
                let track_id: i64 = row.get(0)?;
                if prev_track != Some(track_id) {
                    prev_track = Some(track_id);
                    if let Some(hash) = row.get::<_, Option<String>>(1)? {
                        primary_hash_by_track.insert(track_id, hash);
                    }
                }
            }

            let prop_sql = format!(
                "SELECT track_id, artwork_hash, provider, source_url, status
                 FROM proposed_artwork WHERE track_id IN ({in_list})"
            );
            let mut prop_stmt = self.connection.prepare(&prop_sql)?;
            let mut prop_rows =
                prop_stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = prop_rows.next()? {
                let status: String = row.get(4)?;
                proposed_raw_by_track.insert(
                    row.get(0)?,
                    (row.get(1)?, row.get(2)?, row.get(3)?, status == "pending"),
                );
            }
        }

        let mut needed_hashes: Vec<String> = Vec::new();
        for track_id in &unique_ids {
            if let Some((hash, _)) = user_by_track.get(track_id) {
                needed_hashes.push(hash.clone());
            } else if let Some(hash) = primary_hash_by_track.get(track_id) {
                needed_hashes.push(hash.clone());
            }
            if let Some((hash, _, _, _)) = proposed_raw_by_track.get(track_id) {
                needed_hashes.push(hash.clone());
            }
        }
        needed_hashes.sort_unstable();
        needed_hashes.dedup();

        let mut stored_by_hash: HashMap<String, StoredArtwork> =
            HashMap::with_capacity(needed_hashes.len());
        for chunk in needed_hashes.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!("SELECT hash, mime, byte_len FROM artwork WHERE hash IN ({in_list})");
            let mut stmt = self.connection.prepare(&sql)?;
            let mut rows =
                stmt.query(rusqlite::params_from_iter(chunk.iter().map(String::as_str)))?;
            while let Some(row) = rows.next()? {
                let hash: String = row.get(0)?;
                let mime: String = row.get(1)?;
                let byte_len: u64 = row.get(2)?;
                let kind = ImageKind::from_mime(&mime).ok_or(DbError::UnknownName {
                    column: "artwork.mime",
                    name: mime,
                })?;
                stored_by_hash.insert(
                    hash.clone(),
                    StoredArtwork {
                        hash,
                        kind,
                        byte_len,
                    },
                );
            }
        }

        let mut out = HashMap::with_capacity(unique_ids.len());
        for track_id in unique_ids {
            let user_entry = user_by_track.remove(&track_id);
            let dirty_on_disk = user_entry.as_ref().is_some_and(|(_, dirty)| *dirty);
            let shown_hash = user_entry
                .map(|(hash, _)| hash)
                .or_else(|| primary_hash_by_track.remove(&track_id));
            let shown = shown_hash.and_then(|hash| stored_by_hash.get(&hash).cloned());
            let proposed = proposed_raw_by_track.remove(&track_id).and_then(
                |(hash, provider, source_url, pending)| {
                    stored_by_hash
                        .get(&hash)
                        .cloned()
                        .map(|artwork| ProposedArtwork {
                            artwork,
                            provider,
                            source_url,
                            pending,
                        })
                },
            );
            out.insert(
                track_id,
                TrackArtworkState {
                    shown,
                    proposed,
                    dirty_on_disk,
                },
            );
        }
        Ok(out)
    }

    /// Cached cover outcome for every address in `source_urls`, resolved in chunked queries.
    pub fn cached_artwork_sources(
        &self,
        source_urls: &[&str],
        max_negative_age: Duration,
    ) -> Result<HashMap<String, CachedArtworkSource>, DbError> {
        if source_urls.is_empty() {
            return Ok(HashMap::new());
        }
        let mut unique_urls: Vec<&str> = source_urls.to_vec();
        unique_urls.sort_unstable();
        unique_urls.dedup();

        let max_ns = i64::try_from(max_negative_age.as_nanos()).unwrap_or(i64::MAX);
        let fresh_since = now_unix_ns().saturating_sub(max_ns);
        let mut resolved: HashMap<String, CachedArtworkSource> =
            HashMap::with_capacity(unique_urls.len());
        let mut fallback_urls: Vec<&str> = Vec::new();

        for chunk in unique_urls.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!(
                "SELECT s.source_url, s.artwork_hash, s.fetched_ns, a.mime, a.byte_len
                 FROM artwork_source s
                 LEFT JOIN artwork a ON a.hash = s.artwork_hash
                 WHERE s.source_url IN ({in_list})"
            );
            let mut stmt = self.connection.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = rows.next()? {
                let url: String = row.get(0)?;
                let hash: Option<String> = row.get(1)?;
                let fetched_ns: i64 = row.get(2)?;
                let mime: Option<String> = row.get(3)?;
                let byte_len: Option<u64> = row.get(4)?;
                match (hash, mime, byte_len) {
                    (Some(hash), Some(mime), Some(byte_len)) => {
                        let kind = ImageKind::from_mime(&mime).ok_or(DbError::UnknownName {
                            column: "artwork.mime",
                            name: mime,
                        })?;
                        resolved.insert(
                            url,
                            CachedArtworkSource::Stored(StoredArtwork {
                                hash,
                                kind,
                                byte_len,
                            }),
                        );
                    }
                    (None, _, _) if !max_negative_age.is_zero() && fetched_ns >= fresh_since => {
                        resolved.insert(url, CachedArtworkSource::Unavailable);
                    }
                    _ => {}
                }
            }
            for &url in chunk {
                if !resolved.contains_key(url) {
                    fallback_urls.push(url);
                }
            }
        }

        if !fallback_urls.is_empty() {
            for chunk in fallback_urls.chunks(IN_CHUNK) {
                let in_list = placeholders(chunk.len());
                let sql = format!(
                    "SELECT p.source_url, p.artwork_hash, a.mime, a.byte_len
                     FROM proposed_artwork p
                     JOIN artwork a ON a.hash = p.artwork_hash
                     WHERE p.source_url IN ({in_list})"
                );
                let mut stmt = self.connection.prepare(&sql)?;
                let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
                while let Some(row) = rows.next()? {
                    let url: String = row.get(0)?;
                    let hash: String = row.get(1)?;
                    let mime: String = row.get(2)?;
                    let byte_len: u64 = row.get(3)?;
                    let kind = ImageKind::from_mime(&mime).ok_or(DbError::UnknownName {
                        column: "artwork.mime",
                        name: mime,
                    })?;
                    resolved.insert(
                        url,
                        CachedArtworkSource::Stored(StoredArtwork {
                            hash,
                            kind,
                            byte_len,
                        }),
                    );
                }
            }
        }

        Ok(resolved)
    }

    /// Records the result of fetching `source_url` into `artwork_source` (`None` marks 404 or
    /// unsupported bytes so candidate previews do not keep retrying the same dead URL).
    pub fn record_fetched_artwork(
        &mut self,
        source_url: &str,
        artwork: Option<&StoredArtwork>,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        if let Some(artwork) = artwork {
            insert_artwork(&tx, artwork)?;
        }
        record_artwork_source_in_tx(&tx, source_url, artwork.map(|art| art.hash.as_str()))?;
        tx.commit()?;
        Ok(())
    }

    /// Stages a release picture for the track. A decision already taken on the same picture is
    /// kept; a different picture is pending again.
    pub fn store_proposed_artwork(
        &mut self,
        track_id: i64,
        artwork: &StoredArtwork,
        provider: &str,
        source_url: &str,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        insert_artwork(&tx, artwork)?;
        tx.prepare_cached(
            "INSERT INTO proposed_artwork
                 (track_id, artwork_hash, provider, source_url, status, updated_ns)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5)
             ON CONFLICT (track_id) DO UPDATE SET
                 status = CASE
                     WHEN proposed_artwork.artwork_hash = excluded.artwork_hash
                         THEN proposed_artwork.status
                     ELSE 'pending'
                 END,
                 artwork_hash = excluded.artwork_hash,
                 provider = excluded.provider,
                 source_url = excluded.source_url,
                 updated_ns = excluded.updated_ns",
        )?
        .execute(params![
            track_id,
            artwork.hash,
            provider,
            source_url,
            now_unix_ns()
        ])?;
        record_artwork_source_in_tx(&tx, source_url, Some(&artwork.hash))?;
        tx.commit()?;
        Ok(())
    }

    /// Removes a pending cover proposal without marking an existing hash as user-skipped.
    pub fn clear_pending_proposed_artwork(&mut self, track_id: i64) -> Result<(), DbError> {
        self.connection.execute(
            "DELETE FROM proposed_artwork WHERE track_id = ?1 AND status = 'pending'",
            [track_id],
        )?;
        Ok(())
    }

    /// Copies the pending proposal into the user layer. Returns false when there was none.
    pub fn accept_proposed_artwork(&mut self, track_id: i64) -> Result<bool, DbError> {
        let tx = self.connection.transaction()?;
        let accepted = accept_proposed_artwork_in_tx(&tx, track_id)?;
        if !accepted {
            return Ok(false);
        }
        tx.commit()?;
        Ok(true)
    }

    pub fn skip_proposed_artwork(&mut self, track_id: i64) -> Result<bool, DbError> {
        let tx = self.connection.transaction()?;
        let skipped = skip_proposed_artwork_in_tx(&tx, track_id)?;
        tx.commit()?;
        Ok(skipped)
    }

    pub fn has_unwritten_artwork(&self, track_id: i64) -> Result<bool, DbError> {
        let dirty: Option<i64> = self
            .connection
            .prepare_cached(
                "SELECT 1 FROM user_artwork WHERE track_id = ?1 AND dirty_on_disk = 1 LIMIT 1",
            )?
            .query_row([track_id], |row| row.get(0))
            .optional()?;
        Ok(dirty.is_some())
    }
}

pub(crate) fn accept_proposed_artwork_in_tx(
    tx: &Transaction<'_>,
    track_id: i64,
) -> Result<bool, DbError> {
    let hash: Option<String> = tx
        .prepare_cached(
            "SELECT artwork_hash FROM proposed_artwork
             WHERE track_id = ?1 AND status = 'pending'",
        )?
        .query_row([track_id], |row| row.get(0))
        .optional()?;
    let Some(artwork) = hash
        .map(|hash| stored_artwork(tx, &hash))
        .transpose()?
        .flatten()
    else {
        return Ok(false);
    };
    set_user_artwork_in_tx(tx, track_id, &artwork)?;
    decide(tx, track_id, "accepted")?;
    Ok(true)
}

pub(crate) fn skip_proposed_artwork_in_tx(
    tx: &Transaction<'_>,
    track_id: i64,
) -> Result<bool, DbError> {
    decide(tx, track_id, "skipped")
}

fn record_artwork_source_in_tx(
    tx: &Transaction<'_>,
    source_url: &str,
    hash: Option<&str>,
) -> Result<(), DbError> {
    tx.prepare_cached(
        "INSERT INTO artwork_source (source_url, artwork_hash, fetched_ns)
         VALUES (?1, ?2, ?3)
         ON CONFLICT (source_url) DO UPDATE SET
             artwork_hash = excluded.artwork_hash,
             fetched_ns = excluded.fetched_ns",
    )?
    .execute(params![source_url, hash, now_unix_ns()])?;
    Ok(())
}

fn decide(tx: &Transaction<'_>, track_id: i64, status: &str) -> Result<bool, DbError> {
    Ok(tx
        .prepare_cached(
            "UPDATE proposed_artwork SET status = ?2, updated_ns = ?3
             WHERE track_id = ?1 AND status = 'pending'",
        )?
        .execute(params![track_id, status, now_unix_ns()])?
        > 0)
}

fn set_user_artwork_in_tx(
    tx: &Transaction<'_>,
    track_id: i64,
    artwork: &StoredArtwork,
) -> Result<bool, DbError> {
    let exists: bool = tx
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM track WHERE id = ?1)")?
        .query_row([track_id], |row| row.get(0))?;
    if !exists {
        return Ok(false);
    }
    insert_artwork(tx, artwork)?;
    let in_file: Option<String> = match primary_asset(tx, track_id)? {
        Some(asset_id) => tx
            .prepare_cached("SELECT artwork_hash FROM asset WHERE id = ?1")?
            .query_row([asset_id], |row| row.get(0))?,
        None => None,
    };
    let dirty = in_file.as_deref() != Some(artwork.hash.as_str());
    tx.prepare_cached(
        "INSERT INTO user_artwork (track_id, artwork_hash, dirty_on_disk, updated_ns)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (track_id) DO UPDATE SET
             artwork_hash = excluded.artwork_hash,
             dirty_on_disk = excluded.dirty_on_disk,
             updated_ns = excluded.updated_ns",
    )?
    .execute(params![track_id, artwork.hash, dirty, now_unix_ns()])?;
    Ok(true)
}
