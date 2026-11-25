use resonance_core::identity::{Chromaprint, ContentHash};
use resonance_core::library::AudioFormat;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use crate::identity::parse_format;
use crate::{Database, DbError};

/// Most files of about the same length read when looking for other encodes of one file. A
/// library has a few hundred files within 2.5 s of any length, so this is only a guard.
const DURATION_WINDOW_LIMIT: u32 = 4_000;

/// One included file as the copies and versions check sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCopy {
    pub asset_id: i64,
    pub track_id: i64,
    pub source_folder: String,
    pub path: String,
    pub format: AudioFormat,
    pub duration_ms: Option<u64>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub byte_hash: Option<ContentHash>,
    pub audio_hash: Option<ContentHash>,
    pub fingerprint: Option<Vec<u32>>,
    pub title: String,
    pub artist: Option<String>,
    /// The quality check found damage or a likely transcode.
    pub flagged: bool,
    pub missing: bool,
    pub is_primary: bool,
}

const COPY_COLUMNS: &str = "asset.id, asset.track_id, source.folder, asset.path, asset.format,
     asset.duration_ms, asset.bitrate_kbps, asset.sample_rate, asset.bit_depth,
     asset.byte_hash, asset.audio_hash, asset.fingerprint_raw, asset.sort_title,
     asset.sort_artist,
     EXISTS (SELECT 1 FROM quality_flag WHERE quality_flag.asset_id = asset.id
             AND quality_flag.kind IN ('damaged', 'transcode')),
     asset.missing_since IS NOT NULL, asset.is_primary";

struct RawCopy {
    asset_id: i64,
    track_id: i64,
    source_folder: String,
    path: String,
    format: String,
    duration_ms: Option<u64>,
    bitrate_kbps: Option<u32>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
    byte_hash: Option<Vec<u8>>,
    audio_hash: Option<Vec<u8>>,
    fingerprint: Option<Vec<u8>>,
    title: String,
    artist: Option<String>,
    flagged: bool,
    missing: bool,
    is_primary: bool,
}

fn raw_copy(row: &Row<'_>) -> rusqlite::Result<RawCopy> {
    Ok(RawCopy {
        asset_id: row.get(0)?,
        track_id: row.get(1)?,
        source_folder: row.get(2)?,
        path: row.get(3)?,
        format: row.get(4)?,
        duration_ms: row.get(5)?,
        bitrate_kbps: row.get(6)?,
        sample_rate: row.get(7)?,
        bit_depth: row.get(8)?,
        byte_hash: row.get(9)?,
        audio_hash: row.get(10)?,
        fingerprint: row.get(11)?,
        title: row.get(12)?,
        artist: row.get(13)?,
        flagged: row.get(14)?,
        missing: row.get(15)?,
        is_primary: row.get(16)?,
    })
}

fn stored_copy(raw: RawCopy) -> Result<StoredCopy, DbError> {
    Ok(StoredCopy {
        asset_id: raw.asset_id,
        track_id: raw.track_id,
        source_folder: raw.source_folder,
        path: raw.path,
        format: parse_format(raw.format)?,
        duration_ms: raw.duration_ms,
        bitrate_kbps: raw.bitrate_kbps,
        sample_rate: raw.sample_rate,
        bit_depth: raw.bit_depth,
        byte_hash: raw.byte_hash.as_deref().and_then(ContentHash::from_slice),
        audio_hash: raw.audio_hash.as_deref().and_then(ContentHash::from_slice),
        fingerprint: raw
            .fingerprint
            .as_deref()
            .and_then(Chromaprint::raw_from_le_bytes),
        title: raw.title,
        artist: raw.artist,
        flagged: raw.flagged,
        missing: raw.missing,
        is_primary: raw.is_primary,
    })
}

fn read_copies(
    connection: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<StoredCopy>, DbError> {
    let mut statement = connection.prepare(sql)?;
    let raw = statement
        .query_map(params, raw_copy)?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter().map(stored_copy).collect()
}

impl Database {
    /// The track's included files, the one it plays first.
    pub fn track_files(&self, track_id: i64) -> Result<Vec<StoredCopy>, DbError> {
        read_copies(
            &self.connection,
            &format!(
                "SELECT {COPY_COLUMNS} FROM asset JOIN source ON source.id = asset.source_id
                 WHERE asset.track_id = ?1 AND asset.included = 1
                 ORDER BY asset.is_primary DESC, asset.id ASC"
            ),
            [track_id],
        )
    }

    /// Files on other tracks that could be copies or versions of `file`: the same bytes or
    /// decoded audio, the same artist, or a fingerprinted file within 2.5 s of its length. Each
    /// branch reads its own index, so a library of 100,000 files answers in about a millisecond.
    pub fn copy_candidates(&self, file: &StoredCopy) -> Result<Vec<StoredCopy>, DbError> {
        let byte_hash = file.byte_hash.map(|hash| hash.as_bytes().to_vec());
        let audio_hash = file.audio_hash.map(|hash| hash.as_bytes().to_vec());
        let mut found = read_copies(
            &self.connection,
            &format!(
                "SELECT {COPY_COLUMNS} FROM asset JOIN source ON source.id = asset.source_id
                 WHERE asset.id IN (
                         SELECT id FROM asset WHERE byte_hash = ?2
                         UNION SELECT id FROM asset WHERE audio_hash = ?3
                         UNION SELECT id FROM asset
                               WHERE included = 1 AND (sort_artist IS NULL) = 0
                                 AND sort_artist = ?4)
                   AND asset.included = 1 AND asset.track_id != ?1"
            ),
            params![file.track_id, byte_hash, audio_hash, file.artist],
        )?;

        if file.fingerprint.is_some()
            && let Some(duration) = file.duration_ms
        {
            let window = resonance_core::duplicates::SAME_RECORDING_MAX_DURATION_DELTA_MS;
            let near = read_copies(
                &self.connection,
                &format!(
                    "SELECT {COPY_COLUMNS} FROM asset JOIN source ON source.id = asset.source_id
                     WHERE asset.included = 1 AND asset.track_id != ?1
                       AND (asset.duration_ms IS NULL) = 0
                       AND asset.duration_ms BETWEEN ?2 AND ?3
                       AND asset.fingerprint_raw IS NOT NULL
                     LIMIT {DURATION_WINDOW_LIMIT}"
                ),
                params![
                    file.track_id,
                    duration.saturating_sub(window),
                    duration.saturating_add(window)
                ],
            )?;
            for copy in near {
                if !found.iter().any(|seen| seen.asset_id == copy.asset_id) {
                    found.push(copy);
                }
            }
        }
        found.sort_by_key(|copy| copy.asset_id);
        Ok(found)
    }

    /// Folds `asset_id` into `kept_track_id`: its old track's favourite, collections and edited
    /// tags move across where the kept track has none, then the file's row goes, and its old
    /// track with it once that has no files left. The file on disk is not touched.
    pub fn remove_copy_into(&mut self, kept_track_id: i64, asset_id: i64) -> Result<bool, DbError> {
        let tx = self.connection.transaction()?;
        let Some(track_id): Option<i64> = tx
            .query_row(
                "SELECT track_id FROM asset WHERE id = ?1",
                [asset_id],
                |row| row.get(0),
            )
            .optional()?
        else {
            return Ok(false);
        };
        if track_id != kept_track_id {
            merge_track_into(&tx, kept_track_id, track_id)?;
        }
        tx.execute("DELETE FROM asset_fts WHERE rowid = ?1", [asset_id])?;
        tx.execute("DELETE FROM asset WHERE id = ?1", [asset_id])?;
        if track_id != kept_track_id {
            delete_track_without_files(&tx, track_id)?;
        }
        tx.commit()?;
        Ok(true)
    }
}

impl Database {
    /// Makes `asset_id` the file its track plays.
    pub fn make_primary_asset(&mut self, asset_id: i64) -> Result<(), DbError> {
        self.connection.execute(
            "UPDATE asset SET is_primary = (id = ?1)
             WHERE track_id = (SELECT track_id FROM asset WHERE id = ?1)",
            [asset_id],
        )?;
        Ok(())
    }
}

/// Carries a track's favourite, collection places and edited tags over to `kept`. What `kept`
/// already holds wins.
pub(crate) fn merge_track_into(
    tx: &Transaction<'_>,
    kept: i64,
    merged: i64,
) -> Result<(), DbError> {
    tx.execute(
        "UPDATE track
         SET favourite = CASE
             WHEN favourite = 1 OR COALESCE((SELECT favourite FROM track WHERE id = ?2), 0) = 1
             THEN 1 ELSE 0
         END
         WHERE id = ?1",
        params![kept, merged],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO collection_item (collection_id, track_id, position, added_ns)
         SELECT collection_id, ?1, position, added_ns
         FROM collection_item
         WHERE track_id = ?2",
        params![kept, merged],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO user_tag (track_id, field, value, dirty_on_disk, updated_ns)
         SELECT ?1, field, value, dirty_on_disk, updated_ns
         FROM user_tag
         WHERE track_id = ?2",
        params![kept, merged],
    )?;
    Ok(())
}

pub(crate) fn delete_track_without_files(
    tx: &Transaction<'_>,
    track_id: i64,
) -> Result<(), DbError> {
    let remaining: i64 = tx.query_row(
        "SELECT count(*) FROM asset WHERE track_id = ?1",
        [track_id],
        |row| row.get(0),
    )?;
    if remaining == 0 {
        tx.execute("DELETE FROM track WHERE id = ?1", [track_id])?;
    }
    Ok(())
}
