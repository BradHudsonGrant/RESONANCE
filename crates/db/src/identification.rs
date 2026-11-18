use resonance_core::library::TagField;
use rusqlite::{Connection, OptionalExtension, params};

use crate::search::now_unix_ns;
use crate::{Database, DbError, tag_field};

/// What identification reads for a track: its primary file's path, length, stored fingerprint
/// and tags. Nothing here needs the audio decoded again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentificationInput {
    pub track_id: i64,
    /// Relative to the source folder, with forward slashes.
    pub path: String,
    pub duration_ms: Option<u64>,
    pub fingerprint: Option<String>,
    pub fingerprint_duration_s: Option<u32>,
    pub tags: Vec<(TagField, String)>,
}

/// Which tracks still need a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentificationPass {
    /// Tracks with no result at all. Steps 1 and 2 run on them.
    Local,
    /// Tracks without an online result yet.
    Online,
    /// Any track, such as one the user pasted a release link for.
    Any,
}

impl IdentificationPass {
    fn condition(self) -> &'static str {
        match self {
            Self::Local => {
                "NOT EXISTS (SELECT 1 FROM track_identification ti WHERE ti.track_id = asset.track_id)"
            }
            Self::Online => {
                "NOT EXISTS (SELECT 1 FROM track_identification ti WHERE ti.track_id = asset.track_id AND ti.online = 1)"
            }
            Self::Any => "1",
        }
    }
}

/// A stored result, with its parts kept as the JSON the identify crate wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredIdentification {
    pub track_id: i64,
    pub online: bool,
    pub confidence: Option<f64>,
    pub audio_matched: bool,
    pub known_json: String,
    pub best_json: Option<String>,
    pub steps_json: String,
    pub updated_ns: i64,
}

/// A cached provider answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedResponse {
    pub status: u16,
    pub body: String,
    pub fetched_ns: i64,
}

const PRIMARY_ASSET: &str = "asset.is_primary = 1 AND asset.included = 1";

fn read_input(
    connection: &Connection,
    row: &rusqlite::Row<'_>,
) -> Result<IdentificationInput, DbError> {
    let asset_id: i64 = row.get(0)?;
    let mut tags_stmt =
        connection.prepare_cached("SELECT field, value FROM file_tag WHERE asset_id = ?1")?;
    let tags = tags_stmt
        .query_map([asset_id], |tag| {
            Ok((tag.get::<_, String>(0)?, tag.get::<_, String>(1)?))
        })?
        .map(|tag| {
            let (field, value) = tag?;
            Ok((tag_field(field)?, value))
        })
        .collect::<Result<_, DbError>>()?;
    Ok(IdentificationInput {
        track_id: row.get(1)?,
        path: row.get(2)?,
        duration_ms: row.get(3)?,
        fingerprint: row.get(4)?,
        fingerprint_duration_s: row.get(5)?,
        tags,
    })
}

const INPUT_COLUMNS: &str = "asset.id, asset.track_id, asset.path, asset.duration_ms,
     asset.fingerprint, asset.fingerprint_duration_s";

impl Database {
    /// Up to `limit` tracks after `after_track_id` that still need `pass`, in track order.
    pub fn identification_inputs(
        &self,
        pass: IdentificationPass,
        after_track_id: i64,
        limit: u32,
    ) -> Result<Vec<IdentificationInput>, DbError> {
        let mut stmt = self.connection.prepare(&format!(
            "SELECT {INPUT_COLUMNS} FROM asset
             WHERE {PRIMARY_ASSET} AND asset.track_id > ?1 AND {}
             ORDER BY asset.track_id LIMIT ?2",
            pass.condition()
        ))?;
        let mut rows = stmt.query(params![after_track_id, limit])?;
        let mut inputs = Vec::new();
        while let Some(row) = rows.next()? {
            inputs.push(read_input(&self.connection, row)?);
        }
        Ok(inputs)
    }

    /// The input for one track, if it still needs `pass`.
    pub fn identification_input(
        &self,
        pass: IdentificationPass,
        track_id: i64,
    ) -> Result<Option<IdentificationInput>, DbError> {
        let mut stmt = self.connection.prepare(&format!(
            "SELECT {INPUT_COLUMNS} FROM asset
             WHERE {PRIMARY_ASSET} AND asset.track_id = ?1 AND {}",
            pass.condition()
        ))?;
        let mut rows = stmt.query([track_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(read_input(&self.connection, row)?)),
            None => Ok(None),
        }
    }

    pub fn count_tracks_needing_identification(
        &self,
        pass: IdentificationPass,
    ) -> Result<u64, DbError> {
        Ok(self.connection.query_row(
            &format!(
                "SELECT count(*) FROM asset WHERE {PRIMARY_ASSET} AND {}",
                pass.condition()
            ),
            [],
            |row| row.get(0),
        )?)
    }

    /// Replaces the track's result, stamped now. A local result never replaces an online one.
    pub fn record_identification(&mut self, result: &StoredIdentification) -> Result<(), DbError> {
        self.connection.execute(
            "INSERT INTO track_identification
                 (track_id, online, confidence, audio_matched, known_json, best_json, steps_json,
                  updated_ns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (track_id) DO UPDATE SET
                 online = excluded.online,
                 confidence = excluded.confidence,
                 audio_matched = excluded.audio_matched,
                 known_json = excluded.known_json,
                 best_json = excluded.best_json,
                 steps_json = excluded.steps_json,
                 updated_ns = excluded.updated_ns
             WHERE excluded.online >= track_identification.online",
            params![
                result.track_id,
                result.online,
                result.confidence,
                result.audio_matched,
                result.known_json,
                result.best_json,
                result.steps_json,
                now_unix_ns(),
            ],
        )?;
        Ok(())
    }

    pub fn identification(&self, track_id: i64) -> Result<Option<StoredIdentification>, DbError> {
        Ok(self
            .connection
            .query_row(
                "SELECT track_id, online, confidence, audio_matched, known_json, best_json,
                     steps_json, updated_ns
                 FROM track_identification WHERE track_id = ?1",
                [track_id],
                |row| {
                    Ok(StoredIdentification {
                        track_id: row.get(0)?,
                        online: row.get(1)?,
                        confidence: row.get(2)?,
                        audio_matched: row.get(3)?,
                        known_json: row.get(4)?,
                        best_json: row.get(5)?,
                        steps_json: row.get(6)?,
                        updated_ns: row.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    /// A cached answer fetched at or after `fresh_since_ns`.
    pub fn cached_response(
        &self,
        provider: &str,
        request_hash: &[u8; 32],
        fresh_since_ns: i64,
    ) -> Result<Option<CachedResponse>, DbError> {
        Ok(self
            .connection
            .query_row(
                "SELECT status, body, fetched_ns FROM provider_cache
                 WHERE provider = ?1 AND request_hash = ?2 AND fetched_ns >= ?3",
                params![provider, request_hash.as_slice(), fresh_since_ns],
                |row| {
                    Ok(CachedResponse {
                        status: row.get(0)?,
                        body: row.get(1)?,
                        fetched_ns: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn store_response(
        &mut self,
        provider: &str,
        request_hash: &[u8; 32],
        status: u16,
        body: &str,
    ) -> Result<(), DbError> {
        self.connection.execute(
            "INSERT OR REPLACE INTO provider_cache (provider, request_hash, status, body, fetched_ns)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![provider, request_hash.as_slice(), status, body, now_unix_ns()],
        )?;
        Ok(())
    }

    /// Drops answers fetched before `fresh_since_ns`. Returns how many went.
    pub fn purge_stale_responses(&mut self, fresh_since_ns: i64) -> Result<usize, DbError> {
        Ok(self.connection.execute(
            "DELETE FROM provider_cache WHERE fetched_ns < ?1",
            [fresh_since_ns],
        )?)
    }
}
