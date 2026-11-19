//! Cover pictures: the folder that keeps each picture once, and the file, user and proposed
//! layers that say which picture a track shows.
//!
//! The track shows the user's picture when there is one, else the one in its primary file.
//! Identification only ever proposes; a proposal reaches the user layer when it is accepted, and
//! a file only when the user writes tags.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
    let any: bool = connection
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM user_artwork)")?
        .query_row([], |row| row.get(0))?;
    if !any {
        return Ok(());
    }
    let mut chosen =
        connection.prepare_cached("SELECT artwork_hash FROM user_artwork WHERE track_id = ?1")?;
    for track in tracks {
        if let Some(hash) = chosen
            .query_row([track.track_id], |row| row.get::<_, String>(0))
            .optional()?
        {
            track.artwork = Some(hash);
        }
    }
    Ok(())
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

    /// A picture already fetched from this address, so a release's tracks fetch it once.
    pub fn artwork_fetched_from(&self, source_url: &str) -> Result<Option<StoredArtwork>, DbError> {
        let hash: Option<String> = self
            .connection
            .prepare_cached("SELECT artwork_hash FROM proposed_artwork WHERE source_url = ?1")?
            .query_row([source_url], |row| row.get(0))
            .optional()?;
        match hash {
            Some(hash) => stored_artwork(&self.connection, &hash),
            None => Ok(None),
        }
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
        tx.commit()?;
        Ok(())
    }

    /// Copies the pending proposal into the user layer. Returns false when there was none.
    pub fn accept_proposed_artwork(&mut self, track_id: i64) -> Result<bool, DbError> {
        let tx = self.connection.transaction()?;
        let hash: Option<String> = tx
            .prepare_cached(
                "SELECT artwork_hash FROM proposed_artwork
                 WHERE track_id = ?1 AND status = 'pending'",
            )?
            .query_row([track_id], |row| row.get(0))
            .optional()?;
        let Some(artwork) = hash
            .map(|hash| stored_artwork(&tx, &hash))
            .transpose()?
            .flatten()
        else {
            return Ok(false);
        };
        set_user_artwork_in_tx(&tx, track_id, &artwork)?;
        decide(&tx, track_id, "accepted")?;
        tx.commit()?;
        Ok(true)
    }

    pub fn skip_proposed_artwork(&mut self, track_id: i64) -> Result<bool, DbError> {
        let tx = self.connection.transaction()?;
        let skipped = decide(&tx, track_id, "skipped")?;
        tx.commit()?;
        Ok(skipped)
    }
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
