use resonance_core::analysis::{
    AnalysisKey, AnalysisProvenance, AnalysisRun, AnalysisStatus, BeatGrid, decode_beats,
    encode_beats,
};
use resonance_core::identity::{AssetAlignment, ContentHash};
use rusqlite::{OptionalExtension, params};

use crate::{Database, DbError};

/// The grid a track shows, in track time, and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveBeatGrid {
    pub grid: BeatGrid,
    pub provenance: AnalysisProvenance,
}

impl Database {
    /// The run cached for this recording under `key`, if any.
    pub fn analysis_run(
        &self,
        audio_hash: &ContentHash,
        key: &AnalysisKey,
    ) -> Result<Option<AnalysisRun>, DbError> {
        let found = self
            .connection
            .query_row(
                "SELECT status, confidence, completed_ns FROM analysis_run
                 WHERE audio_hash = ?1 AND analyser = ?2 AND version = ?3 AND param_hash = ?4",
                params![
                    audio_hash.as_bytes().as_slice(),
                    key.analyser,
                    key.version,
                    key.param_hash.as_slice()
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<f64>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((status, confidence, completed_ns)) = found else {
            return Ok(None);
        };
        let status = AnalysisStatus::from_name(&status).ok_or(DbError::UnknownName {
            column: "analysis_run.status",
            name: status,
        })?;
        Ok(Some(AnalysisRun {
            key: key.clone(),
            status,
            confidence: confidence.map(|value| value as f32),
            completed_ns,
        }))
    }

    pub fn record_analysis_run(
        &mut self,
        audio_hash: &ContentHash,
        run: &AnalysisRun,
    ) -> Result<(), DbError> {
        self.connection.execute(
            "INSERT INTO analysis_run
                 (audio_hash, analyser, version, param_hash, status, confidence, completed_ns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (audio_hash, analyser, version, param_hash) DO UPDATE SET
                 status = excluded.status,
                 confidence = excluded.confidence,
                 completed_ns = excluded.completed_ns",
            params![
                audio_hash.as_bytes().as_slice(),
                run.key.analyser,
                run.key.version,
                run.key.param_hash.as_slice(),
                run.status.name(),
                run.confidence.map(f64::from),
                run.completed_ns
            ],
        )?;
        Ok(())
    }

    /// Replaces the automatic grid for a recording. A user's grid for any of its tracks stays.
    pub fn store_beat_grid_auto(
        &mut self,
        audio_hash: &ContentHash,
        grid: &BeatGrid,
        confidence: f32,
    ) -> Result<(), DbError> {
        self.connection.execute(
            "INSERT INTO beat_grid_auto (audio_hash, bpm, beats, first_downbeat, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (audio_hash) DO UPDATE SET
                 bpm = excluded.bpm,
                 beats = excluded.beats,
                 first_downbeat = excluded.first_downbeat,
                 confidence = excluded.confidence",
            params![
                audio_hash.as_bytes().as_slice(),
                grid.bpm,
                encode_beats(&grid.beats_s),
                grid.first_downbeat,
                f64::from(confidence)
            ],
        )?;
        Ok(())
    }

    /// Stores the user's grid for a track, in track time.
    pub fn set_beat_grid_user(&mut self, track_id: i64, grid: &BeatGrid) -> Result<(), DbError> {
        self.connection.execute(
            "INSERT INTO beat_grid_user (track_id, bpm, beats, first_downbeat, edited_ns)
             VALUES (?1, ?2, ?3, ?4, CAST(unixepoch('subsec') * 1000000000 AS INTEGER))
             ON CONFLICT (track_id) DO UPDATE SET
                 bpm = excluded.bpm,
                 beats = excluded.beats,
                 first_downbeat = excluded.first_downbeat,
                 edited_ns = excluded.edited_ns",
            params![
                track_id,
                grid.bpm,
                encode_beats(&grid.beats_s),
                grid.first_downbeat
            ],
        )?;
        Ok(())
    }

    /// The user's grid if there is one, else the automatic grid of the track's primary file
    /// moved into track time.
    pub fn effective_beat_grid(&self, track_id: i64) -> Result<Option<EffectiveBeatGrid>, DbError> {
        let user = self
            .connection
            .query_row(
                "SELECT bpm, beats, first_downbeat FROM beat_grid_user WHERE track_id = ?1",
                [track_id],
                read_grid,
            )
            .optional()?;
        if let Some(grid) = user {
            return Ok(Some(EffectiveBeatGrid {
                grid: grid?,
                provenance: AnalysisProvenance::ConfirmedByUser,
            }));
        }

        let auto = self
            .connection
            .query_row(
                "SELECT grid.bpm, grid.beats, grid.first_downbeat, grid.confidence,
                     asset.offset_samples, asset.tempo_ratio, asset.sample_rate
                 FROM asset
                 JOIN beat_grid_auto AS grid ON grid.audio_hash = asset.audio_hash
                 WHERE asset.track_id = ?1 AND asset.is_primary = 1 AND asset.included = 1",
                [track_id],
                |row| {
                    Ok((
                        read_grid(row)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, f64>(5)?,
                        row.get::<_, Option<u32>>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((grid, confidence, offset_samples, tempo_ratio, sample_rate)) = auto else {
            return Ok(None);
        };
        let grid = grid?;
        let alignment =
            AssetAlignment::new(offset_samples, tempo_ratio).unwrap_or(AssetAlignment::IDENTITY);
        let grid = match sample_rate {
            Some(rate) if rate > 0 => grid.asset_to_track(&alignment, rate),
            _ => grid,
        };
        Ok(Some(EffectiveBeatGrid {
            grid,
            provenance: AnalysisProvenance::Estimated {
                confidence: confidence as f32,
            },
        }))
    }
}

/// Reads `bpm, beats, first_downbeat` from the first three columns.
fn read_grid(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<BeatGrid, DbError>> {
    let bpm: f64 = row.get(0)?;
    let beats: Vec<u8> = row.get(1)?;
    let first_downbeat: u32 = row.get(2)?;
    Ok(decode_beats(&beats)
        .map(|beats_s| BeatGrid {
            bpm,
            beats_s,
            first_downbeat,
        })
        .ok_or(DbError::UnknownName {
            column: "beat_grid.beats",
            name: format!("{} bytes", beats.len()),
        }))
}
