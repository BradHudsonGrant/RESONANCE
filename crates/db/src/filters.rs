use resonance_core::collections::normalise_collection_name;
use resonance_core::filter::{FilterTerm, terms_from_key, terms_key};
use resonance_core::quick_filters::{FilterUse, QuickFilterBar};
use resonance_core::search::FilterPill;
use rusqlite::{OptionalExtension, params};

use crate::{Database, DbError, search};

const RECOMMEND: &str = "recommend_quick_filters";

/// A saved filter set, listed in the sidebar beside the collections.
#[derive(Debug, Clone, PartialEq)]
pub struct SmartView {
    pub id: i64,
    pub name: String,
    pub search: Option<String>,
    pub pill: FilterPill,
    pub terms: Vec<FilterTerm>,
}

fn pill_name(pill: FilterPill) -> Option<&'static str> {
    match pill {
        FilterPill::All => None,
        FilterPill::Favourites => Some("favourites"),
        FilterPill::Recent => Some("recent"),
    }
}

fn pill_from_name(name: Option<&str>) -> FilterPill {
    match name {
        Some("favourites") => FilterPill::Favourites,
        Some("recent") => FilterPill::Recent,
        _ => FilterPill::All,
    }
}

impl Database {
    /// Counts one use of a filter, for recommending quick filters.
    pub fn record_filter_use(&mut self, term: &FilterTerm, now_ms: i64) -> Result<(), DbError> {
        self.connection.execute(
            "INSERT INTO filter_use (term, uses, last_used_ms) VALUES (?1, 1, ?2)
             ON CONFLICT (term) DO UPDATE SET uses = uses + 1, last_used_ms = excluded.last_used_ms",
            params![term.key(), now_ms],
        )?;
        Ok(())
    }

    /// Every counted filter this build can read.
    pub fn filter_uses(&self) -> Result<Vec<FilterUse>, DbError> {
        let mut statement = self
            .connection
            .prepare("SELECT term, uses, last_used_ms FROM filter_use")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(key, uses, last_used_ms)| {
                FilterTerm::from_key(&key).ok().map(|term| FilterUse {
                    term,
                    uses,
                    last_used_ms,
                })
            })
            .collect())
    }

    pub fn quick_filter_bar(&self) -> Result<QuickFilterBar, DbError> {
        let mut statement = self
            .connection
            .prepare("SELECT term FROM quick_filter ORDER BY position")?;
        let pinned = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|key| FilterTerm::from_key(&key).ok())
            .collect();
        let mut statement = self
            .connection
            .prepare("SELECT term FROM quick_filter_dismissed ORDER BY term")?;
        let dismissed = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let recommend: Option<i64> = self
            .connection
            .query_row(
                "SELECT value FROM table_setting WHERE name = ?1",
                [RECOMMEND],
                |row| row.get(0),
            )
            .optional()?;
        Ok(QuickFilterBar {
            pinned,
            dismissed,
            recommend: recommend.is_none_or(|value| value != 0),
        })
    }

    pub fn save_quick_filter_bar(&mut self, bar: &QuickFilterBar) -> Result<(), DbError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM quick_filter", [])?;
        transaction.execute("DELETE FROM quick_filter_dismissed", [])?;
        {
            let mut pin =
                transaction.prepare("INSERT INTO quick_filter (position, term) VALUES (?1, ?2)")?;
            for (position, term) in bar.pinned.iter().enumerate() {
                pin.execute(params![position as i64, term.key()])?;
            }
            let mut dismiss = transaction
                .prepare("INSERT OR IGNORE INTO quick_filter_dismissed (term) VALUES (?1)")?;
            for key in &bar.dismissed {
                dismiss.execute([key])?;
            }
        }
        transaction.execute(
            "INSERT INTO table_setting (name, value) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET value = excluded.value",
            params![RECOMMEND, i64::from(bar.recommend)],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Every smart view in sidebar order.
    pub fn smart_views(&self) -> Result<Vec<SmartView>, DbError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, search, pill, terms FROM smart_view ORDER BY position, id",
        )?;
        let views = statement
            .query_map([], |row| {
                Ok(SmartView {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    search: row.get(2)?,
                    pill: pill_from_name(row.get::<_, Option<String>>(3)?.as_deref()),
                    terms: terms_from_key(&row.get::<_, String>(4)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(views)
    }

    /// Saves a filter set at the end of the list.
    pub fn create_smart_view(
        &mut self,
        name: &str,
        search: Option<&str>,
        pill: FilterPill,
        terms: &[FilterTerm],
    ) -> Result<SmartView, DbError> {
        let name = normalise_collection_name(name)?;
        let search = search
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned);
        let transaction = self.connection.transaction()?;
        let taken: Option<i64> = transaction
            .query_row(
                "SELECT id FROM smart_view WHERE name = ?1",
                [&name],
                |row| row.get(0),
            )
            .optional()?;
        if taken.is_some() {
            return Err(DbError::DuplicateViewName { name });
        }
        let position: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM smart_view",
            [],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO smart_view (name, position, search, pill, terms, created_ns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &name,
                position,
                search,
                pill_name(pill),
                terms_key(terms),
                search::now_unix_ns()
            ],
        )?;
        let id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(SmartView {
            id,
            name,
            search,
            pill,
            terms: terms.to_vec(),
        })
    }

    /// Renames a smart view. `false` when there is none with `view_id`.
    pub fn rename_smart_view(&mut self, view_id: i64, name: &str) -> Result<bool, DbError> {
        let name = normalise_collection_name(name)?;
        let taken: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM smart_view WHERE name = ?1 AND id <> ?2",
                params![&name, view_id],
                |row| row.get(0),
            )
            .optional()?;
        if taken.is_some() {
            return Err(DbError::DuplicateViewName { name });
        }
        let changed = self.connection.execute(
            "UPDATE smart_view SET name = ?2 WHERE id = ?1",
            params![view_id, &name],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_smart_view(&mut self, view_id: i64) -> Result<bool, DbError> {
        let changed = self
            .connection
            .execute("DELETE FROM smart_view WHERE id = ?1", [view_id])?;
        Ok(changed > 0)
    }
}
