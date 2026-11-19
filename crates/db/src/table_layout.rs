use resonance_core::columns::{
    LibraryColumn, ShownColumn, SortColumn, SortKey, TableLayout, generic_tag_fields,
};
use resonance_core::library::TagField;
use resonance_core::search::SortDirection;
use rusqlite::{OptionalExtension, params};

use crate::{Database, DbError};

const WRAP: &str = "wrap";

impl Database {
    /// The library table as the user last left it, or the default one.
    pub fn table_layout(&self) -> Result<TableLayout, DbError> {
        let mut statement = self
            .connection
            .prepare("SELECT column_id, width_px, chosen FROM table_column ORDER BY position")?;
        let columns = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<u16>>(1)?,
                    row.get::<_, u32>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|(id, width_px, chosen)| {
                LibraryColumn::from_id(&id).map(|column| ShownColumn {
                    column,
                    width_px,
                    chosen,
                })
            })
            .collect();

        let mut statement = self
            .connection
            .prepare("SELECT column_id, direction FROM table_sort ORDER BY position")?;
        let sort = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|(id, direction)| {
                SortColumn::from_id(&id).map(|column| SortKey {
                    column,
                    direction: if direction == "desc" {
                        SortDirection::Desc
                    } else {
                        SortDirection::Asc
                    },
                })
            })
            .collect();

        let wrap: Option<i64> = self
            .connection
            .query_row(
                "SELECT value FROM table_setting WHERE name = ?1",
                [WRAP],
                |row| row.get(0),
            )
            .optional()?;
        Ok(TableLayout::from_stored(
            columns,
            wrap.is_some_and(|value| value != 0),
            sort,
        ))
    }

    pub fn save_table_layout(&mut self, layout: &TableLayout) -> Result<(), DbError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM table_column", [])?;
        transaction.execute("DELETE FROM table_sort", [])?;
        {
            let mut add_column = transaction.prepare(
                "INSERT INTO table_column (position, column_id, width_px, chosen)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (position, shown) in layout.columns.iter().enumerate() {
                add_column.execute(params![
                    position as i64,
                    shown.column.id(),
                    shown.width_px,
                    shown.chosen,
                ])?;
            }
            let mut add_sort = transaction.prepare(
                "INSERT INTO table_sort (position, column_id, direction) VALUES (?1, ?2, ?3)",
            )?;
            for (position, key) in layout.sort.iter().enumerate() {
                let direction = match key.direction {
                    SortDirection::Asc => "asc",
                    SortDirection::Desc => "desc",
                };
                add_sort.execute(params![position as i64, key.column.id(), direction])?;
            }
        }
        transaction.execute(
            "INSERT INTO table_setting (name, value) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET value = excluded.value",
            params![WRAP, i64::from(layout.wrap)],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// The tags without a column of their own that at least one track in the library has.
    pub fn tags_in_library(&self) -> Result<Vec<TagField>, DbError> {
        let mut statement = self
            .connection
            .prepare("SELECT EXISTS (SELECT 1 FROM asset_tag_value WHERE field = ?1)")?;
        let mut present = Vec::new();
        for field in generic_tag_fields() {
            let found: bool = statement.query_row([field.name()], |row| row.get(0))?;
            if found {
                present.push(field);
            }
        }
        Ok(present)
    }
}
