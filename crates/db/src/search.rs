use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use resonance_core::key::Key;
use resonance_core::library::{TagField, bpm_from_tags, title_from_tags};
use resonance_core::search::{
    FilterPill, SortDirection, TrackQuery, TrackSortField, build_fts_match_query,
    key_filter_values, parse_year_tag,
};
use resonance_core::staging::effective_tags;
use rusqlite::types::Value;
use rusqlite::{Connection, Transaction, params, params_from_iter};

use crate::{Database, DbError, TrackRow, tag_field, track_row};

const RECENT_WINDOW_NS: i64 = 30 * 24 * 60 * 60 * 1_000_000_000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryFacets {
    pub labels: Vec<String>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub missing_count: u32,
}

pub(crate) struct AssetSearchMeta<'a> {
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub genre: Option<&'a str>,
    pub label: Option<&'a str>,
    pub comment: Option<&'a str>,
    pub tag_key: Option<String>,
    pub sort_key: Option<String>,
    pub tag_bpm: Option<f64>,
    pub tag_year: Option<i32>,
}

impl<'a> AssetSearchMeta<'a> {
    pub(crate) fn from_path_and_tags(path: &'a str, tags: &'a [(TagField, String)]) -> Self {
        let trimmed_tag = |target: TagField| -> Option<&'a str> {
            tags.iter()
                .find(|(field, _)| *field == target)
                .map(|(_, value)| value.trim())
                .filter(|value| !value.is_empty())
        };

        let key_raw = trimmed_tag(TagField::Key);
        let parsed_key = key_raw.and_then(|value| value.parse::<Key>().ok());
        let tag_key = parsed_key
            .map(|key| key.camelot())
            .or_else(|| key_raw.map(str::to_owned));
        let sort_key = parsed_key.map(Key::sort_code);

        Self {
            title: title_from_tags(path, tags),
            artist: trimmed_tag(TagField::Artist),
            album: trimmed_tag(TagField::Album),
            genre: trimmed_tag(TagField::Genre),
            label: trimmed_tag(TagField::Label),
            comment: trimmed_tag(TagField::Comment),
            tag_key,
            sort_key,
            tag_bpm: bpm_from_tags(tags),
            tag_year: trimmed_tag(TagField::Year).and_then(parse_year_tag),
        }
    }
}

pub(crate) fn now_unix_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
        })
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Refreshes the asset's sort, filter and search columns from its file `tags` with the user's
/// values for its track laid over them.
pub(crate) fn sync_asset_metadata_in_tx(
    tx: &Transaction<'_>,
    asset_id: i64,
    path: &str,
    tags: &[(TagField, String)],
    delete_existing_fts: bool,
) -> Result<(), DbError> {
    let user = crate::tags::user_tags_of_asset(tx, asset_id)?;
    if user.is_empty() {
        return write_asset_metadata(tx, asset_id, path, tags, delete_existing_fts);
    }
    let shown = effective_tags(tags, &user);
    write_asset_metadata(tx, asset_id, path, &shown, delete_existing_fts)
}

fn write_asset_metadata(
    tx: &Transaction<'_>,
    asset_id: i64,
    path: &str,
    tags: &[(TagField, String)],
    delete_existing_fts: bool,
) -> Result<(), DbError> {
    let meta = AssetSearchMeta::from_path_and_tags(path, tags);
    tx.execute(
        "UPDATE asset
         SET sort_title = ?2,
             sort_artist = ?3,
             tag_album = ?4,
             tag_genre = ?5,
             tag_label = ?6,
             tag_key = ?7,
             sort_key = ?8,
             tag_bpm = ?9,
             tag_year = ?10
         WHERE id = ?1",
        params![
            asset_id,
            meta.title,
            meta.artist,
            meta.album,
            meta.genre,
            meta.label,
            meta.tag_key,
            meta.sort_key,
            meta.tag_bpm,
            meta.tag_year,
        ],
    )?;
    if delete_existing_fts {
        tx.execute("DELETE FROM asset_fts WHERE rowid = ?1", [asset_id])?;
    }
    tx.execute(
        "INSERT INTO asset_fts (rowid, title, artist, album, genre, label, comment, path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            asset_id,
            meta.title,
            meta.artist.unwrap_or(""),
            meta.album.unwrap_or(""),
            meta.genre.unwrap_or(""),
            meta.label.unwrap_or(""),
            meta.comment.unwrap_or(""),
            path,
        ],
    )?;
    Ok(())
}

/// Refines v1–v3 migrated rows with Rust's exact title, BPM, key and year normalisation so
/// migrated databases match freshly scanned ones.
pub(crate) fn backfill_v4_metadata(tx: &Transaction<'_>) -> Result<(), DbError> {
    let mut assets_stmt = tx.prepare("SELECT id, path FROM asset ORDER BY id")?;
    let assets: Vec<(i64, String)> = assets_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    if assets.is_empty() {
        return Ok(());
    }

    let mut tags_by_asset: HashMap<i64, Vec<(TagField, String)>> = HashMap::new();
    let mut tags_stmt = tx.prepare("SELECT asset_id, field, value FROM file_tag")?;
    let mut tag_rows = tags_stmt.query([])?;
    while let Some(row) = tag_rows.next()? {
        tags_by_asset
            .entry(row.get(0)?)
            .or_default()
            .push((tag_field(row.get(1)?)?, row.get(2)?));
    }
    drop(tag_rows);
    drop(tags_stmt);
    drop(assets_stmt);

    for (asset_id, path) in assets {
        let tags = tags_by_asset.remove(&asset_id).unwrap_or_default();
        // The user layer does not exist yet at this version.
        write_asset_metadata(tx, asset_id, &path, &tags, true)?;
    }
    Ok(())
}

impl Database {
    /// Up to `limit` included tracks matching `query`, ordered by `query.sort` and `query.direction`.
    pub fn query_tracks_page(
        &self,
        query: &TrackQuery,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<TrackRow>, DbError> {
        let (sql, mut params) = build_query_sql(query, false);
        let limit_bind = bind_param(&mut params, Value::Integer(i64::from(limit)));
        let offset_bind = bind_param(&mut params, Value::Integer(i64::from(offset)));
        let sql = format!("{sql} LIMIT {limit_bind} OFFSET {offset_bind}");

        let mut stmt = self.connection.prepare(&sql)?;
        let ids: Vec<i64> = stmt
            .query_map(params_from_iter(params), |row| row.get(0))?
            .collect::<Result<_, _>>()?;

        load_tracks_by_ids(&self.connection, &ids)
    }

    /// Number of included tracks matching `query`.
    pub fn query_track_count(&self, query: &TrackQuery) -> Result<u32, DbError> {
        let (sql, params) = build_query_sql(query, true);
        let count: u32 = self
            .connection
            .query_row(&sql, params_from_iter(params), |row| row.get(0))?;
        Ok(count)
    }

    pub fn set_track_favourite(&mut self, track_id: i64, favourite: bool) -> Result<bool, DbError> {
        let changed = self.connection.execute(
            "UPDATE track SET favourite = ?2 WHERE id = ?1",
            params![track_id, i64::from(favourite)],
        )?;
        Ok(changed > 0)
    }

    /// Distinct labels and BPM/year bounds across included tracks, for populating filter controls.
    pub fn library_facets(&self) -> Result<LibraryFacets, DbError> {
        let mut stmt = self.connection.prepare(
            "SELECT DISTINCT tag_label
             FROM asset
             WHERE included = 1 AND tag_label IS NOT NULL
             ORDER BY tag_label ASC",
        )?;
        let labels = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        let (bpm_min, bpm_max): (Option<f64>, Option<f64>) = self.connection.query_row(
            "SELECT MIN(tag_bpm), MAX(tag_bpm)
             FROM asset
             WHERE included = 1 AND tag_bpm IS NOT NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let (year_min, year_max): (Option<i32>, Option<i32>) = self.connection.query_row(
            "SELECT MIN(tag_year), MAX(tag_year)
             FROM asset
             WHERE included = 1 AND tag_year IS NOT NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let missing_count: u32 = self.connection.query_row(
            "SELECT count(*) FROM asset WHERE included = 1 AND missing_since IS NOT NULL",
            [],
            |row| row.get(0),
        )?;

        Ok(LibraryFacets {
            labels,
            bpm_min,
            bpm_max,
            year_min,
            year_max,
            missing_count,
        })
    }

    /// Query plan steps (`EXPLAIN QUERY PLAN`) for tests verifying index usage.
    pub fn explain_track_query(
        &self,
        query: &TrackQuery,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<String>, DbError> {
        let (sql, mut params) = build_query_sql(query, false);
        let limit_bind = bind_param(&mut params, Value::Integer(i64::from(limit)));
        let offset_bind = bind_param(&mut params, Value::Integer(i64::from(offset)));
        let sql = format!("EXPLAIN QUERY PLAN {sql} LIMIT {limit_bind} OFFSET {offset_bind}");

        let mut stmt = self.connection.prepare(&sql)?;
        let steps = stmt
            .query_map(params_from_iter(params), |row| row.get(3))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(steps)
    }
}

fn bind_param(params: &mut Vec<Value>, value: Value) -> String {
    params.push(value);
    format!("?{}", params.len())
}

fn build_query_sql(query: &TrackQuery, count_only: bool) -> (String, Vec<Value>) {
    let mut params = Vec::new();
    let fts_match = build_fts_match_query(query.text.as_deref().unwrap_or(""));
    let mut conditions = vec!["asset.included = 1".to_owned()];
    let mut needs_track_join = query.sort == TrackSortField::Added
        || matches!(query.pill, FilterPill::Favourites | FilterPill::Recent);
    let mut needs_source_join = !count_only
        && query.sort == TrackSortField::Index
        && query.pill != FilterPill::Recent
        && query.collection_id.is_none();

    if let Some(fts) = fts_match.as_ref() {
        let slot = bind_param(&mut params, Value::Text(fts.clone()));
        conditions.push(format!("asset_fts MATCH {slot}"));
    }

    if let Some(collection_id) = query.collection_id {
        let slot = bind_param(&mut params, Value::Integer(collection_id));
        conditions.push(format!("collection_item.collection_id = {slot}"));
    }

    match query.pill {
        FilterPill::All => {}
        FilterPill::Favourites => {
            needs_track_join = true;
            conditions.push("track.favourite = 1".to_owned());
        }
        FilterPill::Recent => {
            needs_track_join = true;
            let cutoff = (now_unix_ns() - RECENT_WINDOW_NS).max(1);
            let slot = bind_param(&mut params, Value::Integer(cutoff));
            conditions.push(format!("track.added_ns >= {slot}"));
        }
    }

    if query.missing_only {
        conditions.push("asset.missing_since IS NOT NULL".to_owned());
    }

    if let Some(bpm_min) = query.bpm_min.filter(|value| value.is_finite()) {
        let slot = bind_param(&mut params, Value::Real(bpm_min));
        conditions.push(format!("asset.tag_bpm >= {slot}"));
    }
    if let Some(bpm_max) = query.bpm_max.filter(|value| value.is_finite()) {
        let slot = bind_param(&mut params, Value::Real(bpm_max));
        conditions.push(format!("asset.tag_bpm <= {slot}"));
    }
    if let Some(year_min) = query.year_min {
        let slot = bind_param(&mut params, Value::Integer(i64::from(year_min)));
        conditions.push(format!("asset.tag_year >= {slot}"));
    }
    if let Some(year_max) = query.year_max {
        let slot = bind_param(&mut params, Value::Integer(i64::from(year_max)));
        conditions.push(format!("asset.tag_year <= {slot}"));
    }
    if let Some(label) = query
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let slot = bind_param(&mut params, Value::Text(label.to_owned()));
        conditions.push(format!("asset.tag_label = {slot}"));
    }

    let expanded_keys = key_filter_values(&query.keys);
    let has_keys = !expanded_keys.is_empty();
    if has_keys {
        let placeholders: Vec<String> = expanded_keys
            .into_iter()
            .map(|value| bind_param(&mut params, Value::Text(value)))
            .collect();
        conditions.push(format!("asset.tag_key IN ({})", placeholders.join(", ")));
    }

    if !query.formats.is_empty() {
        let placeholders: Vec<String> = query
            .formats
            .iter()
            .map(|format| bind_param(&mut params, Value::Text(format.name().to_owned())))
            .collect();
        conditions.push(format!("asset.format IN ({})", placeholders.join(", ")));
    }

    // Choose join order to keep SQLite walking the matching index without a temp sort.
    let has_selective_filter = query.pill == FilterPill::Favourites
        || query.collection_id.is_some()
        || query.missing_only
        || query.bpm_min.is_some()
        || query.bpm_max.is_some()
        || query.year_min.is_some()
        || query.year_max.is_some()
        || query
            .label
            .as_deref()
            .is_some_and(|label| !label.trim().is_empty())
        || has_keys
        || !query.formats.is_empty();

    let from = if fts_match.is_some() {
        let mut from =
            String::from("FROM asset_fts CROSS JOIN asset ON asset.id = asset_fts.rowid");
        if query.collection_id.is_some() {
            from.push_str(" JOIN collection_item ON collection_item.track_id = asset.track_id");
        }
        if needs_source_join {
            from.push_str(" JOIN source ON source.id = asset.source_id");
        }
        if needs_track_join {
            from.push_str(" JOIN track ON track.id = asset.track_id");
        }
        from
    } else if query.collection_id.is_some() {
        let mut from = String::from(
            "FROM collection_item CROSS JOIN asset ON asset.track_id = collection_item.track_id",
        );
        if needs_source_join {
            from.push_str(" JOIN source ON source.id = asset.source_id");
        }
        if needs_track_join {
            from.push_str(" JOIN track ON track.id = asset.track_id");
        }
        from
    } else if query.pill == FilterPill::Favourites {
        let mut from = String::from("FROM track CROSS JOIN asset ON asset.track_id = track.id");
        if needs_source_join {
            from.push_str(" JOIN source ON source.id = asset.source_id");
        }
        from
    } else if !count_only
        && !has_selective_filter
        && query.sort == TrackSortField::Index
        && query.pill != FilterPill::Recent
    {
        needs_source_join = false;
        let mut from = String::from("FROM source CROSS JOIN asset ON asset.source_id = source.id");
        if needs_track_join {
            from.push_str(" JOIN track ON track.id = asset.track_id");
        }
        from
    } else if !count_only
        && !has_selective_filter
        && (query.sort == TrackSortField::Added
            || (query.pill == FilterPill::Recent && query.sort == TrackSortField::Index))
    {
        let mut from = String::from("FROM track CROSS JOIN asset ON asset.track_id = track.id");
        if needs_source_join {
            from.push_str(" JOIN source ON source.id = asset.source_id");
        }
        from
    } else {
        let mut from = String::from("FROM asset");
        if needs_source_join {
            from.push_str(" CROSS JOIN source ON source.id = asset.source_id");
        }
        if needs_track_join {
            from.push_str(" JOIN track ON track.id = asset.track_id");
        }
        from
    };
    let _ = needs_source_join;

    let where_clause = conditions.join(" AND ");
    if count_only {
        return (
            format!("SELECT count(*) {from} WHERE {where_clause}"),
            params,
        );
    }

    let dir = match query.direction {
        SortDirection::Asc => "ASC",
        SortDirection::Desc => "DESC",
    };

    let order_by = match query.sort {
        TrackSortField::Index if query.collection_id.is_some() => {
            format!("collection_item.position {dir}, collection_item.track_id {dir}")
        }
        TrackSortField::Index if query.pill == FilterPill::Recent => match query.direction {
            SortDirection::Asc => "track.added_ns DESC, track.id DESC, asset.id DESC",
            SortDirection::Desc => "track.added_ns ASC, track.id ASC, asset.id ASC",
        }
        .to_owned(),
        TrackSortField::Index => format!("source.folder {dir}, asset.path {dir}"),
        TrackSortField::Title => format!("asset.sort_title {dir}, asset.id {dir}"),
        TrackSortField::Artist => {
            format!("(asset.sort_artist IS NULL) ASC, asset.sort_artist {dir}, asset.id {dir}")
        }
        TrackSortField::Bpm => {
            format!("(asset.tag_bpm IS NULL) ASC, asset.tag_bpm {dir}, asset.id {dir}")
        }
        TrackSortField::Key => {
            format!("(asset.sort_key IS NULL) ASC, asset.sort_key {dir}, asset.id {dir}")
        }
        TrackSortField::Duration => {
            format!("(asset.duration_ms IS NULL) ASC, asset.duration_ms {dir}, asset.id {dir}")
        }
        TrackSortField::Added => format!("track.added_ns {dir}, track.id {dir}, asset.id {dir}"),
    };

    (
        format!("SELECT asset.id {from} WHERE {where_clause} ORDER BY {order_by}"),
        params,
    )
}

pub(crate) fn load_tracks_by_ids(
    connection: &Connection,
    ids: &[i64],
) -> Result<Vec<TrackRow>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders: Vec<String> = (1..=ids.len()).map(|index| format!("?{index}")).collect();
    let in_list = placeholders.join(", ");

    let track_sql = format!(
        "SELECT asset.id, asset.track_id, source.folder, asset.path, asset.size_bytes,
                asset.modified_ns, asset.format, asset.duration_ms, asset.sample_rate,
                asset.channels, asset.bit_depth, asset.bitrate_kbps, track.favourite,
                asset.missing_since
         FROM asset
         JOIN source ON source.id = asset.source_id
         JOIN track ON track.id = asset.track_id
         WHERE asset.id IN ({in_list})"
    );
    let mut track_stmt = connection.prepare(&track_sql)?;
    let mut by_id: HashMap<i64, TrackRow> = HashMap::with_capacity(ids.len());
    let mut rows = track_stmt.query(params_from_iter(ids.iter().copied()))?;
    while let Some(row) = rows.next()? {
        let (asset_id, track) = track_row(row)?;
        by_id.insert(asset_id, track);
    }

    let tag_sql = format!(
        "SELECT asset_id, field, value
         FROM file_tag
         WHERE asset_id IN ({in_list})
         ORDER BY field"
    );
    let mut tag_stmt = connection.prepare(&tag_sql)?;
    let mut tag_rows = tag_stmt.query(params_from_iter(ids.iter().copied()))?;
    while let Some(row) = tag_rows.next()? {
        let asset_id: i64 = row.get(0)?;
        if let Some(track) = by_id.get_mut(&asset_id) {
            track.file.tags.push((tag_field(row.get(1)?)?, row.get(2)?));
        }
    }

    let mut ordered = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(mut track) = by_id.remove(id) {
            track.file.tags.sort();
            ordered.push(track);
        }
    }
    crate::collections::populate_track_collections(connection, &mut ordered)?;
    crate::tags::populate_user_tags(connection, &mut ordered)?;
    Ok(ordered)
}
