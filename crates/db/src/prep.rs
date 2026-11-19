//! Batch tag operations, release-wide edits and multi-track proposal decisions for Prep.

use resonance_core::library::TagField;
use resonance_core::prep::{
    TextCase, find_and_replace_edits, fix_case_edits, parent_folder, release_edits_from,
};
use resonance_core::staging::{effective_tags, fields_to_accept_above, staging_diff};
use rusqlite::{OptionalExtension, params};

use crate::search::now_unix_ns;
use crate::tags::{
    AcceptReport, accept_in_tx, refresh_track, tag_layers_of, track_exists, write_user_tag,
};
use crate::{Database, DbError};

/// A batch tag tool run across a selection, release or Prep batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchTagOperation {
    /// Sets or clears one or more fields (`Some("")` blanks the file's value; `None` hands back
    /// to the file).
    SetFields {
        edits: Vec<(TagField, Option<String>)>,
    },
    /// Replaces text across `fields` (or all text fields when empty).
    FindAndReplace {
        fields: Vec<TagField>,
        find: String,
        replace: String,
        case_sensitive: bool,
    },
    /// Fixes casing across `fields` (or the default text fields when empty), preserving DJ
    /// acronyms (`UK`, `DJ`, `EP`, `VIP`, `LP`, `MC`, `II`).
    FixCase {
        fields: Vec<TagField>,
        case: TextCase,
    },
    /// Copies release-level fields (`Album`, `Label`, `CatNo`, `Year`) from `source_track_id` to
    /// the target tracks or every track in the same release folder.
    ApplyReleaseFields {
        source_track_id: i64,
        fields: Vec<TagField>,
    },
}

impl Database {
    /// Track IDs with pending field or artwork proposals, in track order.
    pub fn pending_prep_track_ids(&self, limit: u32) -> Result<Vec<i64>, DbError> {
        let mut stmt = self.connection.prepare_cached(
            "SELECT track_id FROM (
                 SELECT DISTINCT track_id FROM proposed_tag WHERE status = 'pending'
                 UNION
                 SELECT DISTINCT track_id FROM proposed_artwork WHERE status = 'pending'
             )
             ORDER BY track_id LIMIT ?1",
        )?;
        let ids = stmt
            .query_map([limit], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(ids)
    }

    /// Accepts proposals for `track_ids` in one transaction. When `fields` is empty, takes all
    /// pending unlocked changing proposals on each track; when `fields` is given, takes those
    /// fields even if locked by the user.
    pub fn accept_proposals_for_tracks(
        &mut self,
        track_ids: &[i64],
        fields: &[TagField],
    ) -> Result<AcceptReport, DbError> {
        let tx = self.connection.transaction()?;
        let now = now_unix_ns();
        let mut report = AcceptReport::default();

        for &track_id in track_ids {
            let Some(layers) = tag_layers_of(&tx, track_id)? else {
                continue;
            };
            let target_fields: Vec<TagField> = if fields.is_empty() {
                let diff = staging_diff(&layers.file, &layers.user, &layers.proposals);
                fields_to_accept_above(&diff, 0.0)
            } else {
                fields.to_vec()
            };
            let accepted = accept_in_tx(&tx, track_id, &target_fields, now)?;
            if accepted > 0 {
                report.tracks += 1;
                report.fields += accepted;
            }
        }

        tx.commit()?;
        Ok(report)
    }

    /// Skips pending proposals for `fields` (or all pending proposals when `fields` is empty)
    /// across `track_ids` in one transaction.
    pub fn skip_proposals_for_tracks(
        &mut self,
        track_ids: &[i64],
        fields: &[TagField],
    ) -> Result<u32, DbError> {
        let tx = self.connection.transaction()?;
        let now = now_unix_ns();
        let mut skipped = 0usize;

        for &track_id in track_ids {
            if fields.is_empty() {
                skipped += tx.execute(
                    "UPDATE proposed_tag SET status = 'skipped', updated_ns = ?2
                     WHERE track_id = ?1 AND status = 'pending'",
                    params![track_id, now],
                )?;
            } else {
                for field in fields {
                    skipped += tx.execute(
                        "UPDATE proposed_tag SET status = 'skipped', updated_ns = ?3
                         WHERE track_id = ?1 AND field = ?2 AND status = 'pending'",
                        params![track_id, field.name(), now],
                    )?;
                }
            }
        }

        tx.commit()?;
        Ok(u32::try_from(skipped).unwrap_or(u32::MAX))
    }

    /// Every track whose primary included file sits in the same release folder (or shares the
    /// same album in the same source when at the source root) as `track_id`.
    pub fn tracks_in_same_release(&self, track_id: i64) -> Result<Vec<i64>, DbError> {
        let row: Option<(i64, String, Option<String>)> = self
            .connection
            .prepare_cached(
                "SELECT source_id, path, tag_album FROM asset
                 WHERE track_id = ?1 AND is_primary = 1 AND included = 1 LIMIT 1",
            )?
            .query_row([track_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?;
        let Some((source_id, path, tag_album)) = row else {
            return Ok(Vec::new());
        };

        let folder = parent_folder(&path);
        if !folder.is_empty() {
            let mut stmt = self.connection.prepare_cached(
                "SELECT track_id, path FROM asset
                 WHERE source_id = ?1 AND is_primary = 1 AND included = 1
                 ORDER BY track_id",
            )?;
            let rows = stmt.query_map([source_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut siblings = Vec::new();
            for row in rows {
                let (candidate_id, candidate_path) = row?;
                if parent_folder(&candidate_path).eq_ignore_ascii_case(folder) {
                    siblings.push(candidate_id);
                }
            }
            return Ok(siblings);
        }

        if let Some(album) = tag_album.filter(|a| !a.is_empty()) {
            let mut stmt = self.connection.prepare_cached(
                "SELECT track_id FROM asset
                 WHERE source_id = ?1 AND is_primary = 1 AND included = 1 AND tag_album = ?2
                 ORDER BY track_id",
            )?;
            let siblings = stmt
                .query_map(params![source_id, album], |row| row.get(0))?
                .collect::<Result<Vec<i64>, _>>()?;
            if !siblings.is_empty() {
                return Ok(siblings);
            }
        }

        Ok(vec![track_id])
    }

    /// Runs (or previews when `dry_run` is true) a batch tag operation across `track_ids`.
    /// For `ApplyReleaseFields`, uses `track_ids` when multiple tracks were passed, or discovers
    /// every track in the same release folder as `source_track_id` when `track_ids` has 0 or 1.
    pub fn run_batch_tag_operation(
        &mut self,
        track_ids: &[i64],
        operation: &BatchTagOperation,
        dry_run: bool,
    ) -> Result<AcceptReport, DbError> {
        let targets: Vec<i64> = match operation {
            BatchTagOperation::ApplyReleaseFields {
                source_track_id, ..
            } if track_ids.len() <= 1 => self.tracks_in_same_release(*source_track_id)?,
            _ => track_ids.to_vec(),
        };

        let tx = self.connection.transaction()?;
        let release_values: Vec<(TagField, String)> = match operation {
            BatchTagOperation::ApplyReleaseFields {
                source_track_id,
                fields,
            } => {
                let Some(layers) = tag_layers_of(&tx, *source_track_id)? else {
                    return Ok(AcceptReport::default());
                };
                let effective = effective_tags(&layers.file, &layers.user);
                release_edits_from(&effective, &layers.proposals, fields)
            }
            _ => Vec::new(),
        };

        let now = now_unix_ns();
        let mut report = AcceptReport::default();

        for &track_id in &targets {
            if !track_exists(&tx, track_id)? {
                continue;
            }
            let Some(layers) = tag_layers_of(&tx, track_id)? else {
                continue;
            };
            let effective = effective_tags(&layers.file, &layers.user);
            let current_of = |wanted: TagField| {
                effective
                    .iter()
                    .find(|(field, _)| *field == wanted)
                    .map(|(_, value)| value.as_str())
            };

            let mut track_changed_fields = 0u32;

            match operation {
                BatchTagOperation::SetFields { edits } => {
                    for (field, value) in edits {
                        let changes = match value {
                            Some(val) => current_of(*field) != Some(val.as_str()),
                            None => layers.user.iter().any(|(held, _)| *held == *field),
                        };
                        if !changes {
                            continue;
                        }
                        track_changed_fields += 1;
                        if !dry_run {
                            match value {
                                Some(val) => write_user_tag(&tx, track_id, *field, val, now)?,
                                None => {
                                    tx.prepare_cached(
                                        "DELETE FROM user_tag WHERE track_id = ?1 AND field = ?2",
                                    )?
                                    .execute(params![track_id, field.name()])?;
                                }
                            }
                        }
                    }
                }
                BatchTagOperation::FindAndReplace {
                    fields,
                    find,
                    replace,
                    case_sensitive,
                } => {
                    for (field, next) in
                        find_and_replace_edits(&effective, fields, find, replace, *case_sensitive)
                    {
                        track_changed_fields += 1;
                        if !dry_run {
                            write_user_tag(&tx, track_id, field, &next, now)?;
                        }
                    }
                }
                BatchTagOperation::FixCase { fields, case } => {
                    for (field, next) in fix_case_edits(&effective, fields, *case) {
                        track_changed_fields += 1;
                        if !dry_run {
                            write_user_tag(&tx, track_id, field, &next, now)?;
                        }
                    }
                }
                BatchTagOperation::ApplyReleaseFields { .. } => {
                    for (field, next) in &release_values {
                        if current_of(*field) == Some(next.as_str()) {
                            continue;
                        }
                        track_changed_fields += 1;
                        if !dry_run {
                            write_user_tag(&tx, track_id, *field, next, now)?;
                        }
                    }
                }
            }

            if track_changed_fields > 0 {
                report.tracks += 1;
                report.fields += track_changed_fields;
                if !dry_run {
                    refresh_track(&tx, track_id)?;
                }
            }
        }

        if !dry_run && report.fields > 0 {
            tx.commit()?;
        }
        Ok(report)
    }
}
