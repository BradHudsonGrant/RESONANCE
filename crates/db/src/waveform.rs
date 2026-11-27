use resonance_core::identity::AssetIdentity;
use resonance_core::waveform::Overview;
use rusqlite::{Connection, OptionalExtension, params};

use crate::{Database, DbError};

/// Keeps the overview the identity pass drew, under the recording's audio hash.
pub(crate) fn record_overview(
    connection: &Connection,
    identity: &AssetIdentity,
) -> Result<(), DbError> {
    let (Some(audio), Some(overview)) = (&identity.audio, &identity.overview) else {
        return Ok(());
    };
    connection.execute(
        "INSERT INTO waveform_overview (audio_hash, columns) VALUES (?1, ?2)
         ON CONFLICT (audio_hash) DO UPDATE SET columns = excluded.columns",
        params![audio.hash.as_bytes().as_slice(), overview.to_bytes()],
    )?;
    Ok(())
}

impl Database {
    /// The overview of each track's primary file, in the order asked for. `None` for a track
    /// that is gone or whose file has not been read yet.
    pub fn waveform_overviews(&self, track_ids: &[i64]) -> Result<Vec<Option<Overview>>, DbError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT waveform_overview.columns
             FROM asset
             JOIN waveform_overview ON waveform_overview.audio_hash = asset.audio_hash
             WHERE asset.track_id = ?1
             ORDER BY asset.is_primary DESC, asset.id
             LIMIT 1",
        )?;
        track_ids
            .iter()
            .map(|track_id| {
                let columns = statement
                    .query_row([track_id], |row| row.get::<_, Vec<u8>>(0))
                    .optional()?;
                Ok(columns.and_then(|bytes| Overview::from_bytes(&bytes)))
            })
            .collect()
    }
}
