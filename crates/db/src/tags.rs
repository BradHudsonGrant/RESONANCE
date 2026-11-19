//! The proposed and user tag layers. File tags are stored by the scan. A user value changes
//! what a track shows at once: its asset's sort, filter and search columns are refreshed in the
//! same transaction, long before anything is written to the file.

use std::collections::HashSet;

use resonance_core::library::TagField;
use resonance_core::staging::{
    Proposal, ProposalStatus, StoredProposal, changing_proposals, effective_tags,
    fields_to_accept_above, staging_diff,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::search::{now_unix_ns, sync_asset_metadata_in_tx};
use crate::{Database, DbError, TrackRow, tag_field};

/// A track's three layers, as staging compares them.
#[derive(Debug, Clone, PartialEq)]
pub struct TagLayers {
    /// The tags in the track's primary file.
    pub file: Vec<(TagField, String)>,
    pub user: Vec<(TagField, String)>,
    pub proposals: Vec<StoredProposal>,
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

fn user_tags_of_track(
    connection: &Connection,
    track_id: i64,
) -> Result<Vec<(TagField, String)>, DbError> {
    read_tags(
        connection,
        "SELECT field, value FROM user_tag WHERE track_id = ?1",
        track_id,
    )
}

fn file_tags_of_asset(
    connection: &Connection,
    asset_id: i64,
) -> Result<Vec<(TagField, String)>, DbError> {
    read_tags(
        connection,
        "SELECT field, value FROM file_tag WHERE asset_id = ?1",
        asset_id,
    )
}

fn primary_asset(connection: &Connection, track_id: i64) -> Result<Option<i64>, DbError> {
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

fn tag_layers_of(connection: &Connection, track_id: i64) -> Result<Option<TagLayers>, DbError> {
    let Some(asset_id) = primary_asset(connection, track_id)? else {
        return Ok(None);
    };
    Ok(Some(TagLayers {
        file: file_tags_of_asset(connection, asset_id)?,
        user: user_tags_of_track(connection, track_id)?,
        proposals: proposals_of_track(connection, track_id)?,
    }))
}

pub(crate) fn populate_user_tags(
    connection: &Connection,
    tracks: &mut [TrackRow],
) -> Result<(), DbError> {
    let any: bool = connection
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM user_tag)")?
        .query_row([], |row| row.get(0))?;
    for track in tracks {
        track.user_tags = if any {
            user_tags_of_track(connection, track.track_id)?
        } else {
            Vec::new()
        };
    }
    Ok(())
}

/// Refreshes every asset of the track after its user layer changed.
fn refresh_track(tx: &Transaction<'_>, track_id: i64) -> Result<(), DbError> {
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

fn write_user_tag(
    tx: &Transaction<'_>,
    track_id: i64,
    field: TagField,
    value: &str,
    now: i64,
) -> Result<(), DbError> {
    tx.prepare_cached(
        "INSERT INTO user_tag (track_id, field, value, dirty_on_disk, updated_ns)
         VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT (track_id, field) DO UPDATE SET
             dirty_on_disk = CASE
                 WHEN user_tag.value = excluded.value THEN user_tag.dirty_on_disk ELSE 1
             END,
             value = excluded.value,
             updated_ns = excluded.updated_ns",
    )?
    .execute(params![track_id, field.name(), value, now])?;
    Ok(())
}

fn track_exists(tx: &Transaction<'_>, track_id: i64) -> Result<bool, DbError> {
    Ok(tx
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM track WHERE id = ?1)")?
        .query_row([track_id], |row| row.get(0))?)
}

/// Copies the proposals for `fields` into the user layer. Returns how many were taken.
fn accept_in_tx(
    tx: &Transaction<'_>,
    track_id: i64,
    fields: &[TagField],
    now: i64,
) -> Result<u32, DbError> {
    let mut accepted = 0;
    for field in fields {
        let value: Option<String> = tx
            .prepare_cached("SELECT value FROM proposed_tag WHERE track_id = ?1 AND field = ?2")?
            .query_row(params![track_id, field.name()], |row| row.get(0))
            .optional()?;
        let Some(value) = value else {
            continue;
        };
        write_user_tag(tx, track_id, *field, &value, now)?;
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

impl Database {
    /// The track's file, user and proposed values, or `None` for a track with no file.
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
        let Some(layers) = tag_layers_of(&transaction, track_id)? else {
            return Ok(());
        };
        let shown = effective_tags(&layers.file, &layers.user);
        let staged = changing_proposals(proposals.to_vec(), &shown);
        let staged_fields: HashSet<TagField> =
            staged.iter().map(|proposal| proposal.field).collect();
        for stored in &layers.proposals {
            if stored.status == ProposalStatus::Pending
                && !staged_fields.contains(&stored.proposal.field)
            {
                transaction.execute(
                    "DELETE FROM proposed_tag WHERE track_id = ?1 AND field = ?2",
                    params![track_id, stored.proposal.field.name()],
                )?;
            }
        }
        let now = now_unix_ns();
        for proposal in &staged {
            transaction
                .prepare_cached(
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
                )?
                .execute(params![
                    track_id,
                    proposal.field.name(),
                    proposal.value,
                    proposal.provider,
                    proposal.confidence,
                    proposal.audio_matched,
                    now,
                ])?;
        }
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
