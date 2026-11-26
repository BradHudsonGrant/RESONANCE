use resonance_core::identity::{AssetAlignment, AssetIdentity};
use resonance_core::journal::StepState;
use resonance_core::library::ScannedFile;
use resonance_core::replace::CueKind;
use rusqlite::{OptionalExtension, Transaction, params};

use crate::copies::delete_track_without_files;
use crate::external::{asset_full_path, queue_path_change};
use crate::identity::record_identity_in;
use crate::operations::set_step_state;
use crate::{Database, DbError, FileStamp, SourceId};

/// A cue as the library keeps it, in track time.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredCue {
    pub id: i64,
    pub track_id: i64,
    pub kind: CueKind,
    pub position_s: f64,
    /// A replace carried the cue onto a new file and the DJ has not checked it yet.
    pub carried_over_check: bool,
}

/// A new file taking a track's place, beside the file it replaces. `file.path` is relative to
/// the old file's source.
#[derive(Debug, Clone)]
pub struct Replacement<'a> {
    pub track_id: i64,
    pub old_asset_id: i64,
    pub file: &'a ScannedFile,
    pub identity: &'a AssetIdentity,
    pub alignment: AssetAlignment,
    /// Cues that move onto the new file and need checking. Cues already waiting for a check
    /// are left out, so undo leaves them waiting.
    pub cue_ids: &'a [i64],
    /// User values the new file now holds, which no longer wait to be written.
    pub written_fields: &'a [String],
}

/// What puts a [`Replacement`] back.
#[derive(Debug, Clone)]
pub struct Detachment<'a> {
    pub track_id: i64,
    pub old_asset_id: i64,
    pub path: &'a str,
    pub cue_ids: &'a [i64],
    pub written_fields: &'a [String],
}

fn source_of(tx: &Transaction<'_>, asset_id: i64) -> Result<Option<SourceId>, DbError> {
    Ok(tx
        .query_row(
            "SELECT source_id FROM asset WHERE id = ?1",
            [asset_id],
            |row| row.get(0),
        )
        .optional()?
        .map(SourceId))
}

/// Records `file` in `source` and returns its asset and the track the scan put it on.
fn record_file(
    tx: &Transaction<'_>,
    source: SourceId,
    file: &ScannedFile,
    identity: &AssetIdentity,
) -> Result<(i64, i64), DbError> {
    Database::record_scan_in(tx, source, std::slice::from_ref(file))?;
    let (asset_id, track_id): (i64, i64) = tx.query_row(
        "SELECT id, track_id FROM asset WHERE source_id = ?1 AND path = ?2",
        params![source.0, file.path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let stamp = FileStamp {
        size_bytes: file.size_bytes,
        modified_ns: file.modified_ns,
    };
    record_identity_in(tx, asset_id, stamp, identity)?;
    Ok((asset_id, track_id))
}

impl Database {
    pub fn add_cue(
        &mut self,
        track_id: i64,
        kind: CueKind,
        position_s: f64,
    ) -> Result<i64, DbError> {
        self.connection.execute(
            "INSERT INTO cue (track_id, kind, position_s) VALUES (?1, ?2, ?3)",
            params![track_id, kind.name(), position_s],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// A track's cues, earliest first.
    pub fn cues(&self, track_id: i64) -> Result<Vec<StoredCue>, DbError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT id, track_id, kind, position_s, carried_over_check FROM cue
             WHERE track_id = ?1 ORDER BY position_s, id",
        )?;
        let rows = statement
            .query_map([track_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, bool>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, track_id, kind, position_s, carried_over_check)| {
                Ok(StoredCue {
                    id,
                    track_id,
                    kind: CueKind::from_name(&kind).ok_or(DbError::UnknownName {
                        column: "cue.kind",
                        name: kind,
                    })?,
                    position_s,
                    carried_over_check,
                })
            })
            .collect()
    }

    /// The user's values for `track_id` that still wait to be written to its file.
    pub fn unwritten_user_fields(&self, track_id: i64) -> Result<Vec<String>, DbError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT field FROM user_tag WHERE track_id = ?1 AND dirty_on_disk = 1 ORDER BY field",
        )?;
        let fields = statement
            .query_map([track_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(fields)
    }

    /// Puts a new file in a track's place in one transaction, and records step `position` of
    /// operation `operation_id` done with it. The new file keeps the track, so its collections,
    /// sets, favourite and the user's values stay as they are. The old file is left for the
    /// operation's own steps to hide and move to the bin.
    pub fn attach_replacement(
        &mut self,
        operation_id: i64,
        position: u32,
        replacement: &Replacement<'_>,
    ) -> Result<i64, DbError> {
        let tx = self.connection.transaction()?;
        let source = source_of(&tx, replacement.old_asset_id)?
            .ok_or(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        let (asset_id, scanned_track) =
            record_file(&tx, source, replacement.file, replacement.identity)?;
        tx.execute(
            "UPDATE asset SET track_id = ?2, is_primary = 1, offset_samples = ?3,
                 tempo_ratio = ?4
             WHERE id = ?1",
            params![
                asset_id,
                replacement.track_id,
                replacement.alignment.offset_samples(),
                replacement.alignment.tempo_ratio(),
            ],
        )?;
        tx.execute(
            "UPDATE asset SET is_primary = 0 WHERE id = ?1",
            [replacement.old_asset_id],
        )?;
        if scanned_track != replacement.track_id {
            delete_track_without_files(&tx, scanned_track)?;
        }
        if let (Some(old_path), Some(new_path)) = (
            asset_full_path(&tx, replacement.old_asset_id)?,
            asset_full_path(&tx, asset_id)?,
        ) {
            queue_path_change(&tx, replacement.track_id, &old_path, &new_path)?;
        }
        for cue in replacement.cue_ids {
            tx.execute(
                "UPDATE cue SET carried_over_check = 1 WHERE id = ?1 AND track_id = ?2",
                params![cue, replacement.track_id],
            )?;
        }
        for field in replacement.written_fields {
            tx.execute(
                "UPDATE user_tag SET dirty_on_disk = 0 WHERE track_id = ?1 AND field = ?2",
                params![replacement.track_id, field],
            )?;
        }
        set_step_state(&tx, operation_id, position, StepState::Done)?;
        tx.commit()?;
        Ok(asset_id)
    }

    /// Takes a replacing file back out of the library and records step `position` undone.
    pub fn detach_replacement(
        &mut self,
        operation_id: i64,
        position: u32,
        detachment: &Detachment<'_>,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        if let Some(source) = source_of(&tx, detachment.old_asset_id)? {
            let placed: Option<i64> = tx
                .query_row(
                    "SELECT id FROM asset WHERE source_id = ?1 AND path = ?2 AND track_id = ?3
                         AND id != ?4",
                    params![
                        source.0,
                        detachment.path,
                        detachment.track_id,
                        detachment.old_asset_id
                    ],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(asset_id) = placed {
                if let (Some(placed_path), Some(old_path)) = (
                    asset_full_path(&tx, asset_id)?,
                    asset_full_path(&tx, detachment.old_asset_id)?,
                ) {
                    queue_path_change(&tx, detachment.track_id, &placed_path, &old_path)?;
                }
                tx.execute("DELETE FROM asset_fts WHERE rowid = ?1", [asset_id])?;
                tx.execute("DELETE FROM asset WHERE id = ?1", [asset_id])?;
            }
        }
        tx.execute(
            "UPDATE asset SET is_primary = 1 WHERE id = ?1",
            [detachment.old_asset_id],
        )?;
        for cue in detachment.cue_ids {
            tx.execute(
                "UPDATE cue SET carried_over_check = 0 WHERE id = ?1 AND track_id = ?2",
                params![cue, detachment.track_id],
            )?;
        }
        for field in detachment.written_fields {
            tx.execute(
                "UPDATE user_tag SET dirty_on_disk = 1 WHERE track_id = ?1 AND field = ?2",
                params![detachment.track_id, field],
            )?;
        }
        set_step_state(&tx, operation_id, position, StepState::Undone)?;
        tx.commit()?;
        Ok(())
    }

    /// Adds a file placed beside `beside_asset_id` as a track of its own, and records step
    /// `position` done. Returns the new track.
    pub fn add_track_beside(
        &mut self,
        operation_id: i64,
        position: u32,
        beside_asset_id: i64,
        file: &ScannedFile,
        identity: &AssetIdentity,
    ) -> Result<i64, DbError> {
        let tx = self.connection.transaction()?;
        let source = source_of(&tx, beside_asset_id)?
            .ok_or(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        let (_, track_id) = record_file(&tx, source, file, identity)?;
        set_step_state(&tx, operation_id, position, StepState::Done)?;
        tx.commit()?;
        Ok(track_id)
    }

    /// Takes a track added by [`Database::add_track_beside`] back out, and records step
    /// `position` undone.
    pub fn remove_track_beside(
        &mut self,
        operation_id: i64,
        position: u32,
        beside_asset_id: i64,
        path: &str,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        if let Some(source) = source_of(&tx, beside_asset_id)? {
            let added: Option<(i64, i64)> = tx
                .query_row(
                    "SELECT id, track_id FROM asset WHERE source_id = ?1 AND path = ?2",
                    params![source.0, path],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((asset_id, track_id)) = added {
                tx.execute("DELETE FROM asset_fts WHERE rowid = ?1", [asset_id])?;
                tx.execute("DELETE FROM asset WHERE id = ?1", [asset_id])?;
                tx.execute(
                    "DELETE FROM collection_item WHERE track_id = ?1",
                    [track_id],
                )?;
                tx.execute("DELETE FROM user_tag WHERE track_id = ?1", [track_id])?;
                delete_track_without_files(&tx, track_id)?;
            }
        }
        set_step_state(&tx, operation_id, position, StepState::Undone)?;
        tx.commit()?;
        Ok(())
    }
}

impl Database {
    /// The track of the file at `path` in the same source as `beside_asset_id`.
    pub fn track_id_of_asset_path(
        &self,
        beside_asset_id: i64,
        path: &str,
    ) -> Result<Option<i64>, DbError> {
        Ok(self
            .connection
            .query_row(
                "SELECT asset.track_id FROM asset
                 JOIN asset AS beside ON beside.source_id = asset.source_id
                 WHERE beside.id = ?1 AND asset.path = ?2",
                params![beside_asset_id, path],
                |row| row.get(0),
            )
            .optional()?)
    }
}
