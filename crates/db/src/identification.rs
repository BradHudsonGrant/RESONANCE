use std::collections::HashMap;

use crate::{IN_CHUNK, placeholders};
use resonance_core::identity::{Chromaprint, ContentHash};
use resonance_core::library::TagField;
use resonance_core::staging::{Proposal, ProposalStatus, StoredProposal};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::search::now_unix_ns;
use crate::tags::store_proposals_with_layers_in_tx;
use crate::{Database, DbError, tag_field};

/// What identification reads for a track: its primary file's path, length, stored fingerprint
/// and tags. Heavy binary blobs are loaded lazily via `asset_audio_identity`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentificationInput {
    pub track_id: i64,
    /// Relative to the source folder, with forward slashes.
    pub path: String,
    pub duration_ms: Option<u64>,
    pub fingerprint: Option<String>,
    pub fingerprint_duration_s: Option<u32>,
    pub file_isrc: Option<String>,
    pub tags: Vec<(TagField, String)>,
}

/// Lazy audio identity loaded only when a track runs preview verification or AudD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetAudioIdentity {
    pub source_folder: String,
    pub path: String,
    pub fingerprint_raw: Option<Vec<u32>>,
    pub audio_hash: Option<ContentHash>,
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

/// Locally identified track record together with its file tags and staged proposals.
pub type LocalIdentificationItem = (StoredIdentification, Vec<(TagField, String)>, Vec<Proposal>);

/// One identification result (local or online) ready to commit alongside its proposals.
pub struct IdentificationBatchItem {
    pub result: StoredIdentification,
    pub file_tags: Vec<(TagField, String)>,
    pub proposals: Vec<Proposal>,
    pub clear_pending_artwork: bool,
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
    pub candidates_json: String,
    pub selected_candidate: u32,
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

const INPUT_COLUMNS: &str = "asset.id, asset.track_id, asset.path, asset.duration_ms,
     asset.fingerprint, asset.fingerprint_duration_s, asset.file_isrc";

struct PendingInputRow {
    asset_id: i64,
    track_id: i64,
    path: String,
    effective_duration_ms: Option<u64>,
    fingerprint: Option<String>,
    fingerprint_duration_s: Option<u32>,
    file_isrc: Option<String>,
}

fn read_pending_row(row: &rusqlite::Row<'_>) -> Result<PendingInputRow, rusqlite::Error> {
    let duration_ms: Option<u64> = row.get(3)?;
    let fingerprint_duration_s: Option<u32> = row.get(5)?;
    let effective_duration_ms = duration_ms.filter(|&ms| ms > 0).or_else(|| {
        fingerprint_duration_s
            .filter(|&s| s > 0)
            .map(|s| u64::from(s) * 1000)
    });
    Ok(PendingInputRow {
        asset_id: row.get(0)?,
        track_id: row.get(1)?,
        path: row.get(2)?,
        effective_duration_ms,
        fingerprint: row.get(4)?,
        fingerprint_duration_s,
        file_isrc: row.get(6)?,
    })
}

fn hydrate_inputs(
    connection: &Connection,
    pending: Vec<PendingInputRow>,
) -> Result<Vec<IdentificationInput>, DbError> {
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let asset_ids: Vec<i64> = pending.iter().map(|row| row.asset_id).collect();
    let mut tags_by_asset: HashMap<i64, Vec<(TagField, String)>> =
        HashMap::with_capacity(asset_ids.len());
    for chunk in asset_ids.chunks(IN_CHUNK) {
        let in_list = placeholders(chunk.len());
        let sql =
            format!("SELECT asset_id, field, value FROM file_tag WHERE asset_id IN ({in_list})");
        let mut tag_stmt = connection.prepare(&sql)?;
        let mut tag_rows = tag_stmt.query(params_from_iter(chunk.iter().copied()))?;
        while let Some(tag_row) = tag_rows.next()? {
            let asset_id: i64 = tag_row.get(0)?;
            tags_by_asset
                .entry(asset_id)
                .or_default()
                .push((tag_field(tag_row.get(1)?)?, tag_row.get(2)?));
        }
    }
    for tags in tags_by_asset.values_mut() {
        tags.sort();
    }

    Ok(pending
        .into_iter()
        .map(|row| IdentificationInput {
            track_id: row.track_id,
            path: row.path,
            duration_ms: row.effective_duration_ms,
            fingerprint: row.fingerprint,
            fingerprint_duration_s: row.fingerprint_duration_s,
            file_isrc: row.file_isrc,
            tags: tags_by_asset.remove(&row.asset_id).unwrap_or_default(),
        })
        .collect())
}

type IdentificationWriteRef<'a> = (
    &'a StoredIdentification,
    &'a [(TagField, String)],
    &'a [Proposal],
    bool,
);

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
        let mut pending = Vec::new();
        while let Some(row) = rows.next()? {
            pending.push(read_pending_row(row)?);
        }
        hydrate_inputs(&self.connection, pending)
    }

    /// Batched inputs for `track_ids` that still need `pass`, preserving unique caller order.
    pub fn identification_inputs_for_tracks(
        &self,
        pass: IdentificationPass,
        track_ids: &[i64],
    ) -> Result<Vec<IdentificationInput>, DbError> {
        if track_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut unique_ids = track_ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        let mut pending = Vec::with_capacity(unique_ids.len());
        let condition = pass.condition();
        for chunk in unique_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!(
                "SELECT {INPUT_COLUMNS} FROM asset
                 WHERE {PRIMARY_ASSET} AND asset.track_id IN ({in_list}) AND {condition}"
            );
            let mut stmt = self.connection.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = rows.next()? {
                pending.push(read_pending_row(row)?);
            }
        }
        let mut by_track: HashMap<i64, IdentificationInput> =
            hydrate_inputs(&self.connection, pending)?
                .into_iter()
                .map(|item| (item.track_id, item))
                .collect();
        Ok(track_ids
            .iter()
            .filter_map(|id| by_track.remove(id))
            .collect())
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
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let pending = vec![read_pending_row(row)?];
        Ok(hydrate_inputs(&self.connection, pending)?
            .into_iter()
            .next())
    }

    pub fn has_tracks_needing_identification(
        &self,
        pass: IdentificationPass,
    ) -> Result<bool, DbError> {
        Ok(self
            .connection
            .prepare_cached(&format!(
                "SELECT EXISTS(SELECT 1 FROM asset WHERE {PRIMARY_ASSET} AND {} LIMIT 1)",
                pass.condition()
            ))?
            .query_row([], |row| row.get(0))?)
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
                  candidates_json, selected_candidate, updated_ns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (track_id) DO UPDATE SET
                 online = excluded.online,
                 confidence = excluded.confidence,
                 audio_matched = excluded.audio_matched,
                 known_json = excluded.known_json,
                 best_json = excluded.best_json,
                 steps_json = excluded.steps_json,
                 candidates_json = excluded.candidates_json,
                 selected_candidate = excluded.selected_candidate,
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
                result.candidates_json,
                result.selected_candidate,
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
                     steps_json, candidates_json, selected_candidate, updated_ns
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
                        candidates_json: row.get(7)?,
                        selected_candidate: row.get(8)?,
                        updated_ns: row.get(9)?,
                    })
                },
            )
            .optional()?)
    }

    /// Batched `StoredIdentification` lookup for `track_ids` in chunked queries.
    pub fn identifications_by_track_ids(
        &self,
        track_ids: &[i64],
    ) -> Result<HashMap<i64, StoredIdentification>, DbError> {
        if track_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut unique_ids = track_ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        let mut out = HashMap::with_capacity(unique_ids.len());
        for chunk in unique_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!(
                "SELECT track_id, online, confidence, audio_matched, known_json, best_json,
                     steps_json, candidates_json, selected_candidate, updated_ns
                 FROM track_identification WHERE track_id IN ({in_list})"
            );
            let mut stmt = self.connection.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = rows.next()? {
                let track_id: i64 = row.get(0)?;
                out.insert(
                    track_id,
                    StoredIdentification {
                        track_id,
                        online: row.get(1)?,
                        confidence: row.get(2)?,
                        audio_matched: row.get(3)?,
                        known_json: row.get(4)?,
                        best_json: row.get(5)?,
                        steps_json: row.get(6)?,
                        candidates_json: row.get(7)?,
                        selected_candidate: row.get(8)?,
                        updated_ns: row.get(9)?,
                    },
                );
            }
        }
        Ok(out)
    }

    /// Records a batch of local identification results and their proposals in one transaction.
    pub fn record_local_identifications_batch(
        &mut self,
        items: &[LocalIdentificationItem],
    ) -> Result<(), DbError> {
        let borrowed: Vec<IdentificationWriteRef<'_>> = items
            .iter()
            .map(|(result, file_tags, proposals)| {
                (result, file_tags.as_slice(), proposals.as_slice(), false)
            })
            .collect();
        self.record_identifications_batch_inner(&borrowed, false)
    }

    /// Records a batch of online identification results, their proposals and cleared artwork in
    /// one transaction.
    pub fn record_online_identifications_batch(
        &mut self,
        items: &[IdentificationBatchItem],
    ) -> Result<(), DbError> {
        let borrowed: Vec<IdentificationWriteRef<'_>> = items
            .iter()
            .map(|item| {
                (
                    &item.result,
                    item.file_tags.as_slice(),
                    item.proposals.as_slice(),
                    item.clear_pending_artwork,
                )
            })
            .collect();
        self.record_identifications_batch_inner(&borrowed, true)
    }

    fn record_identifications_batch_inner(
        &mut self,
        items: &[IdentificationWriteRef<'_>],
        stage_empty_proposals: bool,
    ) -> Result<(), DbError> {
        if items.is_empty() {
            return Ok(());
        }
        let tx = self.connection.transaction()?;
        let now = now_unix_ns();
        let mut staged_indices: Vec<(usize, i64)> = Vec::new();
        for (idx, &(result, _, proposals, clear_pending_artwork)) in items.iter().enumerate() {
            let wrote = tx
                .prepare_cached(
                    "INSERT INTO track_identification
                         (track_id, online, confidence, audio_matched, known_json, best_json,
                          steps_json, candidates_json, selected_candidate, updated_ns)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT (track_id) DO UPDATE SET
                         online = excluded.online,
                         confidence = excluded.confidence,
                         audio_matched = excluded.audio_matched,
                         known_json = excluded.known_json,
                         best_json = excluded.best_json,
                         steps_json = excluded.steps_json,
                         candidates_json = excluded.candidates_json,
                         selected_candidate = excluded.selected_candidate,
                         updated_ns = excluded.updated_ns
                     WHERE excluded.online >= track_identification.online",
                )?
                .execute(params![
                    result.track_id,
                    result.online,
                    result.confidence,
                    result.audio_matched,
                    result.known_json,
                    result.best_json,
                    result.steps_json,
                    result.candidates_json,
                    result.selected_candidate,
                    now,
                ])?;
            if wrote > 0 && (!proposals.is_empty() || stage_empty_proposals) {
                staged_indices.push((idx, result.track_id));
            }
            if wrote > 0 && clear_pending_artwork {
                tx.prepare_cached(
                    "DELETE FROM proposed_artwork WHERE track_id = ?1 AND status = 'pending'",
                )?
                .execute([result.track_id])?;
            }
        }
        if !staged_indices.is_empty() {
            let mut track_ids: Vec<i64> = staged_indices.iter().map(|&(_, id)| id).collect();
            track_ids.sort_unstable();
            track_ids.dedup();
            let mut user_by_track: HashMap<i64, Vec<(TagField, String)>> = HashMap::new();
            let mut proposals_by_track: HashMap<i64, Vec<StoredProposal>> = HashMap::new();
            for chunk in track_ids.chunks(IN_CHUNK) {
                let in_list = placeholders(chunk.len());

                let user_sql = format!(
                    "SELECT track_id, field, value FROM user_tag WHERE track_id IN ({in_list})"
                );
                let mut user_stmt = tx.prepare(&user_sql)?;
                let mut user_rows = user_stmt.query(params_from_iter(chunk.iter().copied()))?;
                while let Some(row) = user_rows.next()? {
                    let track_id: i64 = row.get(0)?;
                    user_by_track
                        .entry(track_id)
                        .or_default()
                        .push((tag_field(row.get(1)?)?, row.get(2)?));
                }

                let prop_sql = format!(
                    "SELECT track_id, field, value, provider, confidence, audio_matched, status
                     FROM proposed_tag WHERE track_id IN ({in_list})"
                );
                let mut prop_stmt = tx.prepare(&prop_sql)?;
                let mut prop_rows = prop_stmt.query(params_from_iter(chunk.iter().copied()))?;
                while let Some(row) = prop_rows.next()? {
                    let track_id: i64 = row.get(0)?;
                    let status: String = row.get(6)?;
                    proposals_by_track
                        .entry(track_id)
                        .or_default()
                        .push(StoredProposal {
                            proposal: Proposal {
                                field: tag_field(row.get(1)?)?,
                                value: row.get(2)?,
                                provider: row.get(3)?,
                                confidence: row.get(4)?,
                                audio_matched: row.get(5)?,
                            },
                            status: ProposalStatus::from_name(&status).ok_or(
                                DbError::UnknownName {
                                    column: "proposed_tag.status",
                                    name: status,
                                },
                            )?,
                        });
                }
            }
            for tags in user_by_track.values_mut() {
                tags.sort();
            }
            for (idx, track_id) in staged_indices {
                let (_, file_tags, proposals, _) = items[idx];
                let user_tags = user_by_track.get(&track_id).map_or(&[][..], Vec::as_slice);
                let existing = proposals_by_track
                    .get(&track_id)
                    .map_or(&[][..], Vec::as_slice);
                if !proposals.is_empty() || !existing.is_empty() {
                    store_proposals_with_layers_in_tx(
                        &tx, track_id, file_tags, user_tags, existing, proposals, now,
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_selected_candidate(
        &mut self,
        track_id: i64,
        index: u32,
        best_json: Option<&str>,
        confidence: Option<f64>,
        audio_matched: bool,
    ) -> Result<bool, DbError> {
        let updated = self.connection.execute(
            "UPDATE track_identification
             SET selected_candidate = ?2,
                 best_json = ?3,
                 confidence = ?4,
                 audio_matched = ?5,
                 updated_ns = ?6
             WHERE track_id = ?1",
            params![
                track_id,
                index,
                best_json,
                confidence,
                audio_matched,
                now_unix_ns()
            ],
        )?;
        Ok(updated > 0)
    }

    /// Loads source path, raw Chromaprint subfingerprints and `audio_hash` lazily for one track.
    pub fn asset_audio_identity(
        &self,
        track_id: i64,
    ) -> Result<Option<AssetAudioIdentity>, DbError> {
        self.connection
            .query_row(
                "SELECT source.folder, asset.path, asset.fingerprint_raw, asset.audio_hash
                 FROM asset JOIN source ON source.id = asset.source_id
                 WHERE asset.track_id = ?1 AND asset.is_primary = 1 AND asset.included = 1
                 LIMIT 1",
                [track_id],
                |row| {
                    let fp_bytes: Option<Vec<u8>> = row.get(2)?;
                    let hash_bytes: Option<Vec<u8>> = row.get(3)?;
                    Ok(AssetAudioIdentity {
                        source_folder: row.get(0)?,
                        path: row.get(1)?,
                        fingerprint_raw: fp_bytes
                            .as_deref()
                            .and_then(Chromaprint::raw_from_le_bytes),
                        audio_hash: hash_bytes.as_deref().and_then(ContentHash::from_slice),
                    })
                },
            )
            .optional()
            .map_err(Into::into)
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

#[cfg(test)]
mod tests {
    use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};

    use super::*;

    #[test]
    fn identification_input_falls_back_to_fingerprint_duration_when_header_duration_is_missing() {
        let mut db = Database::open_in_memory().expect("in-memory db");
        let source = db.source("C:/Music").expect("source");
        db.record_scan(
            source,
            &[ScannedFile {
                path: "Artist - Track.mp3".to_owned(),
                size_bytes: 1_000_000,
                modified_ns: 1_700_000_000_000_000_000,
                format: AudioFormat::Mp3,
                audio: Some(AudioFacts {
                    duration_ms: 0,
                    sample_rate: Some(44_100),
                    channels: Some(2),
                    bit_depth: None,
                    bitrate_kbps: Some(320),
                }),
                tags: vec![(TagField::Title, "Track".to_owned())],
            }],
        )
        .expect("scan");

        let zero_fallback = db
            .identification_input(IdentificationPass::Any, 1)
            .expect("query")
            .expect("input present");
        assert_eq!(zero_fallback.duration_ms, None);

        db.connection
            .execute(
                "UPDATE asset SET duration_ms = 0, fingerprint_duration_s = 305 WHERE track_id = 1",
                [],
            )
            .expect("set zero duration_ms");
        let input_from_zero = db
            .identification_input(IdentificationPass::Any, 1)
            .expect("query")
            .expect("input present");
        assert_eq!(input_from_zero.duration_ms, Some(305_000));

        db.connection
            .execute(
                "UPDATE asset SET duration_ms = NULL, fingerprint_duration_s = 305 WHERE track_id = 1",
                [],
            )
            .expect("clear duration_ms");

        let input = db
            .identification_input(IdentificationPass::Any, 1)
            .expect("query")
            .expect("input present");
        assert_eq!(input.duration_ms, Some(305_000));
        assert_eq!(input.fingerprint_duration_s, Some(305));
    }
}
