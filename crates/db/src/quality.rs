use std::collections::HashMap;

use resonance_core::identity::AssetQuality;
use resonance_core::quality::{
    FlagCertainty, QualityFlag, QualityKind, QualityMeasurements, worst,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{Database, DbError};

/// The asset's most serious flag as one text value: kind, certainty and reason split by the unit
/// separator, so a page of tracks reads it in its own query. Each flag is prefixed with a rank
/// digit kept in step with `worst`, and `min` picks the lowest without sorting.
macro_rules! worst_flag_sql {
    () => {
        "(SELECT substr(min(
                 (CASE certainty WHEN 'confirmed' THEN 0 ELSE 4 END
                     + CASE kind WHEN 'damaged' THEN 0 WHEN 'clipping' THEN 1
                         WHEN 'dropout' THEN 2 ELSE 3 END)
                 || kind || char(31) || certainty || char(31) || reason), 2)
             FROM quality_flag WHERE quality_flag.asset_id = asset.id)"
    };
}
pub(crate) use worst_flag_sql;

/// Reads a `worst_flag_sql!` value.
pub(crate) fn parse_worst_flag(value: Option<String>) -> Result<Option<QualityFlag>, DbError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let mut parts = value.splitn(3, '\u{1f}');
    let kind = parts.next().unwrap_or_default();
    let certainty = parts.next().unwrap_or_default();
    let reason = parts.next().unwrap_or_default().to_owned();
    read_flag(kind, certainty, reason).map(Some)
}

/// What the last check found for one asset.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredQuality {
    pub asset_id: i64,
    pub measurements: QualityMeasurements,
    pub flags: Vec<QualityFlag>,
    pub checked_ns: i64,
}

/// Replaces the asset's quality rows. Runs inside the caller's transaction, next to the identity
/// it was measured with.
pub(crate) fn record_quality(
    connection: &Connection,
    asset_id: i64,
    quality: &AssetQuality,
) -> Result<(), DbError> {
    let measured = &quality.measurements;
    connection.execute(
        "INSERT INTO asset_quality (asset_id, integrated_lufs, loudness_range_lu,
             sample_peak_dbfs, true_peak_dbtp, crest_low_db, crest_mid_db, clipped_runs,
             dropout_count, spectral_cutoff_hz, checked_ns)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
             CAST(unixepoch('subsec') * 1000000000 AS INTEGER))
         ON CONFLICT (asset_id) DO UPDATE SET
             integrated_lufs = excluded.integrated_lufs,
             loudness_range_lu = excluded.loudness_range_lu,
             sample_peak_dbfs = excluded.sample_peak_dbfs,
             true_peak_dbtp = excluded.true_peak_dbtp,
             crest_low_db = excluded.crest_low_db,
             crest_mid_db = excluded.crest_mid_db,
             clipped_runs = excluded.clipped_runs,
             dropout_count = excluded.dropout_count,
             spectral_cutoff_hz = excluded.spectral_cutoff_hz,
             checked_ns = excluded.checked_ns",
        params![
            asset_id,
            measured.integrated_lufs,
            measured.loudness_range_lu,
            measured.sample_peak_dbfs,
            measured.true_peak_dbtp,
            measured.crest_low_db,
            measured.crest_mid_db,
            measured.clipped_runs,
            measured.dropouts,
            measured.spectral_cutoff_hz,
        ],
    )?;
    connection.execute("DELETE FROM quality_flag WHERE asset_id = ?1", [asset_id])?;
    let mut insert = connection.prepare_cached(
        "INSERT OR IGNORE INTO quality_flag (asset_id, kind, certainty, reason)
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for flag in &quality.flags {
        insert.execute(params![
            asset_id,
            flag.kind.name(),
            flag.certainty.name(),
            flag.reason
        ])?;
    }
    Ok(())
}

fn read_flag(kind: &str, certainty: &str, reason: String) -> Result<QualityFlag, DbError> {
    let unknown = |column: &'static str, name: &str| DbError::UnknownName {
        column,
        name: name.to_owned(),
    };
    Ok(QualityFlag {
        kind: QualityKind::from_name(kind).ok_or_else(|| unknown("quality_flag.kind", kind))?,
        certainty: FlagCertainty::from_name(certainty)
            .ok_or_else(|| unknown("quality_flag.certainty", certainty))?,
        reason,
    })
}

impl Database {
    /// The quality of the asset a track plays, or `None` before it has been checked.
    pub fn track_quality(&self, track_id: i64) -> Result<Option<StoredQuality>, DbError> {
        let row = self
            .connection
            .query_row(
                "SELECT q.asset_id, q.integrated_lufs, q.loudness_range_lu, q.sample_peak_dbfs,
                     q.true_peak_dbtp, q.crest_low_db, q.crest_mid_db, q.clipped_runs,
                     q.dropout_count, q.spectral_cutoff_hz, q.checked_ns
                 FROM asset
                 JOIN asset_quality AS q ON q.asset_id = asset.id
                 WHERE asset.track_id = ?1
                 ORDER BY asset.is_primary DESC, asset.id ASC
                 LIMIT 1",
                [track_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        QualityMeasurements {
                            integrated_lufs: row.get(1)?,
                            loudness_range_lu: row.get(2)?,
                            sample_peak_dbfs: row.get(3)?,
                            true_peak_dbtp: row.get(4)?,
                            crest_low_db: row.get(5)?,
                            crest_mid_db: row.get(6)?,
                            clipped_runs: row.get(7)?,
                            dropouts: row.get(8)?,
                            first_dropout_ms: None,
                            spectral_cutoff_hz: row.get(9)?,
                        },
                        row.get::<_, i64>(10)?,
                    ))
                },
            )
            .optional()?;
        let Some((asset_id, measurements, checked_ns)) = row else {
            return Ok(None);
        };
        let flags = self
            .asset_flags(&[asset_id])?
            .remove(&asset_id)
            .unwrap_or_default();
        Ok(Some(StoredQuality {
            asset_id,
            measurements,
            flags,
            checked_ns,
        }))
    }

    /// The flags on each of `asset_ids`, most serious first. Assets with none are left out.
    pub fn asset_flags(
        &self,
        asset_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<QualityFlag>>, DbError> {
        let mut flags: HashMap<i64, Vec<QualityFlag>> = HashMap::new();
        let mut statement = self.connection.prepare_cached(
            "SELECT kind, certainty, reason FROM quality_flag WHERE asset_id = ?1",
        )?;
        for &asset_id in asset_ids {
            let rows = statement
                .query_map([asset_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if rows.is_empty() {
                continue;
            }
            let mut list = rows
                .into_iter()
                .map(|(kind, certainty, reason)| read_flag(&kind, &certainty, reason))
                .collect::<Result<Vec<_>, _>>()?;
            list.sort_by(|a, b| b.certainty.cmp(&a.certainty).then(a.kind.cmp(&b.kind)));
            flags.insert(asset_id, list);
        }
        Ok(flags)
    }

    /// The most serious flag of each asset, which the table shows.
    pub fn worst_flags(&self, asset_ids: &[i64]) -> Result<HashMap<i64, QualityFlag>, DbError> {
        Ok(self
            .asset_flags(asset_ids)?
            .into_iter()
            .filter_map(|(asset_id, flags)| worst(&flags).cloned().map(|flag| (asset_id, flag)))
            .collect())
    }
}
