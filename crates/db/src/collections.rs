use std::collections::{HashMap, HashSet};

use resonance_core::collections::normalise_collection_name;
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};

use crate::external::is_mirror;
use crate::{Database, DbError, TrackRow, search};

/// A manual collection as the sidebar lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionSummary {
    pub id: i64,
    pub name: String,
    pub position: i64,
    pub track_count: u32,
    /// The other app's library this collection mirrors, which makes it read-only.
    pub external_library_id: Option<i64>,
}

/// A collection a track belongs to, as shown on track rows and in the inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackCollection {
    pub id: i64,
    pub name: String,
}

const SELECT_COLLECTION: &str = "SELECT
        c.id,
        c.name,
        c.position,
        (
            SELECT count(*)
            FROM collection_item ci
            WHERE ci.collection_id = c.id
              AND EXISTS (
                  SELECT 1
                  FROM asset
                  WHERE asset.track_id = ci.track_id
                    AND asset.included = 1
              )
        ) AS track_count,
        c.external_library_id
    FROM collection c";

fn collection_exists(connection: &Connection, collection_id: i64) -> Result<bool, DbError> {
    let found: Option<i64> = connection
        .query_row(
            "SELECT id FROM collection WHERE id = ?1",
            [collection_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

fn collection_summary(row: &Row<'_>) -> rusqlite::Result<CollectionSummary> {
    Ok(CollectionSummary {
        id: row.get(0)?,
        name: row.get(1)?,
        position: row.get(2)?,
        track_count: row.get(3)?,
        external_library_id: row.get(4)?,
    })
}

impl Database {
    /// Every manual collection in display order, with its included track count.
    pub fn collections(&self) -> Result<Vec<CollectionSummary>, DbError> {
        let mut stmt = self.connection.prepare(&format!(
            "{SELECT_COLLECTION} ORDER BY c.position ASC, c.id ASC"
        ))?;
        let rows = stmt
            .query_map([], collection_summary)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// One manual collection by id, with its included track count.
    pub fn collection(&self, collection_id: i64) -> Result<Option<CollectionSummary>, DbError> {
        let mut stmt = self
            .connection
            .prepare(&format!("{SELECT_COLLECTION} WHERE c.id = ?1"))?;
        stmt.query_row([collection_id], collection_summary)
            .optional()
            .map_err(DbError::from)
    }

    /// Creates a new manual collection at the end of the list.
    pub fn create_collection(&mut self, name: &str) -> Result<CollectionSummary, DbError> {
        let cleaned = normalise_collection_name(name)?;
        let tx = self.connection.transaction()?;
        let duplicate: Option<i64> = tx
            .query_row(
                "SELECT id FROM collection WHERE name = ?1",
                [&cleaned],
                |row| row.get(0),
            )
            .optional()?;
        if duplicate.is_some() {
            return Err(DbError::DuplicateCollectionName { name: cleaned });
        }
        let position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM collection",
            [],
            |row| row.get(0),
        )?;
        let created_ns = search::now_unix_ns();
        tx.execute(
            "INSERT INTO collection (name, position, created_ns) VALUES (?1, ?2, ?3)",
            params![&cleaned, position, created_ns],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(CollectionSummary {
            id,
            name: cleaned,
            position,
            track_count: 0,
            external_library_id: None,
        })
    }

    /// Renames a manual collection, returning `None` if no collection has `collection_id`.
    pub fn rename_collection(
        &mut self,
        collection_id: i64,
        name: &str,
    ) -> Result<Option<CollectionSummary>, DbError> {
        let cleaned = normalise_collection_name(name)?;
        let tx = self.connection.transaction()?;
        if !collection_exists(&tx, collection_id)? {
            return Ok(None);
        }
        if is_mirror(&tx, collection_id)? {
            return Err(DbError::MirrorCollection);
        }
        let duplicate: Option<i64> = tx
            .query_row(
                "SELECT id FROM collection WHERE name = ?1 AND id != ?2",
                params![&cleaned, collection_id],
                |row| row.get(0),
            )
            .optional()?;
        if duplicate.is_some() {
            return Err(DbError::DuplicateCollectionName { name: cleaned });
        }
        tx.execute(
            "UPDATE collection SET name = ?2 WHERE id = ?1",
            params![collection_id, &cleaned],
        )?;
        tx.commit()?;
        self.collection(collection_id)
    }

    /// Deletes a collection without modifying tracks or files on disk. Deleting a mirror stops
    /// following its playlist, and the playlist file stays as it is.
    pub fn delete_collection(&mut self, collection_id: i64) -> Result<bool, DbError> {
        let tx = self.connection.transaction()?;
        let library: Option<i64> = tx
            .query_row(
                "SELECT external_library_id FROM collection WHERE id = ?1",
                [collection_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let changed = tx.execute("DELETE FROM collection WHERE id = ?1", [collection_id])?;
        if let Some(library) = library {
            tx.execute("DELETE FROM external_library WHERE id = ?1", [library])?;
        }
        tx.commit()?;
        Ok(changed > 0)
    }

    /// Adds tracks to a manual collection in order, ignoring duplicates idempotently.
    pub fn add_tracks_to_collection(
        &mut self,
        collection_id: i64,
        track_ids: &[i64],
    ) -> Result<Option<CollectionSummary>, DbError> {
        let tx = self.connection.transaction()?;
        if !collection_exists(&tx, collection_id)? {
            return Ok(None);
        }
        if is_mirror(&tx, collection_id)? {
            return Err(DbError::MirrorCollection);
        }

        if !track_ids.is_empty() {
            let mut next_position: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM collection_item WHERE collection_id = ?1",
                [collection_id],
                |row| row.get(0),
            )?;
            let added_ns = search::now_unix_ns();
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO collection_item (collection_id, track_id, position, added_ns)
                 SELECT ?1, id, ?3, ?4
                 FROM track
                 WHERE id = ?2",
            )?;
            for &track_id in track_ids {
                let inserted =
                    insert.execute(params![collection_id, track_id, next_position, added_ns])?;
                if inserted > 0 {
                    next_position += 1;
                }
            }
        }
        tx.commit()?;
        self.collection(collection_id)
    }

    /// Removes tracks from a manual collection without deleting the tracks themselves.
    pub fn remove_tracks_from_collection(
        &mut self,
        collection_id: i64,
        track_ids: &[i64],
    ) -> Result<Option<CollectionSummary>, DbError> {
        let tx = self.connection.transaction()?;
        if !collection_exists(&tx, collection_id)? {
            return Ok(None);
        }
        if is_mirror(&tx, collection_id)? {
            return Err(DbError::MirrorCollection);
        }

        if !track_ids.is_empty() {
            let mut delete = tx.prepare(
                "DELETE FROM collection_item WHERE collection_id = ?1 AND track_id = ?2",
            )?;
            for &track_id in track_ids {
                delete.execute(params![collection_id, track_id])?;
            }
        }
        tx.commit()?;
        self.collection(collection_id)
    }

    /// Collections a single track belongs to, in sidebar display order.
    pub fn collections_for_track(&self, track_id: i64) -> Result<Vec<TrackCollection>, DbError> {
        let mut by_track = collections_by_track_ids(&self.connection, &[track_id])?;
        Ok(by_track.remove(&track_id).unwrap_or_default())
    }
}

pub(crate) fn collections_by_track_ids(
    connection: &Connection,
    track_ids: &[i64],
) -> Result<HashMap<i64, Vec<TrackCollection>>, DbError> {
    if track_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let has_any: bool = connection
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM collection_item LIMIT 1)")?
        .query_row([], |row| row.get(0))?;
    if !has_any {
        return Ok(HashMap::new());
    }

    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for &id in track_ids {
        if seen.insert(id) {
            unique.push(id);
        }
    }

    let mut map: HashMap<i64, Vec<TrackCollection>> = HashMap::new();
    for chunk in unique.chunks(500) {
        let placeholders: Vec<String> = (1..=chunk.len()).map(|idx| format!("?{idx}")).collect();
        let sql = format!(
            "SELECT ci.track_id, c.id, c.name
             FROM collection_item ci
             JOIN collection c ON c.id = ci.collection_id
             WHERE ci.track_id IN ({})
             ORDER BY c.position ASC, c.id ASC",
            placeholders.join(", ")
        );
        let mut stmt = connection.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(chunk.iter().copied()))?;
        while let Some(row) = rows.next()? {
            let track_id: i64 = row.get(0)?;
            map.entry(track_id).or_default().push(TrackCollection {
                id: row.get(1)?,
                name: row.get(2)?,
            });
        }
    }
    Ok(map)
}

pub(crate) fn populate_track_collections(
    connection: &Connection,
    tracks: &mut [TrackRow],
) -> Result<(), DbError> {
    if tracks.is_empty() {
        return Ok(());
    }
    let has_any: bool = connection
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM collection_item LIMIT 1)")?
        .query_row([], |row| row.get(0))?;
    if !has_any {
        for track in tracks {
            track.collections.clear();
        }
        return Ok(());
    }
    let track_ids: Vec<i64> = tracks.iter().map(|track| track.track_id).collect();
    let mut by_track = collections_by_track_ids(connection, &track_ids)?;
    for track in tracks {
        track.collections = by_track.remove(&track.track_id).unwrap_or_default();
    }
    Ok(())
}
