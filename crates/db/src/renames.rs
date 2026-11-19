//! Files a rename operation moves, and the asset paths that follow them.

use resonance_core::library::TagField;
use resonance_core::staging::effective_tags;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::search::sync_asset_metadata_in_tx;
use crate::tags::{file_tags_of_asset, primary_asset, user_tags_of_asset};
use crate::{Database, DbError};

/// A file a rename may move, with the values its name is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameTarget {
    pub asset_id: i64,
    pub track_id: i64,
    pub source_id: i64,
    pub source_folder: String,
    /// Relative to the source folder, with forward slashes.
    pub path: String,
    pub missing: bool,
    /// The values the track shows: the user's over the file's.
    pub tags: Vec<(TagField, String)>,
}

/// One file an operation renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRename {
    pub asset_id: i64,
    pub from_path: String,
    pub to_path: String,
    pub undone: bool,
}

fn rename_target(connection: &Connection, asset_id: i64) -> Result<Option<RenameTarget>, DbError> {
    let row = connection
        .prepare_cached(
            "SELECT asset.track_id, asset.source_id, source.folder, asset.path,
                 asset.missing_since IS NOT NULL
             FROM asset JOIN source ON source.id = asset.source_id
             WHERE asset.id = ?1",
        )?
        .query_row([asset_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })
        .optional()?;
    let Some((track_id, source_id, source_folder, path, missing)) = row else {
        return Ok(None);
    };
    let file = file_tags_of_asset(connection, asset_id)?;
    let user = user_tags_of_asset(connection, asset_id)?;
    Ok(Some(RenameTarget {
        asset_id,
        track_id,
        source_id,
        source_folder,
        path,
        missing,
        tags: effective_tags(&file, &user),
    }))
}

fn set_asset_path(tx: &Transaction<'_>, asset_id: i64, path: &str) -> Result<(), DbError> {
    tx.execute(
        "UPDATE asset SET path = ?2 WHERE id = ?1",
        params![asset_id, path],
    )?;
    let tags = file_tags_of_asset(tx, asset_id)?;
    sync_asset_metadata_in_tx(tx, asset_id, path, &tags, true)
}

impl Database {
    /// The primary file of each track, in the order given, skipping tracks without one.
    pub fn primary_asset_ids(&self, track_ids: &[i64]) -> Result<Vec<i64>, DbError> {
        let mut assets = Vec::with_capacity(track_ids.len());
        for track_id in track_ids {
            assets.extend(primary_asset(&self.connection, *track_id)?);
        }
        Ok(assets)
    }

    pub fn rename_target(&self, asset_id: i64) -> Result<Option<RenameTarget>, DbError> {
        rename_target(&self.connection, asset_id)
    }

    /// The file names the library holds in one folder of a source, missing files included.
    /// `folder` is relative to the source, with forward slashes, and empty for its top.
    pub fn asset_names_in_folder(
        &self,
        source_id: i64,
        folder: &str,
    ) -> Result<Vec<String>, DbError> {
        let prefix = if folder.is_empty() {
            String::new()
        } else {
            format!("{folder}/")
        };
        let mut statement = self.connection.prepare_cached(
            "SELECT substr(path, length(?2) + 1) FROM asset
             WHERE source_id = ?1 AND substr(path, 1, length(?2)) = ?2
                 AND instr(substr(path, length(?2) + 1), '/') = 0",
        )?;
        let names = statement
            .query_map(params![source_id, prefix], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(names)
    }

    /// Moves the asset to `to_path` and journals the move under `operation_id`, in one
    /// transaction, once the file has been renamed on disk.
    pub fn record_asset_rename(
        &mut self,
        operation_id: i64,
        asset_id: i64,
        from_path: &str,
        to_path: &str,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        set_asset_path(&tx, asset_id, to_path)?;
        tx.execute(
            "INSERT INTO operation_rename (operation_id, asset_id, from_path, to_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![operation_id, asset_id, from_path, to_path],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Puts the asset back at its old path once the file has been renamed back on disk.
    pub fn record_rename_undone(
        &mut self,
        operation_id: i64,
        asset_id: i64,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        let from_path: String = tx.query_row(
            "SELECT from_path FROM operation_rename WHERE operation_id = ?1 AND asset_id = ?2",
            params![operation_id, asset_id],
            |row| row.get(0),
        )?;
        set_asset_path(&tx, asset_id, &from_path)?;
        tx.execute(
            "UPDATE operation_rename SET undone = 1 WHERE operation_id = ?1 AND asset_id = ?2",
            params![operation_id, asset_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The files an operation renamed, in the order it renamed them.
    pub fn operation_renames(&self, operation_id: i64) -> Result<Vec<StoredRename>, DbError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT asset_id, from_path, to_path, undone FROM operation_rename
             WHERE operation_id = ?1 ORDER BY rowid",
        )?;
        let renames = statement
            .query_map([operation_id], |row| {
                Ok(StoredRename {
                    asset_id: row.get(0)?,
                    from_path: row.get(1)?,
                    to_path: row.get(2)?,
                    undone: row.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(renames)
    }
}
