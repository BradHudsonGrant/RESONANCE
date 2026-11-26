//! Other DJ apps' libraries: the collections that mirror their playlists, and the path changes
//! waiting to be written into them.

use resonance_core::collections::normalise_collection_name;
use resonance_core::playlist::{PlaylistTrack, normalise_path, path_inside, same_path};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use crate::{CollectionSummary, Database, DbError, search};

/// A library the user set up, with the collection that mirrors it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredExternalLibrary {
    pub id: i64,
    pub kind: String,
    pub location: String,
    pub collection_id: Option<i64>,
    pub read_ns: i64,
}

/// What reading a playlist into its mirror found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorReport {
    pub library: StoredExternalLibrary,
    pub collection: CollectionSummary,
    /// Entries that matched a track in the library.
    pub matched: u32,
    /// Entries naming files the library does not hold.
    pub not_in_library: u32,
}

/// A path change a library is waiting for, with what the track now shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingExternalChange {
    pub id: i64,
    pub library_id: i64,
    pub track_id: i64,
    pub old_path: String,
    pub new_path: String,
    pub title: String,
    pub duration_s: Option<u32>,
}

fn stored_library(row: &Row<'_>) -> rusqlite::Result<StoredExternalLibrary> {
    Ok(StoredExternalLibrary {
        id: row.get(0)?,
        kind: row.get(1)?,
        location: row.get(2)?,
        collection_id: row.get(3)?,
        read_ns: row.get(4)?,
    })
}

const SELECT_LIBRARY: &str = "SELECT l.id, l.kind, l.location, c.id, l.read_ns
    FROM external_library l
    LEFT JOIN collection c ON c.external_library_id = l.id";

/// The full path of an asset, with forward slashes.
pub(crate) fn asset_full_path(
    connection: &Connection,
    asset_id: i64,
) -> Result<Option<String>, DbError> {
    Ok(connection
        .prepare_cached(
            "SELECT source.folder, asset.path FROM asset
             JOIN source ON source.id = asset.source_id
             WHERE asset.id = ?1",
        )?
        .query_row([asset_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .optional()?
        .map(|(folder, path)| normalise_path(&format!("{folder}/{path}"))))
}

/// Queues `old_path` to `new_path` for every library whose mirror holds the track. A change
/// already waiting for the track moves on to the new path, and one that comes back to where the
/// library already is goes away.
pub(crate) fn queue_path_change(
    tx: &Transaction<'_>,
    track_id: i64,
    old_path: &str,
    new_path: &str,
) -> Result<(), DbError> {
    if same_path(old_path, new_path) {
        return Ok(());
    }
    let libraries: Vec<i64> = tx
        .prepare_cached(
            "SELECT c.external_library_id FROM collection c
             JOIN collection_item ci ON ci.collection_id = c.id
             WHERE ci.track_id = ?1 AND c.external_library_id IS NOT NULL",
        )?
        .query_map([track_id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    let queued_ns = search::now_unix_ns();
    for library_id in libraries {
        let waiting: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, old_path FROM pending_external_change
                 WHERE external_library_id = ?1 AND track_id = ?2",
                params![library_id, track_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match waiting {
            Some((id, known)) if same_path(&known, new_path) => {
                tx.execute("DELETE FROM pending_external_change WHERE id = ?1", [id])?;
            }
            Some((id, _)) => {
                tx.execute(
                    "UPDATE pending_external_change SET new_path = ?2, queued_ns = ?3
                     WHERE id = ?1",
                    params![id, new_path, queued_ns],
                )?;
            }
            None => {
                tx.execute(
                    "INSERT INTO pending_external_change
                         (external_library_id, track_id, old_path, new_path, queued_ns)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![library_id, track_id, old_path, new_path, queued_ns],
                )?;
            }
        }
    }
    Ok(())
}

/// Whether the collection mirrors another app's playlist, which only that playlist changes.
pub(crate) fn is_mirror(connection: &Connection, collection_id: i64) -> Result<bool, DbError> {
    Ok(connection
        .query_row(
            "SELECT external_library_id IS NOT NULL FROM collection WHERE id = ?1",
            [collection_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(false))
}

/// The track a playlist entry names: a file in a source, or a file the library waits to be
/// told the new path of.
fn track_for_entry(
    tx: &Transaction<'_>,
    sources: &[(i64, String)],
    waiting: &[(i64, String)],
    path: &str,
) -> Result<Option<i64>, DbError> {
    for (source_id, folder) in sources {
        let Some(relative) = path_inside(folder, path) else {
            continue;
        };
        let exact = tx
            .prepare_cached(
                "SELECT track_id FROM asset
                 WHERE source_id = ?1 AND path = ?2 AND included = 1",
            )?
            .query_row(params![source_id, relative], |row| row.get(0))
            .optional()?;
        if exact.is_some() {
            return Ok(exact);
        }
        if cfg!(any(windows, target_os = "macos")) {
            let folded = tx
                .prepare_cached(
                    "SELECT track_id FROM asset
                     WHERE source_id = ?1 AND path = ?2 COLLATE NOCASE AND included = 1
                     LIMIT 1",
                )?
                .query_row(params![source_id, relative], |row| row.get(0))
                .optional()?;
            if folded.is_some() {
                return Ok(folded);
            }
        }
    }
    Ok(waiting
        .iter()
        .find(|(_, old_path)| same_path(old_path, path))
        .map(|(track_id, _)| *track_id))
}

/// `base`, or `base 2`, `base 3` and on, whichever no collection has yet.
fn free_collection_name(tx: &Transaction<'_>, base: &str) -> Result<String, DbError> {
    let base = normalise_collection_name(base).unwrap_or_else(|_| "Playlist".to_owned());
    let mut name = base.clone();
    let mut number = 2;
    loop {
        let taken: Option<i64> = tx
            .query_row(
                "SELECT id FROM collection WHERE name = ?1",
                [&name],
                |row| row.get(0),
            )
            .optional()?;
        if taken.is_none() {
            return Ok(name);
        }
        name = format!("{base} {number}");
        number += 1;
    }
}

impl Database {
    pub fn external_libraries(&self) -> Result<Vec<StoredExternalLibrary>, DbError> {
        let mut statement = self
            .connection
            .prepare(&format!("{SELECT_LIBRARY} ORDER BY l.id"))?;
        let rows = statement
            .query_map([], stored_library)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn external_library(&self, id: i64) -> Result<Option<StoredExternalLibrary>, DbError> {
        Ok(self
            .connection
            .query_row(
                &format!("{SELECT_LIBRARY} WHERE l.id = ?1"),
                [id],
                stored_library,
            )
            .optional()?)
    }

    /// The library a mirror collection follows.
    pub fn external_library_of_collection(
        &self,
        collection_id: i64,
    ) -> Result<Option<StoredExternalLibrary>, DbError> {
        Ok(self
            .connection
            .query_row(
                &format!("{SELECT_LIBRARY} WHERE c.id = ?1"),
                [collection_id],
                stored_library,
            )
            .optional()?)
    }

    /// Sets up the library at `location`, or reads it again, and makes its mirror collection
    /// hold the tracks its entries name, in their order. `entries` are full paths.
    pub fn mirror_external_library(
        &mut self,
        kind: &str,
        location: &str,
        name: &str,
        entries: &[String],
    ) -> Result<MirrorReport, DbError> {
        let tx = self.connection.transaction()?;
        let now = search::now_unix_ns();
        let known: Option<i64> = tx
            .query_row(
                "SELECT id FROM external_library WHERE location = ?1",
                [location],
                |row| row.get(0),
            )
            .optional()?;
        let library_id = match known {
            Some(id) => {
                tx.execute(
                    "UPDATE external_library SET read_ns = ?2 WHERE id = ?1",
                    params![id, now],
                )?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO external_library (kind, location, added_ns, read_ns)
                     VALUES (?1, ?2, ?3, ?3)",
                    params![kind, location, now],
                )?;
                tx.last_insert_rowid()
            }
        };
        let mirror: Option<i64> = tx
            .query_row(
                "SELECT id FROM collection WHERE external_library_id = ?1",
                [library_id],
                |row| row.get(0),
            )
            .optional()?;
        let collection_id = match mirror {
            Some(id) => id,
            None => {
                let name = free_collection_name(&tx, name)?;
                tx.execute(
                    "INSERT INTO collection (name, position, created_ns, external_library_id)
                     VALUES (?1, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection),
                         ?2, ?3)",
                    params![name, now, library_id],
                )?;
                tx.last_insert_rowid()
            }
        };

        let sources: Vec<(i64, String)> = tx
            .prepare("SELECT id, folder FROM source ORDER BY length(folder) DESC")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let waiting: Vec<(i64, String)> = tx
            .prepare(
                "SELECT track_id, old_path FROM pending_external_change
                 WHERE external_library_id = ?1",
            )?
            .query_map([library_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        tx.execute(
            "DELETE FROM collection_item WHERE collection_id = ?1",
            [collection_id],
        )?;
        let mut matched = 0;
        let mut not_in_library = 0;
        let mut position = 0i64;
        for entry in entries {
            match track_for_entry(&tx, &sources, &waiting, entry)? {
                Some(track_id) => {
                    matched += 1;
                    let added = tx.execute(
                        "INSERT OR IGNORE INTO collection_item
                             (collection_id, track_id, position, added_ns)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![collection_id, track_id, position, now],
                    )?;
                    position += i64::try_from(added).unwrap_or(0);
                }
                None => not_in_library += 1,
            }
        }
        tx.commit()?;
        let library = self
            .external_library(library_id)?
            .ok_or(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        let collection = self
            .collection(collection_id)?
            .ok_or(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        Ok(MirrorReport {
            library,
            collection,
            matched,
            not_in_library,
        })
    }

    /// Stops following a library. Its mirror and waiting changes go; its files stay.
    pub fn remove_external_library(&mut self, id: i64) -> Result<bool, DbError> {
        Ok(self
            .connection
            .execute("DELETE FROM external_library WHERE id = ?1", [id])?
            > 0)
    }

    /// Every change waiting, oldest library first and in the order the tracks were queued.
    pub fn pending_external_changes(&self) -> Result<Vec<PendingExternalChange>, DbError> {
        let mut statement = self.connection.prepare(
            "SELECT p.id, p.external_library_id, p.track_id, p.old_path, p.new_path,
                 asset.sort_artist, asset.sort_title, asset.duration_ms
             FROM pending_external_change p
             LEFT JOIN asset ON asset.id = (
                 SELECT id FROM asset WHERE track_id = p.track_id
                 ORDER BY is_primary DESC, included DESC, id LIMIT 1
             )
             ORDER BY p.external_library_id, p.queued_ns, p.id",
        )?;
        let rows = statement
            .query_map([], |row| {
                let artist: Option<String> = row.get(5)?;
                let title: Option<String> = row.get(6)?;
                let duration_ms: Option<i64> = row.get(7)?;
                Ok(PendingExternalChange {
                    id: row.get(0)?,
                    library_id: row.get(1)?,
                    track_id: row.get(2)?,
                    old_path: row.get(3)?,
                    new_path: row.get(4)?,
                    title: shown_title(artist.as_deref(), title.as_deref().unwrap_or_default()),
                    duration_s: duration_ms.and_then(|ms| u32::try_from((ms + 500) / 1000).ok()),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Drops changes the library now holds.
    pub fn clear_external_changes(&mut self, ids: &[i64]) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        for id in ids {
            tx.execute("DELETE FROM pending_external_change WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// A collection's tracks in order, as a new playlist names them: each track's primary file.
    pub fn playlist_tracks(&self, collection_id: i64) -> Result<Vec<PlaylistTrack>, DbError> {
        let mut statement = self.connection.prepare(
            "SELECT source.folder, asset.path, asset.sort_artist, asset.sort_title,
                 asset.duration_ms
             FROM collection_item ci
             JOIN asset ON asset.track_id = ci.track_id AND asset.is_primary = 1
                 AND asset.included = 1
             JOIN source ON source.id = asset.source_id
             WHERE ci.collection_id = ?1
             ORDER BY ci.position, ci.track_id",
        )?;
        let rows = statement
            .query_map([collection_id], |row| {
                let folder: String = row.get(0)?;
                let path: String = row.get(1)?;
                let artist: Option<String> = row.get(2)?;
                let title: String = row.get(3)?;
                let duration_ms: Option<i64> = row.get(4)?;
                Ok(PlaylistTrack {
                    path: normalise_path(&format!("{folder}/{path}")),
                    title: shown_title(artist.as_deref(), &title),
                    duration_s: duration_ms.and_then(|ms| u32::try_from((ms + 500) / 1000).ok()),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

/// `Artist - Title`, the way extended playlists show a track.
fn shown_title(artist: Option<&str>, title: &str) -> String {
    match artist.filter(|artist| !artist.is_empty()) {
        Some(artist) => format!("{artist} - {title}"),
        None => title.to_owned(),
    }
}
