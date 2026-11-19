use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use resonance_core::columns::{generic_tag_fields, tag_sort_value};
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
    pub catno: Option<&'a str>,
    pub composer: Option<&'a str>,
    pub remixer: Option<&'a str>,
    pub mix: Option<&'a str>,
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
            catno: trimmed_tag(TagField::CatNo),
            composer: trimmed_tag(TagField::Composer),
            remixer: trimmed_tag(TagField::Remixer),
            mix: trimmed_tag(TagField::Mix),
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
    write_search_columns(tx, asset_id, path, &meta, delete_existing_fts)?;
    write_table_columns(tx, asset_id, &meta, tags)
}

/// The sort, filter and search columns schema v4 added.
fn write_search_columns(
    tx: &Transaction<'_>,
    asset_id: i64,
    path: &str,
    meta: &AssetSearchMeta<'_>,
    delete_existing_fts: bool,
) -> Result<(), DbError> {
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

/// The table columns schema v15 added, and the asset's tag value index rows.
fn write_table_columns(
    tx: &Transaction<'_>,
    asset_id: i64,
    meta: &AssetSearchMeta<'_>,
    tags: &[(TagField, String)],
) -> Result<(), DbError> {
    tx.prepare_cached(
        "UPDATE asset SET tag_catno = ?2, tag_comment = ?3, tag_composer = ?4, tag_remixer = ?5,
             tag_mix = ?6
         WHERE id = ?1",
    )?
    .execute(params![
        asset_id,
        meta.catno,
        meta.comment,
        meta.composer,
        meta.remixer,
        meta.mix,
    ])?;
    write_tag_values(tx, asset_id, tags)
}

/// Replaces the asset's rows in the tag value index with the tags that have no column of their own.
pub(crate) fn write_tag_values(
    tx: &Transaction<'_>,
    asset_id: i64,
    tags: &[(TagField, String)],
) -> Result<(), DbError> {
    tx.prepare_cached("DELETE FROM asset_tag_value WHERE asset_id = ?1")?
        .execute([asset_id])?;
    let mut add = tx.prepare_cached(
        "INSERT INTO asset_tag_value (asset_id, field, sort_value) VALUES (?1, ?2, ?3)",
    )?;
    for field in generic_tag_fields() {
        let value = tags
            .iter()
            .find(|(tag, _)| *tag == field)
            .and_then(|(_, value)| tag_sort_value(value));
        if let Some(value) = value {
            add.execute(params![asset_id, field.name(), value])?;
        }
    }
    Ok(())
}

/// Fills the columns and tag value index that v15 adds from what each track shows.
pub(crate) fn backfill_v15_columns(tx: &Transaction<'_>) -> Result<(), DbError> {
    let mut tags_by_asset: HashMap<i64, Vec<(TagField, String)>> = HashMap::new();
    {
        let mut tags_stmt = tx.prepare("SELECT asset_id, field, value FROM file_tag")?;
        let mut tag_rows = tags_stmt.query([])?;
        while let Some(row) = tag_rows.next()? {
            tags_by_asset
                .entry(row.get(0)?)
                .or_default()
                .push((tag_field(row.get(1)?)?, row.get(2)?));
        }
    }
    let assets: Vec<(i64, String)> = tx
        .prepare("SELECT id, path FROM asset ORDER BY id")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (asset_id, path) in assets {
        let file = tags_by_asset.remove(&asset_id).unwrap_or_default();
        let user = crate::tags::user_tags_of_asset(tx, asset_id)?;
        let shown = if user.is_empty() {
            file
        } else {
            effective_tags(&file, &user)
        };
        let meta = AssetSearchMeta::from_path_and_tags(&path, &shown);
        write_table_columns(tx, asset_id, &meta, &shown)?;
    }
    Ok(())
}

/// Refines v1â€“v3 migrated rows with Rust's exact title, BPM, key and year normalisation so
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
        let meta = AssetSearchMeta::from_path_and_tags(&path, &tags);
        write_search_columns(tx, asset_id, &path, &meta, true)?;
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
        let ids = if let Some(sort) = TagSort::of(query) {
            sort.ids(&self.connection, offset, Some(limit))?
        } else {
            let (sql, mut params) = build_query_sql(query, false);
            let limit_bind = bind_param(&mut params, Value::Integer(i64::from(limit)));
            let offset_bind = bind_param(&mut params, Value::Integer(i64::from(offset)));
            let sql = format!("{sql} LIMIT {limit_bind} OFFSET {offset_bind}");
            let mut stmt = self.connection.prepare(&sql)?;
            stmt.query_map(params_from_iter(params), |row| row.get(0))?
                .collect::<Result<_, _>>()?
        };
        load_tracks_by_ids(&self.connection, &ids)
    }

    /// Every included file matching `query`, in its order.
    pub fn query_asset_ids(&self, query: &TrackQuery) -> Result<Vec<i64>, DbError> {
        if let Some(sort) = TagSort::of(query) {
            return sort.ids(&self.connection, 0, None);
        }
        let (sql, params) = build_query_sql(query, false);
        let mut stmt = self.connection.prepare(&sql)?;
        let ids = stmt
            .query_map(params_from_iter(params), |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
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
        if let Some(sort) = TagSort::of(query) {
            return sort.explain(&self.connection);
        }
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
    let sorts = sort_keys(query);
    let sorts_by = |field: TrackSortField| sorts.iter().any(|(sort, _)| *sort == field);
    let mut needs_track_join = (!count_only && sorts_by(TrackSortField::Added))
        || matches!(query.pill, FilterPill::Favourites | FilterPill::Recent);
    let needs_source_join = !count_only
        && ((query.sort == TrackSortField::Index
            && query.pill != FilterPill::Recent
            && query.collection_id.is_none())
            || sorts_by(TrackSortField::Path));

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
        && matches!(query.sort, TrackSortField::Index | TrackSortField::Path)
        && query.pill != FilterPill::Recent
    {
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

    let where_clause = conditions.join(" AND ");
    if count_only {
        return (
            format!("SELECT count(*) {from} WHERE {where_clause}"),
            params,
        );
    }

    let mut order = Vec::new();
    for (index, (field, direction)) in sorts.iter().enumerate() {
        order.extend(sort_terms(
            query,
            *field,
            *direction,
            index == 0,
            &mut params,
        ));
    }
    // Library and collection order are unique already. Recent order runs newest first.
    let (last, last_direction) = sorts.last().copied().unwrap_or_default();
    match last {
        TrackSortField::Index if query.collection_id.is_some() => {}
        TrackSortField::Index if query.pill == FilterPill::Recent => {
            let reversed = match last_direction {
                SortDirection::Asc => SortDirection::Desc,
                SortDirection::Desc => SortDirection::Asc,
            };
            order.push(format!("asset.id {}", sql_direction(reversed)));
        }
        TrackSortField::Index | TrackSortField::Path => {}
        _ => order.push(format!("asset.id {}", sql_direction(last_direction))),
    }
    let order_by = order.join(", ");

    (
        format!("SELECT asset.id {from} WHERE {where_clause} ORDER BY {order_by}"),
        params,
    )
}

/// The query's sort keys in order, without a second key that cannot change the order.
fn sort_keys(query: &TrackQuery) -> Vec<(TrackSortField, SortDirection)> {
    let mut sorts = vec![(query.sort, query.direction)];
    let unique = matches!(query.sort, TrackSortField::Index | TrackSortField::Path);
    if let Some(then) = query.then_sort
        && !unique
        && then.0 != query.sort
    {
        sorts.push(then);
    }
    sorts
}

fn sql_direction(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Asc => "ASC",
        SortDirection::Desc => "DESC",
    }
}

/// The `ORDER BY` terms for one sort key, without the final tie-break on the asset. A column that
/// can be empty puts its empty values last in both directions, which its two indexes match.
fn sort_terms(
    query: &TrackQuery,
    field: TrackSortField,
    direction: SortDirection,
    primary: bool,
    params: &mut Vec<Value>,
) -> Vec<String> {
    let dir = sql_direction(direction);
    let nullable =
        |column: &str| vec![format!("({column} IS NULL) ASC"), format!("{column} {dir}")];
    let column = |column: &str| vec![format!("{column} {dir}")];
    match field {
        TrackSortField::Index if primary && query.collection_id.is_some() => vec![
            format!("collection_item.position {dir}"),
            format!("collection_item.track_id {dir}"),
        ],
        TrackSortField::Index if primary && query.pill == FilterPill::Recent => {
            let reversed = match direction {
                SortDirection::Asc => "DESC",
                SortDirection::Desc => "ASC",
            };
            vec![
                format!("track.added_ns {reversed}"),
                format!("track.id {reversed}"),
            ]
        }
        TrackSortField::Index | TrackSortField::Path => {
            vec![format!("source.folder {dir}"), format!("asset.path {dir}")]
        }
        TrackSortField::Title => column("asset.sort_title"),
        TrackSortField::Artist => nullable("asset.sort_artist"),
        TrackSortField::Bpm => nullable("asset.tag_bpm"),
        TrackSortField::Key => nullable("asset.sort_key"),
        TrackSortField::Duration => nullable("asset.duration_ms"),
        TrackSortField::Added => vec![format!("track.added_ns {dir}"), format!("track.id {dir}")],
        TrackSortField::Album => nullable("asset.tag_album"),
        TrackSortField::Genre => nullable("asset.tag_genre"),
        TrackSortField::Label => nullable("asset.tag_label"),
        TrackSortField::CatNo => nullable("asset.tag_catno"),
        TrackSortField::Year => nullable("asset.tag_year"),
        TrackSortField::Comment => nullable("asset.tag_comment"),
        TrackSortField::Composer => nullable("asset.tag_composer"),
        TrackSortField::Remixer => nullable("asset.tag_remixer"),
        TrackSortField::Mix => nullable("asset.tag_mix"),
        TrackSortField::Format => column("asset.format"),
        TrackSortField::Bitrate => nullable("asset.bitrate_kbps"),
        TrackSortField::SampleRate => nullable("asset.sample_rate"),
        TrackSortField::BitDepth => nullable("asset.bit_depth"),
        TrackSortField::Channels => nullable("asset.channels"),
        TrackSortField::FileSize => column("asset.size_bytes"),
        TrackSortField::Modified => column("asset.modified_ns"),
        TrackSortField::Tag(tag) => {
            let slot = bind_param(params, Value::Text(tag.name().to_owned()));
            nullable(&format!(
                "(SELECT sort_value FROM asset_tag_value \
                 WHERE asset_tag_value.asset_id = asset.id AND asset_tag_value.field = {slot})"
            ))
        }
    }
}

/// The whole library sorted by a generic tag on its own: the tracks that have the tag in the
/// order of the tag value index, then the ones that do not in asset order. Each half is one walk
/// along an index. SQLite sorted every row when the halves were one `UNION ALL`, so they are read
/// one after the other here.
struct TagSort {
    field: TagField,
    direction: SortDirection,
}

impl TagSort {
    fn of(query: &TrackQuery) -> Option<Self> {
        let TrackSortField::Tag(field) = query.sort else {
            return None;
        };
        let whole_library = query.pill == FilterPill::All
            && query.collection_id.is_none()
            && query.then_sort.is_none()
            && build_fts_match_query(query.text.as_deref().unwrap_or("")).is_none()
            && query.active_filter_count() == 0;
        whole_library.then_some(Self {
            field,
            direction: query.direction,
        })
    }

    fn tagged_sql(&self) -> String {
        let dir = sql_direction(self.direction);
        format!(
            "SELECT asset_tag_value.asset_id
             FROM asset_tag_value CROSS JOIN asset ON asset.id = asset_tag_value.asset_id
             WHERE asset_tag_value.field = ?1 AND asset.included = 1
             ORDER BY asset_tag_value.sort_value {dir}, asset_tag_value.asset_id {dir}
             LIMIT ?2 OFFSET ?3"
        )
    }

    // `NOT INDEXED` keeps SQLite on the table's own order, which is the order wanted.
    fn untagged_sql(&self) -> String {
        let dir = sql_direction(self.direction);
        format!(
            "SELECT asset.id FROM asset NOT INDEXED
             WHERE asset.included = 1 AND NOT EXISTS (
                 SELECT 1 FROM asset_tag_value
                 WHERE asset_tag_value.asset_id = asset.id AND asset_tag_value.field = ?1
             )
             ORDER BY asset.id {dir}
             LIMIT ?2 OFFSET ?3"
        )
    }

    fn ids(
        &self,
        connection: &Connection,
        offset: u32,
        limit: Option<u32>,
    ) -> Result<Vec<i64>, DbError> {
        let field = self.field.name();
        let tagged: u32 = connection
            .prepare_cached(
                "SELECT count(*)
                 FROM asset_tag_value CROSS JOIN asset ON asset.id = asset_tag_value.asset_id
                 WHERE asset_tag_value.field = ?1 AND asset.included = 1",
            )?
            .query_row([field], |row| row.get(0))?;
        let wanted = limit.map_or(-1, i64::from);
        let mut ids: Vec<i64> = Vec::new();
        if offset < tagged {
            ids = connection
                .prepare_cached(&self.tagged_sql())?
                .query_map(params![field, wanted, offset], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
        }
        let still_wanted = limit.map(|limit| i64::from(limit) - ids.len() as i64);
        if still_wanted.is_none_or(|left| left > 0) {
            let skip = offset.saturating_sub(tagged);
            let untagged: Vec<i64> = connection
                .prepare_cached(&self.untagged_sql())?
                .query_map(params![field, still_wanted.unwrap_or(-1), skip], |row| {
                    row.get(0)
                })?
                .collect::<Result<_, _>>()?;
            ids.extend(untagged);
        }
        Ok(ids)
    }

    fn explain(&self, connection: &Connection) -> Result<Vec<String>, DbError> {
        let mut steps = Vec::new();
        for sql in [self.tagged_sql(), self.untagged_sql()] {
            let mut statement = connection.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
            let plan = statement
                .query_map(params![self.field.name(), 200, 0], |row| row.get(3))?
                .collect::<Result<Vec<String>, _>>()?;
            steps.extend(plan);
        }
        Ok(steps)
    }
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
                asset.missing_since, asset.artwork_hash, track.added_ns
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
    crate::artwork::populate_user_artwork(connection, &mut ordered)?;
    Ok(ordered)
}
