use std::collections::HashMap;

use resonance_core::library::{
    is_asset_path_included, normalise_excluded_subfolders, normalise_ignored_folders,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::{Database, DbError, SourceId};

/// A music folder in the library and how scans and watchers treat its subfolders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceConfig {
    pub id: SourceId,
    pub folder: String,
    pub include_subfolders: bool,
    pub watch: bool,
    pub excluded_subfolders: Vec<String>,
    pub track_count: u32,
}

pub(crate) fn source_inclusion_in_tx(
    transaction: &Transaction<'_>,
    source: SourceId,
) -> Result<(bool, Vec<String>, Vec<String>), DbError> {
    let include_subfolders: i64 = transaction.query_row(
        "SELECT include_subfolders FROM source WHERE id = ?1",
        [source.0],
        |row| row.get(0),
    )?;
    let mut excluded_stmt = transaction
        .prepare("SELECT path FROM source_excluded_folder WHERE source_id = ?1 ORDER BY path")?;
    let excluded: Vec<String> = excluded_stmt
        .query_map([source.0], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    let ignored = ignored_folders_in_tx(transaction)?;
    Ok((include_subfolders != 0, excluded, ignored))
}

fn ignored_folders_in_tx(transaction: &Transaction<'_>) -> Result<Vec<String>, DbError> {
    let mut statement = transaction.prepare("SELECT name FROM ignored_folder ORDER BY position")?;
    let names = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names)
}

fn recompute_source_included(
    transaction: &Transaction<'_>,
    source: SourceId,
    include_subfolders: bool,
    excluded_subfolders: &[String],
    ignored_folders: &[String],
) -> Result<(), DbError> {
    let mut select =
        transaction.prepare("SELECT id, path, included FROM asset WHERE source_id = ?1")?;
    let mut rows = select.query([source.0])?;
    let mut updates: Vec<(i64, i64)> = Vec::new();
    while let Some(row) = rows.next()? {
        let asset_id: i64 = row.get(0)?;
        let path: String = row.get(1)?;
        let current: i64 = row.get(2)?;
        let next = i64::from(is_asset_path_included(
            &path,
            include_subfolders,
            excluded_subfolders,
            ignored_folders,
        ));
        if next != current {
            updates.push((asset_id, next));
        }
    }
    drop(rows);
    drop(select);

    if !updates.is_empty() {
        let mut update = transaction.prepare("UPDATE asset SET included = ?2 WHERE id = ?1")?;
        for (asset_id, included) in updates {
            update.execute(params![asset_id, included])?;
        }
    }
    Ok(())
}

impl Database {
    /// Every configured library source in folder order, with its options and included track count.
    pub fn sources(&self) -> Result<Vec<SourceConfig>, DbError> {
        let mut excluded_by_source: HashMap<i64, Vec<String>> = HashMap::new();
        {
            let mut stmt = self.connection.prepare(
                "SELECT source_id, path FROM source_excluded_folder ORDER BY source_id, path",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                excluded_by_source
                    .entry(row.get(0)?)
                    .or_default()
                    .push(row.get(1)?);
            }
        }

        let mut counts: HashMap<i64, u32> = HashMap::new();
        {
            let mut stmt = self.connection.prepare(
                "SELECT source_id, count(*) FROM asset WHERE included = 1 GROUP BY source_id",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                counts.insert(row.get(0)?, row.get(1)?);
            }
        }

        let mut stmt = self
            .connection
            .prepare("SELECT id, folder, include_subfolders, watch FROM source ORDER BY folder")?;
        let sources = stmt
            .query_map([], |row| {
                let raw_id: i64 = row.get(0)?;
                let include: i64 = row.get(2)?;
                let watch: i64 = row.get(3)?;
                Ok(SourceConfig {
                    id: SourceId(raw_id),
                    folder: row.get(1)?,
                    include_subfolders: include != 0,
                    watch: watch != 0,
                    excluded_subfolders: excluded_by_source.remove(&raw_id).unwrap_or_default(),
                    track_count: counts.remove(&raw_id).unwrap_or(0),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(sources)
    }

    /// One configured source by id, if it exists.
    pub fn source_config(&self, id: SourceId) -> Result<Option<SourceConfig>, DbError> {
        let row = self
            .connection
            .query_row(
                "SELECT folder, include_subfolders, watch FROM source WHERE id = ?1",
                [id.0],
                |row| {
                    let include: i64 = row.get(1)?;
                    let watch: i64 = row.get(2)?;
                    Ok((row.get::<_, String>(0)?, include != 0, watch != 0))
                },
            )
            .optional()?;
        let Some((folder, include_subfolders, watch)) = row else {
            return Ok(None);
        };
        let mut excluded_stmt = self.connection.prepare(
            "SELECT path FROM source_excluded_folder WHERE source_id = ?1 ORDER BY path",
        )?;
        let excluded_subfolders = excluded_stmt
            .query_map([id.0], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        let track_count: u32 = self.connection.query_row(
            "SELECT count(*) FROM asset WHERE source_id = ?1 AND included = 1",
            [id.0],
            |row| row.get(0),
        )?;
        Ok(Some(SourceConfig {
            id,
            folder,
            include_subfolders,
            watch,
            excluded_subfolders,
            track_count,
        }))
    }

    /// The editable list of folder names skipped at any depth inside a source.
    pub fn ignored_folders(&self) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .connection
            .prepare("SELECT name FROM ignored_folder ORDER BY position")?;
        let names = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(names)
    }

    /// Replaces the editable ignored-folder list and updates asset inclusion across every source.
    pub fn set_ignored_folders<S: AsRef<str>>(
        &mut self,
        names: &[S],
    ) -> Result<Vec<String>, DbError> {
        let cleaned = normalise_ignored_folders(names);
        let transaction = self.connection.transaction()?;
        {
            transaction.execute("DELETE FROM ignored_folder", [])?;
            let mut insert = transaction
                .prepare("INSERT INTO ignored_folder (position, name) VALUES (?1, ?2)")?;
            for (position, name) in cleaned.iter().enumerate() {
                insert.execute(params![position as i64, name])?;
            }

            let sources: Vec<(i64, bool)> = {
                let mut stmt = transaction.prepare("SELECT id, include_subfolders FROM source")?;
                stmt.query_map([], |row| {
                    let include: i64 = row.get(1)?;
                    Ok((row.get(0)?, include != 0))
                })?
                .collect::<Result<_, _>>()?
            };
            let mut excluded_stmt = transaction.prepare(
                "SELECT path FROM source_excluded_folder WHERE source_id = ?1 ORDER BY path",
            )?;
            for (raw_id, include_subfolders) in sources {
                let excluded: Vec<String> = excluded_stmt
                    .query_map([raw_id], |row| row.get(0))?
                    .collect::<Result<_, _>>()?;
                recompute_source_included(
                    &transaction,
                    SourceId(raw_id),
                    include_subfolders,
                    &excluded,
                    &cleaned,
                )?;
            }
        }
        transaction.commit()?;
        Ok(cleaned)
    }

    /// Updates a source's subfolder and watcher settings and recomputes which of its scanned
    /// assets remain included in the library.
    pub fn update_source<S: AsRef<str>>(
        &mut self,
        id: SourceId,
        include_subfolders: bool,
        watch: bool,
        excluded_subfolders: &[S],
    ) -> Result<Option<SourceConfig>, DbError> {
        let cleaned_excluded = normalise_excluded_subfolders(excluded_subfolders);
        let transaction = self.connection.transaction()?;
        {
            let changed = transaction.execute(
                "UPDATE source SET include_subfolders = ?2, watch = ?3 WHERE id = ?1",
                params![id.0, i64::from(include_subfolders), i64::from(watch)],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            transaction.execute(
                "DELETE FROM source_excluded_folder WHERE source_id = ?1",
                [id.0],
            )?;
            let mut insert = transaction
                .prepare("INSERT INTO source_excluded_folder (source_id, path) VALUES (?1, ?2)")?;
            for path in &cleaned_excluded {
                insert.execute(params![id.0, path])?;
            }
            let ignored = ignored_folders_in_tx(&transaction)?;
            recompute_source_included(
                &transaction,
                id,
                include_subfolders,
                &cleaned_excluded,
                &ignored,
            )?;
        }
        transaction.commit()?;
        self.source_config(id)
    }

    /// Removes a source and its assets, plus any tracks that had no assets outside that source.
    pub fn remove_source(&mut self, id: SourceId) -> Result<bool, DbError> {
        let transaction = self.connection.transaction()?;
        let removed = {
            transaction.execute("DELETE FROM asset WHERE source_id = ?1", [id.0])?;
            let removed = transaction.execute("DELETE FROM source WHERE id = ?1", [id.0])?;
            if removed > 0 {
                transaction.execute(
                    "DELETE FROM track WHERE NOT EXISTS (
                         SELECT 1 FROM asset WHERE asset.track_id = track.id
                     )",
                    [],
                )?;
            }
            removed > 0
        };
        transaction.commit()?;
        Ok(removed)
    }
}
