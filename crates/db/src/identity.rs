use resonance_core::identity::{
    AssetAlignment, AssetIdentity, AudioHash, Chromaprint, ContentHash,
};
use resonance_core::library::{AudioFormat, is_asset_path_included};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::{Database, DbError, FileStamp, ScannedFile, SourceId, sources};

/// An asset whose identity has not been computed yet, or whose file or lossy decoder changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAsset {
    pub asset_id: i64,
    pub track_id: i64,
    pub source_id: SourceId,
    pub source_folder: String,
    pub path: String,
    pub format: AudioFormat,
    pub stamp: FileStamp,
    /// True when this asset has never had an identity recorded yet.
    pub is_new: bool,
}

/// Stored identity and location of an asset in the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAssetIdentity {
    pub asset_id: i64,
    pub track_id: i64,
    pub source_id: SourceId,
    pub source_folder: String,
    pub path: String,
    pub stamp: FileStamp,
    pub identity: Option<AssetIdentity>,
    pub is_primary: bool,
    pub offset_samples: i64,
    pub tempo_ratio_bits: u64,
}

impl StoredAssetIdentity {
    pub fn alignment(&self) -> AssetAlignment {
        AssetAlignment::new(self.offset_samples, f64::from_bits(self.tempo_ratio_bits))
            .unwrap_or(AssetAlignment::IDENTITY)
    }
}

/// Candidate existing asset that shares `byte_hash` or `audio_hash` with another file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityMatch {
    pub asset_id: i64,
    pub track_id: i64,
    pub source_id: SourceId,
    pub source_folder: String,
    pub path: String,
}

fn parse_format(name: String) -> Result<AudioFormat, DbError> {
    AudioFormat::from_name(&name).ok_or(DbError::UnknownName {
        column: "asset.format",
        name,
    })
}

fn parse_hash(
    bytes: Option<Vec<u8>>,
    column: &'static str,
) -> Result<Option<ContentHash>, DbError> {
    match bytes {
        None => Ok(None),
        Some(raw) => ContentHash::from_slice(&raw)
            .map(Some)
            .ok_or_else(|| DbError::UnknownName {
                column,
                name: format!("{} bytes (expected 32)", raw.len()),
            }),
    }
}

impl Database {
    /// True once at least one asset in the database carries a computed `byte_hash` or `audio_hash`.
    pub fn has_identified_assets(&self) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM asset WHERE byte_hash IS NOT NULL OR audio_hash IS NOT NULL LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// Assets with `id > after_asset_id` whose identity needs computing: new files, modified
    /// files, or lossy encodes whose `audio_decoder` differs from `current_decoder`.
    pub fn pending_identity_assets(
        &self,
        after_asset_id: i64,
        current_decoder: &str,
        limit: u32,
    ) -> Result<Vec<PendingAsset>, DbError> {
        let mut stmt = self.connection.prepare(
            "SELECT asset.id, asset.track_id, asset.source_id, source.folder, asset.path,
                 asset.format, asset.size_bytes, asset.modified_ns,
                 asset.identity_size_bytes IS NULL
             FROM asset
             JOIN source ON source.id = asset.source_id
             WHERE asset.id > ?1
               AND asset.included = 1
               AND (
                   asset.identity_size_bytes IS NULL
                   OR asset.identity_size_bytes != asset.size_bytes
                   OR asset.identity_modified_ns IS NULL
                   OR asset.identity_modified_ns != asset.modified_ns
                   OR (
                       asset.format IN ('mp3', 'm4a')
                       AND asset.audio_decoder IS NOT NULL
                       AND asset.audio_decoder != ?2
                   )
               )
             ORDER BY asset.id
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![after_asset_id, current_decoder, limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, u64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, bool>(8)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        rows.into_iter()
            .map(
                |(
                    asset_id,
                    track_id,
                    source_id,
                    source_folder,
                    path,
                    format_name,
                    size_bytes,
                    modified_ns,
                    is_new,
                )| {
                    Ok(PendingAsset {
                        asset_id,
                        track_id,
                        source_id: SourceId(source_id),
                        source_folder,
                        path,
                        format: parse_format(format_name)?,
                        stamp: FileStamp {
                            size_bytes,
                            modified_ns,
                        },
                        is_new,
                    })
                },
            )
            .collect()
    }

    /// Assets matching `byte_hash` (via the `asset_byte_hash` index).
    pub fn assets_with_byte_hash(
        &self,
        byte_hash: &ContentHash,
    ) -> Result<Vec<IdentityMatch>, DbError> {
        let mut stmt = self.connection.prepare(
            "SELECT asset.id, asset.track_id, asset.source_id, source.folder, asset.path
             FROM asset
             JOIN source ON source.id = asset.source_id
             WHERE asset.byte_hash = ?1
             ORDER BY asset.id",
        )?;
        let matches = stmt
            .query_map([byte_hash.as_bytes().as_slice()], |row| {
                Ok(IdentityMatch {
                    asset_id: row.get(0)?,
                    track_id: row.get(1)?,
                    source_id: SourceId(row.get(2)?),
                    source_folder: row.get(3)?,
                    path: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(matches)
    }

    /// Assets matching `audio_hash` (via the `asset_audio_hash` index).
    pub fn assets_with_audio_hash(
        &self,
        audio_hash: &ContentHash,
    ) -> Result<Vec<IdentityMatch>, DbError> {
        let mut stmt = self.connection.prepare(
            "SELECT asset.id, asset.track_id, asset.source_id, source.folder, asset.path
             FROM asset
             JOIN source ON source.id = asset.source_id
             WHERE asset.audio_hash = ?1
             ORDER BY asset.id",
        )?;
        let matches = stmt
            .query_map([audio_hash.as_bytes().as_slice()], |row| {
                Ok(IdentityMatch {
                    asset_id: row.get(0)?,
                    track_id: row.get(1)?,
                    source_id: SourceId(row.get(2)?),
                    source_folder: row.get(3)?,
                    path: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(matches)
    }

    /// Every asset in a source that already has a computed `byte_hash` or `audio_hash`.
    pub fn identified_assets_in_source(
        &self,
        source: SourceId,
    ) -> Result<Vec<IdentityMatch>, DbError> {
        let mut stmt = self.connection.prepare(
            "SELECT asset.id, asset.track_id, asset.source_id, source.folder, asset.path
             FROM asset
             JOIN source ON source.id = asset.source_id
             WHERE asset.source_id = ?1
               AND (asset.byte_hash IS NOT NULL OR asset.audio_hash IS NOT NULL)
             ORDER BY asset.id",
        )?;
        let rows = stmt
            .query_map([source.0], |row| {
                Ok(IdentityMatch {
                    asset_id: row.get(0)?,
                    track_id: row.get(1)?,
                    source_id: SourceId(row.get(2)?),
                    source_folder: row.get(3)?,
                    path: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Records the computed identity for `asset_id` if its `(size_bytes, modified_ns)` still match
    /// `stamp`. Returns `true` when updated.
    pub fn record_asset_identity(
        &mut self,
        asset_id: i64,
        stamp: FileStamp,
        identity: &AssetIdentity,
    ) -> Result<bool, DbError> {
        let audio_bytes = identity
            .audio
            .as_ref()
            .map(|a| a.hash.as_bytes().as_slice());
        let audio_decoder = identity.audio.as_ref().map(|a| a.decoder.as_str());
        let fp_encoded = identity.chromaprint.as_ref().map(|fp| fp.encoded.as_str());
        let fp_raw = identity.chromaprint.as_ref().map(Chromaprint::raw_le_bytes);
        let fp_duration = identity.chromaprint.as_ref().map(|fp| fp.duration_s);

        let changed = self.connection.execute(
            "UPDATE asset
             SET byte_hash = ?2,
                 audio_hash = ?3,
                 audio_decoder = ?4,
                 fingerprint = ?5,
                 fingerprint_raw = ?6,
                 fingerprint_duration_s = ?7,
                 identity_size_bytes = ?8,
                 identity_modified_ns = ?9
             WHERE id = ?1 AND size_bytes = ?8 AND modified_ns = ?9",
            params![
                asset_id,
                identity.byte_hash.as_bytes().as_slice(),
                audio_bytes,
                audio_decoder,
                fp_encoded,
                fp_raw.as_deref(),
                fp_duration,
                stamp.size_bytes,
                stamp.modified_ns,
            ],
        )?;
        Ok(changed > 0)
    }

    /// Reconciles an asset that was scanned as `new_asset_id` when `original_asset_id`'s file was
    /// moved or renamed to `new_asset_id`'s path: keeps `original_asset_id` and its `track_id`,
    /// copies the new path, facts, tags and identity onto it, and removes the temporary duplicate
    /// asset and track.
    pub fn reconcile_moved_asset(
        &mut self,
        original_asset_id: i64,
        new_asset_id: i64,
        identity: &AssetIdentity,
    ) -> Result<bool, DbError> {
        if original_asset_id == new_asset_id {
            return Ok(false);
        }
        let tx = self.connection.transaction()?;
        struct MovedAssetRow {
            new_track_id: i64,
            new_source_id: i64,
            new_path: String,
            size_bytes: u64,
            modified_ns: i64,
            format: String,
            duration_ms: Option<u64>,
            sample_rate: Option<u32>,
            channels: Option<u8>,
            bit_depth: Option<u8>,
            bitrate_kbps: Option<u32>,
            included: i64,
        }

        let new_row = tx
            .query_row(
                "SELECT track_id, source_id, path, size_bytes, modified_ns, format,
                     duration_ms, sample_rate, channels, bit_depth, bitrate_kbps, included
                 FROM asset WHERE id = ?1",
                [new_asset_id],
                |row| {
                    Ok(MovedAssetRow {
                        new_track_id: row.get(0)?,
                        new_source_id: row.get(1)?,
                        new_path: row.get(2)?,
                        size_bytes: row.get(3)?,
                        modified_ns: row.get(4)?,
                        format: row.get(5)?,
                        duration_ms: row.get(6)?,
                        sample_rate: row.get(7)?,
                        channels: row.get(8)?,
                        bit_depth: row.get(9)?,
                        bitrate_kbps: row.get(10)?,
                        included: row.get(11)?,
                    })
                },
            )
            .optional()?;
        let Some(MovedAssetRow {
            new_track_id,
            new_source_id,
            new_path,
            size_bytes,
            modified_ns,
            format,
            duration_ms,
            sample_rate,
            channels,
            bit_depth,
            bitrate_kbps,
            included,
        }) = new_row
        else {
            return Ok(false);
        };

        let exists_original: Option<i64> = tx
            .query_row(
                "SELECT track_id FROM asset WHERE id = ?1",
                [original_asset_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists_original.is_none() {
            return Ok(false);
        }

        let new_tags: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT field, value FROM file_tag WHERE asset_id = ?1")?;
            stmt.query_map([new_asset_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };

        // Remove the temporary asset first so the UNIQUE(source_id, path) slot is free.
        tx.execute("DELETE FROM asset WHERE id = ?1", [new_asset_id])?;
        let remaining_on_new_track: i64 = tx.query_row(
            "SELECT count(*) FROM asset WHERE track_id = ?1",
            [new_track_id],
            |row| row.get(0),
        )?;
        if remaining_on_new_track == 0 {
            tx.execute("DELETE FROM track WHERE id = ?1", [new_track_id])?;
        }

        let audio_bytes = identity
            .audio
            .as_ref()
            .map(|a| a.hash.as_bytes().as_slice());
        let audio_decoder = identity.audio.as_ref().map(|a| a.decoder.as_str());
        let fp_encoded = identity.chromaprint.as_ref().map(|fp| fp.encoded.as_str());
        let fp_raw = identity.chromaprint.as_ref().map(Chromaprint::raw_le_bytes);
        let fp_duration = identity.chromaprint.as_ref().map(|fp| fp.duration_s);

        tx.execute(
            "UPDATE asset
             SET source_id = ?2,
                 path = ?3,
                 size_bytes = ?4,
                 modified_ns = ?5,
                 format = ?6,
                 duration_ms = ?7,
                 sample_rate = ?8,
                 channels = ?9,
                 bit_depth = ?10,
                 bitrate_kbps = ?11,
                 included = ?12,
                 byte_hash = ?13,
                 audio_hash = ?14,
                 audio_decoder = ?15,
                 fingerprint = ?16,
                 fingerprint_raw = ?17,
                 fingerprint_duration_s = ?18,
                 identity_size_bytes = ?4,
                 identity_modified_ns = ?5
             WHERE id = ?1",
            params![
                original_asset_id,
                new_source_id,
                new_path,
                size_bytes,
                modified_ns,
                format,
                duration_ms,
                sample_rate,
                channels,
                bit_depth,
                bitrate_kbps,
                included,
                identity.byte_hash.as_bytes().as_slice(),
                audio_bytes,
                audio_decoder,
                fp_encoded,
                fp_raw.as_deref(),
                fp_duration,
            ],
        )?;

        tx.execute(
            "DELETE FROM file_tag WHERE asset_id = ?1",
            [original_asset_id],
        )?;
        for (field, value) in new_tags {
            tx.execute(
                "INSERT INTO file_tag (asset_id, field, value) VALUES (?1, ?2, ?3)",
                params![original_asset_id, field, value],
            )?;
        }

        tx.commit()?;
        Ok(true)
    }

    /// Moves an existing asset directly to `(new_source, file.path)` during a scan when its old
    /// path is absent on disk, keeping its `asset.id` and `track_id` intact.
    pub fn record_moved_scan_file(
        &mut self,
        asset_id: i64,
        new_source: SourceId,
        file: &ScannedFile,
        identity: Option<&AssetIdentity>,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        record_moved_in_tx(&tx, asset_id, new_source, file, identity)?;
        tx.commit()?;
        Ok(())
    }

    /// Reads the stored identity of `track_id`'s primary asset, if present.
    pub fn track_identity(&self, track_id: i64) -> Result<Option<StoredAssetIdentity>, DbError> {
        struct RawRow {
            asset_id: i64,
            track_id: i64,
            source_id: i64,
            source_folder: String,
            path: String,
            size_bytes: u64,
            modified_ns: i64,
            byte_hash_raw: Option<Vec<u8>>,
            audio_hash_raw: Option<Vec<u8>>,
            audio_decoder: Option<String>,
            fp_encoded: Option<String>,
            fp_raw: Option<Vec<u8>>,
            fp_duration: Option<u32>,
            is_primary: i64,
            offset_samples: i64,
            tempo_ratio: f64,
        }

        let Some(row) = self
            .connection
            .query_row(
                "SELECT asset.id, asset.track_id, asset.source_id, source.folder, asset.path,
                     asset.size_bytes, asset.modified_ns, asset.byte_hash, asset.audio_hash,
                     asset.audio_decoder, asset.fingerprint, asset.fingerprint_raw,
                     asset.fingerprint_duration_s, asset.is_primary, asset.offset_samples,
                     asset.tempo_ratio
                 FROM asset
                 JOIN source ON source.id = asset.source_id
                 WHERE asset.track_id = ?1
                 ORDER BY asset.is_primary DESC, asset.id ASC
                 LIMIT 1",
                [track_id],
                |row| {
                    Ok(RawRow {
                        asset_id: row.get(0)?,
                        track_id: row.get(1)?,
                        source_id: row.get(2)?,
                        source_folder: row.get(3)?,
                        path: row.get(4)?,
                        size_bytes: row.get(5)?,
                        modified_ns: row.get(6)?,
                        byte_hash_raw: row.get(7)?,
                        audio_hash_raw: row.get(8)?,
                        audio_decoder: row.get(9)?,
                        fp_encoded: row.get(10)?,
                        fp_raw: row.get(11)?,
                        fp_duration: row.get(12)?,
                        is_primary: row.get(13)?,
                        offset_samples: row.get(14)?,
                        tempo_ratio: row.get(15)?,
                    })
                },
            )
            .optional()?
        else {
            return Ok(None);
        };

        let byte_hash = parse_hash(row.byte_hash_raw, "asset.byte_hash")?;
        let audio_hash = parse_hash(row.audio_hash_raw, "asset.audio_hash")?;
        let identity = byte_hash.map(|byte_hash| {
            let audio = audio_hash
                .zip(row.audio_decoder)
                .map(|(hash, decoder)| AudioHash { hash, decoder });
            let chromaprint = match (row.fp_encoded, row.fp_raw, row.fp_duration) {
                (Some(encoded), Some(raw_bytes), Some(duration_s)) => {
                    Chromaprint::raw_from_le_bytes(&raw_bytes).map(|raw| Chromaprint {
                        encoded,
                        raw,
                        duration_s,
                    })
                }
                _ => None,
            };
            AssetIdentity {
                byte_hash,
                audio,
                chromaprint,
                decoded_facts: None,
            }
        });

        Ok(Some(StoredAssetIdentity {
            asset_id: row.asset_id,
            track_id: row.track_id,
            source_id: SourceId(row.source_id),
            source_folder: row.source_folder,
            path: row.path,
            stamp: FileStamp {
                size_bytes: row.size_bytes,
                modified_ns: row.modified_ns,
            },
            identity,
            is_primary: row.is_primary != 0,
            offset_samples: row.offset_samples,
            tempo_ratio_bits: row.tempo_ratio.to_bits(),
        }))
    }
}

pub(crate) fn record_moved_in_tx(
    tx: &Transaction<'_>,
    asset_id: i64,
    new_source: SourceId,
    file: &ScannedFile,
    identity: Option<&AssetIdentity>,
) -> Result<(), DbError> {
    let (include_subfolders, excluded_subfolders, ignored_folders) =
        sources::source_inclusion_in_tx(tx, new_source)?;
    let included = i64::from(is_asset_path_included(
        &file.path,
        include_subfolders,
        &excluded_subfolders,
        &ignored_folders,
    ));
    let audio = file.audio;
    let duration_ms = audio.map(|a| a.duration_ms);
    let sample_rate = audio.and_then(|a| a.sample_rate);
    let channels = audio.and_then(|a| a.channels);
    let bit_depth = audio.and_then(|a| a.bit_depth);
    let bitrate_kbps = audio.and_then(|a| a.bitrate_kbps);

    if let Some(id) = identity {
        let audio_bytes = id.audio.as_ref().map(|a| a.hash.as_bytes().as_slice());
        let audio_decoder = id.audio.as_ref().map(|a| a.decoder.as_str());
        let fp_encoded = id.chromaprint.as_ref().map(|fp| fp.encoded.as_str());
        let fp_raw = id.chromaprint.as_ref().map(Chromaprint::raw_le_bytes);
        let fp_duration = id.chromaprint.as_ref().map(|fp| fp.duration_s);

        tx.execute(
            "UPDATE asset
             SET source_id = ?2,
                 path = ?3,
                 size_bytes = ?4,
                 modified_ns = ?5,
                 format = ?6,
                 duration_ms = ?7,
                 sample_rate = ?8,
                 channels = ?9,
                 bit_depth = ?10,
                 bitrate_kbps = ?11,
                 included = ?12,
                 byte_hash = ?13,
                 audio_hash = ?14,
                 audio_decoder = ?15,
                 fingerprint = ?16,
                 fingerprint_raw = ?17,
                 fingerprint_duration_s = ?18,
                 identity_size_bytes = ?4,
                 identity_modified_ns = ?5
             WHERE id = ?1",
            params![
                asset_id,
                new_source.0,
                file.path,
                file.size_bytes,
                file.modified_ns,
                file.format.name(),
                duration_ms,
                sample_rate,
                channels,
                bit_depth,
                bitrate_kbps,
                included,
                id.byte_hash.as_bytes().as_slice(),
                audio_bytes,
                audio_decoder,
                fp_encoded,
                fp_raw.as_deref(),
                fp_duration,
            ],
        )?;
    } else {
        tx.execute(
            "UPDATE asset
             SET source_id = ?2,
                 path = ?3,
                 size_bytes = ?4,
                 modified_ns = ?5,
                 format = ?6,
                 duration_ms = ?7,
                 sample_rate = ?8,
                 channels = ?9,
                 bit_depth = ?10,
                 bitrate_kbps = ?11,
                 included = ?12,
                 identity_size_bytes = ?4,
                 identity_modified_ns = ?5
             WHERE id = ?1",
            params![
                asset_id,
                new_source.0,
                file.path,
                file.size_bytes,
                file.modified_ns,
                file.format.name(),
                duration_ms,
                sample_rate,
                channels,
                bit_depth,
                bitrate_kbps,
                included,
            ],
        )?;
    }

    tx.execute("DELETE FROM file_tag WHERE asset_id = ?1", [asset_id])?;
    for (field, value) in &file.tags {
        tx.execute(
            "INSERT INTO file_tag (asset_id, field, value) VALUES (?1, ?2, ?3)",
            params![asset_id, field.name(), value],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use resonance_core::library::{AudioFacts, ScannedFile, TagField};
    use rusqlite::Connection;

    use super::*;
    use crate::SCHEMA_VERSION;

    fn sample_file(path: &str, format: AudioFormat) -> ScannedFile {
        ScannedFile {
            path: path.to_owned(),
            size_bytes: 5_000_000,
            modified_ns: 1_700_000_000_000_000_000,
            format,
            audio: Some(AudioFacts {
                duration_ms: 240_000,
                sample_rate: Some(44_100),
                channels: Some(2),
                bit_depth: Some(16),
                bitrate_kbps: Some(900),
            }),
            tags: vec![(TagField::Title, "Test Track".to_owned())],
        }
    }

    #[test]
    fn migrates_v1_database_and_uses_hash_indices_in_query_plans() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp/identity_v1_migration");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir created");
        let db_path = dir.join("library.db");

        {
            let conn = Connection::open(&db_path).expect("raw sqlite opens");
            conn.execute_batch(include_str!("../migrations/0001_library.sql"))
                .expect("v1 schema runs");
            conn.pragma_update(None, "user_version", 1u32)
                .expect("user_version set");
            conn.execute("INSERT INTO source (folder) VALUES ('D:/Music')", [])
                .expect("source inserted");
            conn.execute("INSERT INTO track DEFAULT VALUES", [])
                .expect("track inserted");
            conn.execute(
                "INSERT INTO asset (track_id, source_id, path, size_bytes, modified_ns, format)
                 VALUES (1, 1, 'Club/opener.flac', 123456, 789, 'flac')",
                [],
            )
            .expect("asset inserted");
            conn.execute(
                "INSERT INTO file_tag (asset_id, field, value) VALUES (1, 'title', 'Opener')",
                [],
            )
            .expect("tag inserted");
        }

        let mut db = Database::open(&db_path).expect("database migrates to v3");
        assert_eq!(db.schema_version().expect("version"), SCHEMA_VERSION);

        let tracks = db.tracks().expect("tracks read");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].file.title(), "Opener");

        let stored = db
            .track_identity(tracks[0].track_id)
            .expect("identity query")
            .expect("primary asset present");
        assert!(stored.is_primary);
        assert_eq!(stored.alignment(), AssetAlignment::IDENTITY);
        assert!(stored.identity.is_none());

        let pending = db
            .pending_identity_assets(0, "symphonia/0.6.1", 10)
            .expect("pending read");
        assert_eq!(pending.len(), 1);
        assert!(pending[0].is_new);

        let id = AssetIdentity {
            byte_hash: ContentHash::from_bytes([0x11; 32]),
            audio: Some(AudioHash {
                hash: ContentHash::from_bytes([0x22; 32]),
                decoder: "symphonia/0.6.1".to_owned(),
            }),
            chromaprint: Some(Chromaprint {
                encoded: "AQAA".to_owned(),
                raw: vec![1, 2, 3],
                duration_s: 120,
            }),
            decoded_facts: None,
        };
        assert!(
            db.record_asset_identity(pending[0].asset_id, pending[0].stamp, &id)
                .expect("identity recorded")
        );

        let byte_plan: String = db
            .connection
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM asset WHERE byte_hash = ?1",
                [id.byte_hash.as_bytes().as_slice()],
                |row| row.get(3),
            )
            .expect("byte_hash plan");
        assert!(
            byte_plan.contains("asset_byte_hash"),
            "expected asset_byte_hash index in plan, got: {byte_plan}"
        );

        let audio_plan: String = db
            .connection
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM asset WHERE audio_hash = ?1",
                [[0x22u8; 32].as_slice()],
                |row| row.get(3),
            )
            .expect("audio_hash plan");
        assert!(
            audio_plan.contains("asset_audio_hash"),
            "expected asset_audio_hash index in plan, got: {audio_plan}"
        );
    }

    #[test]
    fn lossy_assets_are_requeued_on_decoder_upgrade_while_lossless_assets_stay_settled() {
        let mut db = Database::open_in_memory().expect("in-memory db");
        let source = db.source("music").expect("source");
        db.record_scan(
            source,
            &[
                sample_file("lossless.flac", AudioFormat::Flac),
                sample_file("lossy.mp3", AudioFormat::Mp3),
            ],
        )
        .expect("scan stored");

        let initial = db
            .pending_identity_assets(0, "symphonia/0.6.0", 10)
            .expect("initial pending");
        assert_eq!(initial.len(), 2);

        for (index, item) in initial.iter().enumerate() {
            let tag_byte = (index as u8) + 1;
            let identity = AssetIdentity {
                byte_hash: ContentHash::from_bytes([tag_byte; 32]),
                audio: Some(AudioHash {
                    hash: ContentHash::from_bytes([tag_byte + 10; 32]),
                    decoder: "symphonia/0.6.0".to_owned(),
                }),
                chromaprint: None,
                decoded_facts: None,
            };
            assert!(
                db.record_asset_identity(item.asset_id, item.stamp, &identity)
                    .expect("recorded")
            );
        }

        assert!(
            db.pending_identity_assets(0, "symphonia/0.6.0", 10)
                .expect("same decoder")
                .is_empty()
        );

        let upgraded = db
            .pending_identity_assets(0, "symphonia/0.6.1", 10)
            .expect("upgraded decoder");
        assert_eq!(upgraded.len(), 1);
        assert_eq!(upgraded[0].path, "lossy.mp3");
        assert!(!upgraded[0].is_new);
    }
}
