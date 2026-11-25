use std::io;

use resonance_core::identity::ContentHash;
use resonance_core::library::TagField;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::search::now_unix_ns;
use crate::tags::{refresh_track, write_user_tag_with_dirty};
use crate::{Database, DbError, FileStamp, StoredArtwork};

pub struct FileRevision<'a> {
    pub asset_id: i64,
    pub stamp: FileStamp,
    pub byte_hash: &'a ContentHash,
    pub tags: &'a [(TagField, String)],
    pub artwork: Option<&'a StoredArtwork>,
}

/// Proposed tag field, value, prior user value, and whether written to disk.
pub type AcceptedTagDecision = (TagField, String, Option<String>, bool);

/// Per-track batch of accepted library tag decisions.
pub type TrackTagDecisionChunk = (i64, Vec<AcceptedTagDecision>);

pub enum FileDecision<'a> {
    Write,
    Accept {
        /// Each proposed field, its proposed value, prior user value, and whether written to disk.
        fields: &'a [AcceptedTagDecision],
        /// The proposed cover hash and the previous user cover hash.
        cover: Option<(&'a str, Option<&'a str>)>,
    },
    Undo,
}

fn user_cover_state(conn: &Connection, track_id: i64) -> Result<Option<(String, bool)>, DbError> {
    Ok(conn
        .query_row(
            "SELECT artwork_hash, dirty_on_disk FROM user_artwork WHERE track_id = ?1",
            [track_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn user_tag_state(
    conn: &Connection,
    track_id: i64,
    field: &str,
) -> Result<Option<(String, bool)>, DbError> {
    Ok(conn
        .query_row(
            "SELECT value, dirty_on_disk FROM user_tag WHERE track_id = ?1 AND field = ?2",
            params![track_id, field],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn journal_value(
    tx: &Transaction<'_>,
    op: i64,
    track: i64,
    field: &str,
    value: &str,
    prior: Option<&str>,
    prior_dirty: bool,
) -> Result<(), DbError> {
    tx.execute(
        "INSERT INTO operation_tag_decision (operation_id, track_id, field, prior_value, prior_dirty, accepted_value)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![op, track, field, prior, prior_dirty, value],
    )?;
    Ok(())
}

fn undo_decisions(tx: &Transaction<'_>, operation_id: i64, track: i64) -> Result<(), DbError> {
    let mut stmt = tx.prepare(
        "SELECT field, prior_value, prior_dirty, accepted_value FROM operation_tag_decision
                 WHERE operation_id = ?1 AND track_id = ?2",
    )?;
    let mut rows = stmt.query(params![operation_id, track])?;
    while let Some(row) = rows.next()? {
        let field: String = row.get(0)?;
        let prior: Option<String> = row.get(1)?;
        let prior_dirty: bool = row.get(2)?;
        let accepted: String = row.get(3)?;
        if field == "artwork" {
            if user_cover_state(tx, track)?
                .map(|(hash, _)| hash)
                .as_deref()
                != Some(&accepted)
            {
                return Err(DbError::DecisionChanged);
            }
            match prior {
                Some(hash) => {
                    tx.execute(
                        "UPDATE user_artwork SET artwork_hash=?2, dirty_on_disk=?3, updated_ns=?4 WHERE track_id=?1",
                        params![track, hash, prior_dirty, now_unix_ns()],
                    )?;
                }
                None => {
                    tx.execute("DELETE FROM user_artwork WHERE track_id=?1", [track])?;
                }
            }
            tx.execute(
                "UPDATE proposed_artwork SET status='pending', updated_ns=?3
                    WHERE track_id=?1 AND artwork_hash=?2 AND status='accepted'",
                params![track, accepted, now_unix_ns()],
            )?;
        } else {
            if user_tag_state(tx, track, &field)?
                .map(|(value, _)| value)
                .as_deref()
                != Some(&accepted)
            {
                return Err(DbError::DecisionChanged);
            }
            match prior {
                Some(value) => write_user_tag_with_dirty(
                    tx,
                    track,
                    crate::tag_field(field.clone())?,
                    &value,
                    prior_dirty,
                    now_unix_ns(),
                )?,
                None => {
                    tx.execute(
                        "DELETE FROM user_tag WHERE track_id=?1 AND field=?2",
                        params![track, field],
                    )?;
                }
            }
            tx.execute("UPDATE proposed_tag SET status='pending', updated_ns=?4 WHERE track_id=?1 AND field=?2
                   AND value=?3 AND status='accepted'", params![track,field,accepted,now_unix_ns()])?;
        }
    }
    Ok(())
}

impl Database {
    pub fn check_decision_undo(&self, operation_id: i64, track_id: i64) -> Result<(), DbError> {
        let changed: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM operation_tag_decision d
             WHERE d.operation_id = ?1 AND d.track_id = ?2 AND d.accepted_value IS NOT
               (CASE WHEN d.field = 'artwork' THEN
                  (SELECT artwork_hash FROM user_artwork WHERE track_id = d.track_id)
                ELSE (SELECT value FROM user_tag WHERE track_id = d.track_id AND field = d.field) END))",
             params![operation_id, track_id], |row| row.get(0))?;
        if changed {
            return Err(DbError::DecisionChanged);
        }
        Ok(())
    }

    pub fn user_cover_hash(&self, track_id: i64) -> Result<Option<String>, DbError> {
        Ok(user_cover_state(&self.connection, track_id)?.map(|(hash, _)| hash))
    }

    /// Heavy file preparation is complete. Commit the decision, re-read tags and undo data
    /// together, rolling back all database changes if the file cannot be replaced.
    pub fn finish_tag_decision(
        &mut self,
        track: i64,
        revision: Option<FileRevision<'_>>,
        operation_id: i64,
        inverse: &str,
        decision: FileDecision<'_>,
        replace: impl FnOnce() -> io::Result<()>,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        match decision {
            FileDecision::Write => {}
            FileDecision::Accept { fields, cover } => {
                let now = now_unix_ns();
                for (field, value, prior, dirty_on_disk) in fields {
                    let existing = user_tag_state(&tx, track, field.name())?;
                    if existing.as_ref().map(|(v, _)| v.as_str()) != prior.as_deref()
                        || tx.execute(
                            "UPDATE proposed_tag SET status='accepted', updated_ns=?4 WHERE track_id=?1
                             AND field=?2 AND value=?3 AND status='pending'",
                            params![track, field.name(), value, now],
                        )? != 1
                    {
                        return Err(DbError::DecisionChanged);
                    }
                    let prior_dirty = existing.is_some_and(|(_, dirty)| dirty);
                    journal_value(
                        &tx,
                        operation_id,
                        track,
                        field.name(),
                        value,
                        prior.as_deref(),
                        prior_dirty,
                    )?;
                    write_user_tag_with_dirty(&tx, track, *field, value, *dirty_on_disk, now)?;
                }
                if let Some((hash, prior)) = cover {
                    let existing = user_cover_state(&tx, track)?;
                    if existing.as_ref().map(|(h, _)| h.as_str()) != prior ||
                        tx.execute("UPDATE proposed_artwork SET status='accepted', updated_ns=?3 WHERE track_id=?1
                                   AND artwork_hash=?2 AND status='pending'", params![track, hash, now])? != 1 {
                        return Err(DbError::DecisionChanged);
                    }
                    let prior_dirty = existing.is_some_and(|(_, dirty)| dirty);
                    journal_value(
                        &tx,
                        operation_id,
                        track,
                        "artwork",
                        hash,
                        prior,
                        prior_dirty,
                    )?;
                    tx.execute("INSERT INTO user_artwork(track_id, artwork_hash, dirty_on_disk, updated_ns)
                                VALUES(?1, ?2, 1, ?3) ON CONFLICT(track_id) DO UPDATE SET
                                artwork_hash=excluded.artwork_hash, dirty_on_disk=excluded.dirty_on_disk, updated_ns=excluded.updated_ns", params![track,hash,now])?;
                }
            }
            FileDecision::Undo => undo_decisions(&tx, operation_id, track)?,
        }
        if let Some(file) = revision {
            Self::record_rewritten_in_tx(
                &tx,
                file.asset_id,
                file.stamp,
                file.byte_hash,
                file.tags,
                file.artwork,
            )?;
        }
        if let FileDecision::Accept { fields, .. } = decision {
            for &(field, _, _, dirty_on_disk) in fields {
                if !dirty_on_disk {
                    tx.prepare_cached(
                        "UPDATE user_tag SET dirty_on_disk = 0 WHERE track_id = ?1 AND field = ?2",
                    )?
                    .execute(params![track, field.name()])?;
                }
            }
        }
        if !matches!(decision, FileDecision::Write) {
            refresh_track(&tx, track)?;
        }
        tx.execute(
            "UPDATE operation SET inverse_json=?2 WHERE id=?1",
            params![operation_id, inverse],
        )?;
        replace().map_err(DbError::FileEdit)?;
        tx.commit()?;
        Ok(())
    }

    /// Commits up to a chunk of library-only or file-matching accept decisions in one transaction,
    /// returning each track that tripped optimistic concurrency (`DecisionChanged`).
    pub fn accept_library_decisions_chunk<E: From<DbError>>(
        &mut self,
        operation_id: i64,
        chunk: &[TrackTagDecisionChunk],
        build_inverse: impl FnOnce(&[i64]) -> Result<String, E>,
    ) -> Result<Vec<i64>, E> {
        if chunk.is_empty() {
            return Ok(Vec::new());
        }
        let tx = self.connection.transaction().map_err(DbError::from)?;
        let now = now_unix_ns();
        let mut applied = Vec::with_capacity(chunk.len());
        let mut conflicted = Vec::new();
        for (track, fields) in chunk {
            tx.execute_batch("SAVEPOINT prep_lib_track")
                .map_err(DbError::from)?;
            let mut failed_conflict = false;
            for (field, value, prior, dirty_on_disk) in fields {
                let existing = user_tag_state(&tx, *track, field.name())?;
                if existing.as_ref().map(|(v, _)| v.as_str()) != prior.as_deref()
                    || tx
                        .execute(
                            "UPDATE proposed_tag SET status='accepted', updated_ns=?4 WHERE track_id=?1
                             AND field=?2 AND value=?3 AND status='pending'",
                            params![track, field.name(), value, now],
                        )
                        .map_err(DbError::from)?
                        != 1
                {
                    failed_conflict = true;
                    break;
                }
                let prior_dirty = existing.is_some_and(|(_, dirty)| dirty);
                journal_value(
                    &tx,
                    operation_id,
                    *track,
                    field.name(),
                    value,
                    prior.as_deref(),
                    prior_dirty,
                )?;
                write_user_tag_with_dirty(&tx, *track, *field, value, *dirty_on_disk, now)?;
            }
            if failed_conflict {
                tx.execute_batch(
                    "ROLLBACK TO SAVEPOINT prep_lib_track; RELEASE SAVEPOINT prep_lib_track",
                )
                .map_err(DbError::from)?;
                conflicted.push(*track);
            } else {
                refresh_track(&tx, *track)?;
                tx.execute_batch("RELEASE SAVEPOINT prep_lib_track")
                    .map_err(DbError::from)?;
                applied.push(*track);
            }
        }
        if !applied.is_empty() {
            let inverse_json = build_inverse(&applied)?;
            tx.execute(
                "UPDATE operation SET inverse_json=?2 WHERE id=?1",
                params![operation_id, inverse_json],
            )
            .map_err(DbError::from)?;
        }
        tx.commit().map_err(DbError::from)?;
        Ok(conflicted)
    }

    /// Undoes a chunk of library-only decisions in one transaction while updating `inverse_json`
    /// once for the chunk.
    pub fn undo_library_decisions_chunk(
        &mut self,
        operation_id: i64,
        tracks: &[i64],
        inverse: &str,
    ) -> Result<(), DbError> {
        if tracks.is_empty() {
            return Ok(());
        }
        let tx = self.connection.transaction()?;
        for &track in tracks {
            undo_decisions(&tx, operation_id, track)?;
            refresh_track(&tx, track)?;
        }
        tx.execute(
            "UPDATE operation SET inverse_json=?2 WHERE id=?1",
            params![operation_id, inverse],
        )?;
        tx.commit()?;
        Ok(())
    }
}
