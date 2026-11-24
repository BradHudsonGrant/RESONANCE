//! The proposed and user tag layers. File tags are stored by the scan. A user value changes
//! what a track shows at once: its asset's sort, filter and search columns are refreshed in the
//! same transaction, long before anything is written to the file.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::{IN_CHUNK, placeholders};
use resonance_core::identity::ContentHash;
use resonance_core::library::{AudioFormat, TagField};
use resonance_core::staging::{
    Proposal, ProposalStatus, StoredProposal, changing_proposals, effective_tags,
    fields_to_accept_above, staging_diff,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::artwork::{
    StoredArtwork, record_file_artwork_in_tx, tracks_with_unwritten_artwork, unwritten_artwork,
};
use crate::search::{now_unix_ns, sync_asset_metadata_in_tx};
use crate::{Database, DbError, FileStamp, TrackRow, tag_field};

/// A file whose tags are about to be written, as the library last saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagWriteTarget {
    pub track_id: i64,
    pub asset_id: i64,
    pub source_folder: String,
    pub path: String,
    pub format: AudioFormat,
    pub stamp: FileStamp,
    pub missing: bool,
    pub file: Vec<(TagField, String)>,
    /// The user's values not yet in the file.
    pub unwritten: Vec<(TagField, String)>,
    /// The user's picture, when the file does not hold it yet.
    pub artwork: Option<StoredArtwork>,
}

/// A track's three layers, as staging compares them.
#[derive(Debug, Clone, PartialEq)]
pub struct TagLayers {
    /// The tags in the track's primary file.
    pub file: Vec<(TagField, String)>,
    pub user: Vec<(TagField, String)>,
    /// User values not yet written to disk (`dirty_on_disk = 1`).
    pub dirty_user: Vec<(TagField, String)>,
    pub proposals: Vec<StoredProposal>,
}

impl TagLayers {
    /// An opaque version of the ranked matches, displayed tags and artwork for one review.
    pub fn review_token(
        &self,
        track_id: i64,
        identification: Option<&crate::StoredIdentification>,
        artwork: &crate::TrackArtworkState,
    ) -> String {
        let mut state = DefaultHasher::new();
        track_id.hash(&mut state);
        identification
            .map(|id| (&id.candidates_json, id.selected_candidate))
            .hash(&mut state);
        for layer in [&self.file, &self.user] {
            let mut values: Vec<_> = layer.iter().map(|(f, v)| (f.name(), v)).collect();
            values.sort_unstable();
            values.hash(&mut state);
        }
        let mut proposals: Vec<_> = self
            .proposals
            .iter()
            .map(|stored| {
                let p = &stored.proposal;
                (
                    p.field.name(),
                    &p.value,
                    &p.provider,
                    p.confidence.to_bits(),
                    p.audio_matched,
                    stored.status.name(),
                )
            })
            .collect();
        proposals.sort_unstable();
        proposals.hash(&mut state);
        artwork.shown.as_ref().map(|a| &a.hash).hash(&mut state);
        artwork
            .proposed
            .as_ref()
            .map(|p| (&p.artwork.hash, &p.provider, &p.source_url, p.pending))
            .hash(&mut state);
        format!("{:016x}", state.finish())
    }
}

/// A track with proposals still to decide.
#[derive(Debug, Clone, PartialEq)]
pub struct StagingQueueEntry {
    pub track_id: i64,
    pub pending_fields: u32,
    pub confidence: f64,
    pub audio_matched: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AcceptReport {
    pub tracks: u32,
    pub fields: u32,
}

fn read_tags(
    connection: &Connection,
    sql: &str,
    id: i64,
) -> Result<Vec<(TagField, String)>, DbError> {
    let mut statement = connection.prepare_cached(sql)?;
    let mut rows = statement.query([id])?;
    let mut tags = Vec::new();
    while let Some(row) = rows.next()? {
        tags.push((tag_field(row.get(0)?)?, row.get(1)?));
    }
    tags.sort();
    Ok(tags)
}

pub(crate) fn user_tags_of_asset(
    connection: &Connection,
    asset_id: i64,
) -> Result<Vec<(TagField, String)>, DbError> {
    read_tags(
        connection,
        "SELECT user_tag.field, user_tag.value
         FROM asset JOIN user_tag ON user_tag.track_id = asset.track_id
         WHERE asset.id = ?1",
        asset_id,
    )
}

struct UserTrackTags {
    user: Vec<(TagField, String)>,
    dirty: Vec<(TagField, String)>,
}

fn user_tags_and_dirty_of_track(
    connection: &Connection,
    track_id: i64,
) -> Result<UserTrackTags, DbError> {
    let mut statement = connection
        .prepare_cached("SELECT field, value, dirty_on_disk FROM user_tag WHERE track_id = ?1")?;
    let mut rows = statement.query([track_id])?;
    let mut user = Vec::new();
    let mut dirty = Vec::new();
    while let Some(row) = rows.next()? {
        let field = tag_field(row.get(0)?)?;
        let value: String = row.get(1)?;
        let is_dirty: bool = row.get(2)?;
        if is_dirty {
            dirty.push((field, value.clone()));
        }
        user.push((field, value));
    }
    user.sort();
    dirty.sort();
    Ok(UserTrackTags { user, dirty })
}

pub(crate) fn file_tags_of_asset(
    connection: &Connection,
    asset_id: i64,
) -> Result<Vec<(TagField, String)>, DbError> {
    read_tags(
        connection,
        "SELECT field, value FROM file_tag WHERE asset_id = ?1",
        asset_id,
    )
}

pub(crate) fn primary_asset(
    connection: &Connection,
    track_id: i64,
) -> Result<Option<i64>, DbError> {
    Ok(connection
        .prepare_cached(
            "SELECT id FROM asset WHERE track_id = ?1
             ORDER BY included DESC, is_primary DESC, id ASC LIMIT 1",
        )?
        .query_row([track_id], |row| row.get(0))
        .optional()?)
}

fn proposals_of_track(
    connection: &Connection,
    track_id: i64,
) -> Result<Vec<StoredProposal>, DbError> {
    let mut statement = connection.prepare_cached(
        "SELECT field, value, provider, confidence, audio_matched, status
         FROM proposed_tag WHERE track_id = ?1",
    )?;
    let mut rows = statement.query([track_id])?;
    let mut proposals = Vec::new();
    while let Some(row) = rows.next()? {
        let status: String = row.get(5)?;
        proposals.push(StoredProposal {
            proposal: Proposal {
                field: tag_field(row.get(0)?)?,
                value: row.get(1)?,
                provider: row.get(2)?,
                confidence: row.get(3)?,
                audio_matched: row.get(4)?,
            },
            status: ProposalStatus::from_name(&status).ok_or(DbError::UnknownName {
                column: "proposed_tag.status",
                name: status,
            })?,
        });
    }
    Ok(proposals)
}

pub(crate) fn tag_layers_of(
    connection: &Connection,
    track_id: i64,
) -> Result<Option<TagLayers>, DbError> {
    let Some(asset_id) = primary_asset(connection, track_id)? else {
        return Ok(None);
    };
    let user_tags = user_tags_and_dirty_of_track(connection, track_id)?;
    Ok(Some(TagLayers {
        file: file_tags_of_asset(connection, asset_id)?,
        user: user_tags.user,
        dirty_user: user_tags.dirty,
        proposals: proposals_of_track(connection, track_id)?,
    }))
}

pub(crate) fn populate_user_tags(
    connection: &Connection,
    tracks: &mut [TrackRow],
) -> Result<(), DbError> {
    if tracks.is_empty() {
        return Ok(());
    }
    let any: bool = connection
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM user_tag)")?
        .query_row([], |row| row.get(0))?;
    if !any {
        for track in tracks {
            track.user_tags.clear();
        }
        return Ok(());
    }
    let mut track_ids: Vec<i64> = tracks.iter().map(|track| track.track_id).collect();
    track_ids.sort_unstable();
    track_ids.dedup();

    let mut by_track: HashMap<i64, Vec<(TagField, String)>> = HashMap::new();
    for chunk in track_ids.chunks(IN_CHUNK) {
        let in_list = placeholders(chunk.len());
        let sql =
            format!("SELECT track_id, field, value FROM user_tag WHERE track_id IN ({in_list})");
        let mut stmt = connection.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
        while let Some(row) = rows.next()? {
            let track_id: i64 = row.get(0)?;
            by_track
                .entry(track_id)
                .or_default()
                .push((tag_field(row.get(1)?)?, row.get(2)?));
        }
    }
    for tags in by_track.values_mut() {
        tags.sort();
    }
    for track in tracks {
        track.user_tags = by_track.get(&track.track_id).cloned().unwrap_or_default();
    }
    Ok(())
}

/// Refreshes every asset of the track after its user layer changed.
pub(crate) fn refresh_track(tx: &Transaction<'_>, track_id: i64) -> Result<(), DbError> {
    let assets: Vec<(i64, String)> = tx
        .prepare_cached("SELECT id, path FROM asset WHERE track_id = ?1")?
        .query_map([track_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (asset_id, path) in assets {
        let file = file_tags_of_asset(tx, asset_id)?;
        sync_asset_metadata_in_tx(tx, asset_id, &path, &file, true)?;
    }
    Ok(())
}

fn bpm_matches_integer_format(format: &str, user_value: &str, in_file: Option<&str>) -> bool {
    if format != AudioFormat::M4a.name() {
        return false;
    }
    let Some(file_value) = in_file else {
        return false;
    };
    match (
        user_value.trim().parse::<f64>(),
        file_value.trim().parse::<f64>(),
    ) {
        (Ok(user_bpm), Ok(file_bpm)) if user_bpm.is_finite() && file_bpm.is_finite() => {
            user_bpm.round() == file_bpm.round()
        }
        _ => false,
    }
}

pub(crate) fn write_user_tag(
    tx: &Transaction<'_>,
    track_id: i64,
    field: TagField,
    value: &str,
    now: i64,
) -> Result<(), DbError> {
    write_user_tag_with_dirty(tx, track_id, field, value, true, now)
}

pub(crate) fn write_user_tag_with_dirty(
    tx: &Transaction<'_>,
    track_id: i64,
    field: TagField,
    value: &str,
    dirty_on_disk: bool,
    now: i64,
) -> Result<(), DbError> {
    tx.prepare_cached(
        "INSERT INTO user_tag (track_id, field, value, dirty_on_disk, updated_ns)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (track_id, field) DO UPDATE SET
             dirty_on_disk = CASE
                 WHEN excluded.dirty_on_disk = 0 THEN 0
                 WHEN user_tag.value = excluded.value THEN user_tag.dirty_on_disk
                 ELSE 1
             END,
             value = excluded.value,
             updated_ns = excluded.updated_ns",
    )?
    .execute(params![track_id, field.name(), value, dirty_on_disk, now])?;
    Ok(())
}

pub(crate) fn track_exists(tx: &Transaction<'_>, track_id: i64) -> Result<bool, DbError> {
    Ok(tx
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM track WHERE id = ?1)")?
        .query_row([track_id], |row| row.get(0))?)
}

/// Copies the proposals for `fields` into the user layer. Returns how many were taken.
pub(crate) fn accept_in_tx(
    tx: &Transaction<'_>,
    track_id: i64,
    fields: &[TagField],
    now: i64,
) -> Result<u32, DbError> {
    if fields.is_empty() {
        return Ok(0);
    }
    let proposals: Vec<(TagField, String)> = {
        let mut stmt =
            tx.prepare_cached("SELECT field, value FROM proposed_tag WHERE track_id = ?1")?;
        let mut rows = stmt.query([track_id])?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next()? {
            pairs.push((tag_field(row.get(0)?)?, row.get(1)?));
        }
        pairs
    };
    let mut accepted = 0;
    for field in fields {
        let Some((_, value)) = proposals.iter().find(|(held, _)| *held == *field) else {
            continue;
        };
        write_user_tag(tx, track_id, *field, value, now)?;
        tx.prepare_cached(
            "UPDATE proposed_tag SET status = 'accepted', updated_ns = ?3
             WHERE track_id = ?1 AND field = ?2",
        )?
        .execute(params![track_id, field.name(), now])?;
        accepted += 1;
    }
    if accepted > 0 {
        refresh_track(tx, track_id)?;
    }
    Ok(accepted)
}

fn write_target_of(
    connection: &Connection,
    asset_id: i64,
) -> Result<Option<TagWriteTarget>, DbError> {
    let row = connection
        .prepare_cached(
            "SELECT asset.track_id, source.folder, asset.path, asset.format, asset.size_bytes,
                 asset.modified_ns, asset.missing_since IS NOT NULL
             FROM asset JOIN source ON source.id = asset.source_id
             WHERE asset.id = ?1",
        )?
        .query_row([asset_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, bool>(6)?,
            ))
        })
        .optional()?;
    let Some((track_id, source_folder, path, format, size_bytes, modified_ns, missing)) = row
    else {
        return Ok(None);
    };
    let format = AudioFormat::from_name(&format).ok_or(DbError::UnknownName {
        column: "asset.format",
        name: format,
    })?;
    Ok(Some(TagWriteTarget {
        track_id,
        asset_id,
        source_folder,
        path,
        format,
        stamp: FileStamp {
            size_bytes,
            modified_ns,
        },
        missing,
        file: file_tags_of_asset(connection, asset_id)?,
        unwritten: read_tags(
            connection,
            "SELECT field, value FROM user_tag WHERE track_id = ?1 AND dirty_on_disk = 1",
            track_id,
        )?,
        artwork: unwritten_artwork(connection, track_id)?,
    }))
}

pub(crate) fn store_proposals_with_layers_in_tx(
    transaction: &Transaction<'_>,
    track_id: i64,
    file_tags: &[(TagField, String)],
    user_tags: &[(TagField, String)],
    existing_proposals: &[StoredProposal],
    proposals: &[Proposal],
    now: i64,
) -> Result<(), DbError> {
    let shown = effective_tags(file_tags, user_tags);
    let staged = changing_proposals(proposals.to_vec(), &shown);
    let staged_fields: HashSet<TagField> = staged.iter().map(|proposal| proposal.field).collect();
    for stored in existing_proposals {
        if stored.status == ProposalStatus::Pending
            && !staged_fields.contains(&stored.proposal.field)
        {
            transaction
                .prepare_cached("DELETE FROM proposed_tag WHERE track_id = ?1 AND field = ?2")?
                .execute(params![track_id, stored.proposal.field.name()])?;
        }
    }
    let mut stmt = transaction.prepare_cached(
        "INSERT INTO proposed_tag
             (track_id, field, value, provider, confidence, audio_matched, status,
              updated_ns)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)
         ON CONFLICT (track_id, field) DO UPDATE SET
             status = CASE
                 WHEN proposed_tag.value = excluded.value THEN proposed_tag.status
                 ELSE 'pending'
             END,
             value = excluded.value,
             provider = excluded.provider,
             confidence = excluded.confidence,
             audio_matched = excluded.audio_matched,
             updated_ns = excluded.updated_ns",
    )?;
    for proposal in &staged {
        stmt.execute(params![
            track_id,
            proposal.field.name(),
            proposal.value,
            proposal.provider,
            proposal.confidence,
            proposal.audio_matched,
            now,
        ])?;
    }
    Ok(())
}

pub(crate) fn store_proposals_in_tx(
    transaction: &Transaction<'_>,
    track_id: i64,
    proposals: &[Proposal],
    now: i64,
) -> Result<(), DbError> {
    let Some(layers) = tag_layers_of(transaction, track_id)? else {
        return Ok(());
    };
    store_proposals_with_layers_in_tx(
        transaction,
        track_id,
        &layers.file,
        &layers.user,
        &layers.proposals,
        proposals,
        now,
    )
}

impl Database {
    /// The tracks among `track_ids`, or among all tracks when it is empty, that hold a user value
    /// or picture not yet written to their file, in track order.
    pub fn tracks_with_unwritten_tags(&self, track_ids: &[i64]) -> Result<Vec<i64>, DbError> {
        if track_ids.is_empty() {
            let mut statement = self.connection.prepare_cached(
                "SELECT DISTINCT track_id FROM user_tag WHERE dirty_on_disk = 1 ORDER BY track_id",
            )?;
            let mut unwritten: Vec<i64> = statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            unwritten.extend(tracks_with_unwritten_artwork(&self.connection)?);
            unwritten.sort_unstable();
            unwritten.dedup();
            return Ok(unwritten);
        }

        let mut unique_ids = track_ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        let mut unwritten = Vec::new();
        for chunk in unique_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!(
                "SELECT track_id FROM user_tag WHERE dirty_on_disk = 1 AND track_id IN ({in_list})
                 UNION
                 SELECT track_id FROM user_artwork WHERE dirty_on_disk = 1 AND track_id IN ({in_list})"
            );
            let mut stmt = self.connection.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(chunk.iter().copied()), |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            unwritten.extend(rows);
        }
        unwritten.sort_unstable();
        unwritten.dedup();
        Ok(unwritten)
    }

    /// Number of non-missing tracks holding dirty file-writable user tags or artwork.
    pub fn count_unwritten_tracks(&self) -> Result<u32, DbError> {
        Ok(self
            .connection
            .prepare_cached(
                "SELECT count(*) FROM (
                     SELECT ut.track_id
                     FROM user_tag ut
                     JOIN asset a ON a.id = (
                         SELECT id FROM asset
                         WHERE track_id = ut.track_id
                         ORDER BY included DESC, is_primary DESC, id ASC
                         LIMIT 1
                     )
                     LEFT JOIN file_tag ft ON ft.asset_id = a.id AND ft.field = ut.field
                     WHERE ut.dirty_on_disk = 1
                       AND ut.field NOT IN ('mix', 'subgenre')
                       AND a.missing_since IS NULL
                       AND COALESCE(ft.value, '') != ut.value
                     UNION
                     SELECT ua.track_id
                     FROM user_artwork ua
                     JOIN asset a ON a.id = (
                         SELECT id FROM asset
                         WHERE track_id = ua.track_id
                         ORDER BY included DESC, is_primary DESC, id ASC
                         LIMIT 1
                     )
                     WHERE ua.dirty_on_disk = 1
                       AND a.missing_since IS NULL
                       AND (a.artwork_hash IS NULL OR a.artwork_hash != ua.artwork_hash)
                 )",
            )?
            .query_row([], |row| row.get(0))?)
    }

    /// Batched `TagLayers` lookup for `track_ids`.
    pub fn tag_layers_by_track_ids(
        &self,
        track_ids: &[i64],
    ) -> Result<HashMap<i64, TagLayers>, DbError> {
        if track_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut unique_ids = track_ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();

        let mut primary_by_track: HashMap<i64, i64> = HashMap::with_capacity(unique_ids.len());
        let mut asset_ids = Vec::with_capacity(unique_ids.len());
        for chunk in unique_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!(
                "SELECT track_id, id FROM asset
                 WHERE track_id IN ({in_list})
                 ORDER BY track_id ASC, included DESC, is_primary DESC, id ASC"
            );
            let mut stmt = self.connection.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            let mut prev_track: Option<i64> = None;
            while let Some(row) = rows.next()? {
                let track_id: i64 = row.get(0)?;
                if prev_track != Some(track_id) {
                    prev_track = Some(track_id);
                    let asset_id: i64 = row.get(1)?;
                    primary_by_track.insert(track_id, asset_id);
                    asset_ids.push(asset_id);
                }
            }
        }
        if primary_by_track.is_empty() {
            return Ok(HashMap::new());
        }

        let mut file_by_asset: HashMap<i64, Vec<(TagField, String)>> =
            HashMap::with_capacity(asset_ids.len());
        for chunk in asset_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!(
                "SELECT asset_id, field, value FROM file_tag WHERE asset_id IN ({in_list})"
            );
            let mut stmt = self.connection.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = rows.next()? {
                let asset_id: i64 = row.get(0)?;
                file_by_asset
                    .entry(asset_id)
                    .or_default()
                    .push((tag_field(row.get(1)?)?, row.get(2)?));
            }
        }
        for tags in file_by_asset.values_mut() {
            tags.sort();
        }

        let mut active_track_ids: Vec<i64> = primary_by_track.keys().copied().collect();
        active_track_ids.sort_unstable();
        let mut user_by_track: HashMap<i64, UserTrackTags> = HashMap::new();
        let mut proposals_by_track: HashMap<i64, Vec<StoredProposal>> = HashMap::new();
        for chunk in active_track_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let user_sql = format!(
                "SELECT track_id, field, value, dirty_on_disk
                 FROM user_tag WHERE track_id IN ({in_list})"
            );
            let mut user_stmt = self.connection.prepare(&user_sql)?;
            let mut user_rows =
                user_stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = user_rows.next()? {
                let track_id: i64 = row.get(0)?;
                let field = tag_field(row.get(1)?)?;
                let value: String = row.get(2)?;
                let is_dirty: bool = row.get(3)?;
                let entry = user_by_track
                    .entry(track_id)
                    .or_insert_with(|| UserTrackTags {
                        user: Vec::new(),
                        dirty: Vec::new(),
                    });
                if is_dirty {
                    entry.dirty.push((field, value.clone()));
                }
                entry.user.push((field, value));
            }

            let prop_sql = format!(
                "SELECT track_id, field, value, provider, confidence, audio_matched, status
                 FROM proposed_tag WHERE track_id IN ({in_list})"
            );
            let mut prop_stmt = self.connection.prepare(&prop_sql)?;
            let mut prop_rows =
                prop_stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
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
                        status: ProposalStatus::from_name(&status).ok_or(DbError::UnknownName {
                            column: "proposed_tag.status",
                            name: status,
                        })?,
                    });
            }
        }

        let mut result = HashMap::with_capacity(primary_by_track.len());
        for (track_id, asset_id) in primary_by_track {
            let mut user_tags = user_by_track.remove(&track_id).unwrap_or(UserTrackTags {
                user: Vec::new(),
                dirty: Vec::new(),
            });
            user_tags.user.sort();
            user_tags.dirty.sort();
            result.insert(
                track_id,
                TagLayers {
                    file: file_by_asset.remove(&asset_id).unwrap_or_default(),
                    user: user_tags.user,
                    dirty_user: user_tags.dirty,
                    proposals: proposals_by_track.remove(&track_id).unwrap_or_default(),
                },
            );
        }
        Ok(result)
    }

    /// The track's primary file and the user values it does not hold yet.
    pub fn tag_write_target(&self, track_id: i64) -> Result<Option<TagWriteTarget>, DbError> {
        match primary_asset(&self.connection, track_id)? {
            Some(asset_id) => write_target_of(&self.connection, asset_id),
            None => Ok(None),
        }
    }

    pub fn tag_write_target_of_asset(
        &self,
        asset_id: i64,
    ) -> Result<Option<TagWriteTarget>, DbError> {
        write_target_of(&self.connection, asset_id)
    }

    /// Primary write targets for `track_ids`, loaded in chunked `IN` queries.
    pub fn tag_write_targets_by_track_ids(
        &self,
        track_ids: &[i64],
    ) -> Result<HashMap<i64, TagWriteTarget>, DbError> {
        if track_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut unique_ids = track_ids.to_vec();
        unique_ids.sort_unstable();
        unique_ids.dedup();

        struct PendingTarget {
            track_id: i64,
            asset_id: i64,
            source_folder: String,
            path: String,
            format: AudioFormat,
            stamp: FileStamp,
            missing: bool,
        }
        let mut targets = Vec::with_capacity(unique_ids.len());
        let mut asset_ids = Vec::with_capacity(unique_ids.len());
        let mut active_track_ids = Vec::with_capacity(unique_ids.len());

        for chunk in unique_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!(
                "SELECT asset.track_id, asset.id, source.folder, asset.path, asset.format,
                     asset.size_bytes, asset.modified_ns, asset.missing_since IS NOT NULL
                 FROM asset JOIN source ON source.id = asset.source_id
                 WHERE asset.track_id IN ({in_list})
                 ORDER BY asset.track_id ASC, asset.included DESC, asset.is_primary DESC, asset.id ASC"
            );
            let mut stmt = self.connection.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            let mut prev_track: Option<i64> = None;
            while let Some(row) = rows.next()? {
                let track_id: i64 = row.get(0)?;
                if prev_track != Some(track_id) {
                    prev_track = Some(track_id);
                    let asset_id: i64 = row.get(1)?;
                    let format_name: String = row.get(4)?;
                    let format =
                        AudioFormat::from_name(&format_name).ok_or(DbError::UnknownName {
                            column: "asset.format",
                            name: format_name,
                        })?;
                    asset_ids.push(asset_id);
                    active_track_ids.push(track_id);
                    targets.push(PendingTarget {
                        track_id,
                        asset_id,
                        source_folder: row.get(2)?,
                        path: row.get(3)?,
                        format,
                        stamp: FileStamp {
                            size_bytes: row.get(5)?,
                            modified_ns: row.get(6)?,
                        },
                        missing: row.get(7)?,
                    });
                }
            }
        }
        if targets.is_empty() {
            return Ok(HashMap::new());
        }

        let mut file_by_asset: HashMap<i64, Vec<(TagField, String)>> =
            HashMap::with_capacity(asset_ids.len());
        for chunk in asset_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let sql = format!(
                "SELECT asset_id, field, value FROM file_tag WHERE asset_id IN ({in_list})"
            );
            let mut stmt = self.connection.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = rows.next()? {
                let asset_id: i64 = row.get(0)?;
                file_by_asset
                    .entry(asset_id)
                    .or_default()
                    .push((tag_field(row.get(1)?)?, row.get(2)?));
            }
        }
        for tags in file_by_asset.values_mut() {
            tags.sort();
        }

        active_track_ids.sort_unstable();
        let mut unwritten_by_track: HashMap<i64, Vec<(TagField, String)>> = HashMap::new();
        let mut artwork_raw_by_track: HashMap<i64, (String, String, u64)> = HashMap::new();
        for chunk in active_track_ids.chunks(IN_CHUNK) {
            let in_list = placeholders(chunk.len());
            let tag_sql = format!(
                "SELECT track_id, field, value FROM user_tag
                 WHERE track_id IN ({in_list}) AND dirty_on_disk = 1"
            );
            let mut tag_stmt = self.connection.prepare(&tag_sql)?;
            let mut tag_rows = tag_stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = tag_rows.next()? {
                let track_id: i64 = row.get(0)?;
                unwritten_by_track
                    .entry(track_id)
                    .or_default()
                    .push((tag_field(row.get(1)?)?, row.get(2)?));
            }

            let art_sql = format!(
                "SELECT ua.track_id, a.hash, a.mime, a.byte_len
                 FROM user_artwork ua
                 JOIN artwork a ON a.hash = ua.artwork_hash
                 WHERE ua.track_id IN ({in_list}) AND ua.dirty_on_disk = 1"
            );
            let mut art_stmt = self.connection.prepare(&art_sql)?;
            let mut art_rows = art_stmt.query(rusqlite::params_from_iter(chunk.iter().copied()))?;
            while let Some(row) = art_rows.next()? {
                artwork_raw_by_track.insert(row.get(0)?, (row.get(1)?, row.get(2)?, row.get(3)?));
            }
        }
        for tags in unwritten_by_track.values_mut() {
            tags.sort();
        }

        let mut out = HashMap::with_capacity(targets.len());
        for t in targets {
            let artwork = artwork_raw_by_track
                .remove(&t.track_id)
                .map(|(hash, mime, byte_len)| -> Result<StoredArtwork, DbError> {
                    let kind = resonance_core::artwork::ImageKind::from_mime(&mime).ok_or(
                        DbError::UnknownName {
                            column: "artwork.mime",
                            name: mime,
                        },
                    )?;
                    Ok(StoredArtwork {
                        hash,
                        kind,
                        byte_len,
                    })
                })
                .transpose()?;
            out.insert(
                t.track_id,
                TagWriteTarget {
                    track_id: t.track_id,
                    asset_id: t.asset_id,
                    source_folder: t.source_folder,
                    path: t.path,
                    format: t.format,
                    stamp: t.stamp,
                    missing: t.missing,
                    file: file_by_asset.remove(&t.asset_id).unwrap_or_default(),
                    unwritten: unwritten_by_track.remove(&t.track_id).unwrap_or_default(),
                    artwork,
                },
            );
        }
        Ok(out)
    }

    /// Stores what a file holds after its tags were rewritten: its new size, time and byte hash
    /// (the audio hash is unchanged), its tags and front cover, and which user values it now
    /// carries. A user value the file holds is written; one it does not, after an undo for
    /// instance, is not.
    pub fn record_rewritten_file(
        &mut self,
        asset_id: i64,
        stamp: FileStamp,
        byte_hash: &ContentHash,
        tags: &[(TagField, String)],
        artwork: Option<&StoredArtwork>,
    ) -> Result<(), DbError> {
        let transaction = self.connection.transaction()?;
        Self::record_rewritten_in_tx(&transaction, asset_id, stamp, byte_hash, tags, artwork)?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn record_rewritten_in_tx(
        transaction: &Transaction<'_>,
        asset_id: i64,
        stamp: FileStamp,
        byte_hash: &ContentHash,
        tags: &[(TagField, String)],
        artwork: Option<&StoredArtwork>,
    ) -> Result<(), DbError> {
        let (track_id, path, format): (i64, String, String) = transaction.query_row(
            "SELECT track_id, path, format FROM asset WHERE id = ?1",
            [asset_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let old_tags = file_tags_of_asset(transaction, asset_id)?;
        transaction.execute(
            "UPDATE asset SET size_bytes = ?2, modified_ns = ?3, identity_size_bytes = ?2,
                 identity_modified_ns = ?3, byte_hash = ?4
             WHERE id = ?1",
            params![
                asset_id,
                stamp.size_bytes,
                stamp.modified_ns,
                byte_hash.as_bytes().as_slice()
            ],
        )?;
        transaction.execute("DELETE FROM file_tag WHERE asset_id = ?1", [asset_id])?;
        for (field, value) in tags {
            transaction
                .prepare_cached(
                    "INSERT INTO file_tag (asset_id, field, value) VALUES (?1, ?2, ?3)",
                )?
                .execute(params![asset_id, field.name(), value])?;
        }
        let user_rows = {
            let mut stmt = transaction.prepare_cached(
                "SELECT field, value, dirty_on_disk FROM user_tag WHERE track_id = ?1",
            )?;
            let mapped = stmt.query_map([track_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            })?;
            let mut list = Vec::new();
            for item in mapped {
                let (field_name, value, dirty) = item?;
                list.push((tag_field(field_name)?, value, dirty));
            }
            list
        };
        for (field, value, was_dirty) in user_rows {
            if !field.is_written_to_files() {
                continue;
            }
            let in_old_file = old_tags
                .iter()
                .find(|(held, _)| *held == field)
                .map(|(_, v)| v.as_str());
            let old_written = in_old_file == Some(value.as_str())
                || (value.is_empty() && in_old_file.is_none())
                || (field == TagField::Bpm
                    && bpm_matches_integer_format(&format, &value, in_old_file));
            let in_file = tags
                .iter()
                .find(|(held, _)| *held == field)
                .map(|(_, value)| value.as_str());
            let written = in_file == Some(value.as_str())
                || (value.is_empty() && in_file.is_none())
                || (field == TagField::Bpm && bpm_matches_integer_format(&format, &value, in_file));
            let dirty = if written {
                false
            } else {
                was_dirty || old_written
            };
            if dirty != was_dirty {
                transaction
                    .prepare_cached(
                        "UPDATE user_tag SET dirty_on_disk = ?3 WHERE track_id = ?1 AND field = ?2",
                    )?
                    .execute(params![track_id, field.name(), dirty])?;
            }
        }
        record_file_artwork_in_tx(transaction, asset_id, track_id, artwork)?;
        sync_asset_metadata_in_tx(transaction, asset_id, &path, tags, true)?;
        Ok(())
    }

    /// The track's file, user and proposed values, or `None` for a track with no file.
    pub fn review_token(&self, track_id: i64) -> Result<String, DbError> {
        let layers = self.tag_layers(track_id)?.ok_or(DbError::DecisionChanged)?;
        let art = self.track_artwork_state(track_id)?;
        Ok(layers.review_token(track_id, self.identification(track_id)?.as_ref(), &art))
    }

    pub fn check_review(&self, track_id: i64, expected: Option<&str>) -> Result<(), DbError> {
        if let Some(token) = expected
            && self.review_token(track_id)? != token
        {
            return Err(DbError::DecisionChanged);
        }
        Ok(())
    }

    pub fn tag_layers(&self, track_id: i64) -> Result<Option<TagLayers>, DbError> {
        tag_layers_of(&self.connection, track_id)
    }

    /// Sets the user's value for each field on every track: `Some` holds that value, an empty
    /// one blanking the file's, and `None` hands the field back to the file. Values are stored
    /// as given, so normalise them first. Returns how many tracks there were.
    pub fn set_user_tags(
        &mut self,
        track_ids: &[i64],
        edits: &[(TagField, Option<String>)],
    ) -> Result<u32, DbError> {
        let transaction = self.connection.transaction()?;
        let now = now_unix_ns();
        let mut changed = 0;
        for &track_id in track_ids {
            if !track_exists(&transaction, track_id)? {
                continue;
            }
            for (field, value) in edits {
                match value {
                    Some(value) => write_user_tag(&transaction, track_id, *field, value, now)?,
                    None => {
                        transaction
                            .prepare_cached(
                                "DELETE FROM user_tag WHERE track_id = ?1 AND field = ?2",
                            )?
                            .execute(params![track_id, field.name()])?;
                    }
                }
            }
            refresh_track(&transaction, track_id)?;
            changed += 1;
        }
        transaction.commit()?;
        Ok(changed)
    }

    /// Replaces what identification proposes for the track. The user layer is never touched.
    /// A field proposed with the value it was decided on keeps that decision; a new value is
    /// pending again; a field no longer proposed loses its pending row.
    pub fn store_proposals(
        &mut self,
        track_id: i64,
        proposals: &[Proposal],
    ) -> Result<(), DbError> {
        let transaction = self.connection.transaction()?;
        store_proposals_in_tx(&transaction, track_id, proposals, now_unix_ns())?;
        transaction.commit()?;
        Ok(())
    }

    /// Takes the proposals for `fields` into the user layer, including any the user holds,
    /// because the user picked them. Returns how many were taken.
    pub fn accept_proposals(&mut self, track_id: i64, fields: &[TagField]) -> Result<u32, DbError> {
        let transaction = self.connection.transaction()?;
        let accepted = accept_in_tx(&transaction, track_id, fields, now_unix_ns())?;
        transaction.commit()?;
        Ok(accepted)
    }

    /// Takes every proposal at or above `min_confidence` that changes a field the user does not
    /// hold, across all tracks.
    pub fn accept_above_threshold(&mut self, min_confidence: f64) -> Result<AcceptReport, DbError> {
        let transaction = self.connection.transaction()?;
        let track_ids: Vec<i64> = transaction
            .prepare(
                "SELECT DISTINCT track_id FROM proposed_tag
                 WHERE status = 'pending' AND confidence >= ?1 ORDER BY track_id",
            )?
            .query_map([min_confidence], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        let now = now_unix_ns();
        let mut report = AcceptReport::default();
        for track_id in track_ids {
            let Some(layers) = tag_layers_of(&transaction, track_id)? else {
                continue;
            };
            let diff = staging_diff(&layers.file, &layers.user, &layers.proposals);
            let fields = fields_to_accept_above(&diff, min_confidence);
            let accepted = accept_in_tx(&transaction, track_id, &fields, now)?;
            if accepted > 0 {
                report.tracks += 1;
                report.fields += accepted;
            }
        }
        transaction.commit()?;
        Ok(report)
    }

    /// Marks the track's pending proposals for `fields`, or all of them when `fields` is empty,
    /// as skipped. Returns how many.
    pub fn skip_proposals(&mut self, track_id: i64, fields: &[TagField]) -> Result<u32, DbError> {
        let transaction = self.connection.transaction()?;
        let now = now_unix_ns();
        let skipped = if fields.is_empty() {
            transaction.execute(
                "UPDATE proposed_tag SET status = 'skipped', updated_ns = ?2
                 WHERE track_id = ?1 AND status = 'pending'",
                params![track_id, now],
            )?
        } else {
            let mut skipped = 0;
            for field in fields {
                skipped += transaction.execute(
                    "UPDATE proposed_tag SET status = 'skipped', updated_ns = ?3
                     WHERE track_id = ?1 AND field = ?2 AND status = 'pending'",
                    params![track_id, field.name(), now],
                )?;
            }
            skipped
        };
        transaction.commit()?;
        Ok(u32::try_from(skipped).unwrap_or(u32::MAX))
    }

    /// Tracks with pending proposals, in track order.
    pub fn staging_queue(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<StagingQueueEntry>, DbError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT track_id, count(*), max(confidence), max(audio_matched)
             FROM proposed_tag WHERE status = 'pending'
             GROUP BY track_id ORDER BY track_id LIMIT ?1 OFFSET ?2",
        )?;
        let entries = statement
            .query_map([limit, offset], |row| {
                Ok(StagingQueueEntry {
                    track_id: row.get(0)?,
                    pending_fields: row.get(1)?,
                    confidence: row.get(2)?,
                    audio_matched: row.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(entries)
    }

    pub fn staging_queue_count(&self) -> Result<u32, DbError> {
        Ok(self.connection.query_row(
            "SELECT count(DISTINCT track_id) FROM proposed_tag WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?)
    }
}
