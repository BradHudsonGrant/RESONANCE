use resonance_core::preview::{PreviewSettings, PreviewStart};
use rusqlite::params;

use crate::{Database, DbError};

const AUTO_PREVIEW: &str = "auto_preview";
const START: &str = "preview_start";
const START_SECONDS: &str = "preview_start_seconds";

const BEGINNING: i64 = 0;
const THIRTY_PERCENT: i64 = 1;
const SECONDS: i64 = 2;

impl Database {
    /// How previews behave, as the user last left them, or the defaults.
    pub fn preview_settings(&self) -> Result<PreviewSettings, DbError> {
        let mut statement = self
            .connection
            .prepare("SELECT name, value FROM table_setting WHERE name IN (?1, ?2, ?3)")?;
        let mut settings = PreviewSettings::default();
        let mut start_kind = None;
        let mut start_seconds = 0;
        let stored = statement.query_map(params![AUTO_PREVIEW, START, START_SECONDS], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in stored {
            let (name, value) = row?;
            match name.as_str() {
                AUTO_PREVIEW => settings.auto_preview = value != 0,
                START => start_kind = Some(value),
                _ => start_seconds = u32::try_from(value).unwrap_or(0),
            }
        }
        settings.start = match start_kind {
            Some(BEGINNING) => PreviewStart::Beginning,
            Some(SECONDS) => PreviewStart::Seconds(start_seconds).clamped(),
            _ => PreviewStart::ThirtyPercent,
        };
        Ok(settings)
    }

    pub fn save_preview_settings(&mut self, settings: &PreviewSettings) -> Result<(), DbError> {
        let (kind, seconds) = match settings.start.clamped() {
            PreviewStart::Beginning => (BEGINNING, 0),
            PreviewStart::ThirtyPercent => (THIRTY_PERCENT, 0),
            PreviewStart::Seconds(seconds) => (SECONDS, seconds),
        };
        let transaction = self.connection.transaction()?;
        {
            let mut set = transaction.prepare(
                "INSERT INTO table_setting (name, value) VALUES (?1, ?2)
                 ON CONFLICT (name) DO UPDATE SET value = excluded.value",
            )?;
            set.execute(params![AUTO_PREVIEW, i64::from(settings.auto_preview)])?;
            set.execute(params![START, kind])?;
            set.execute(params![START_SECONDS, i64::from(seconds)])?;
        }
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_library_has_the_default_preview_settings() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.preview_settings().unwrap(), PreviewSettings::default());
    }

    #[test]
    fn preview_settings_read_back_as_saved() {
        let mut db = Database::open_in_memory().unwrap();
        for start in [
            PreviewStart::Beginning,
            PreviewStart::ThirtyPercent,
            PreviewStart::Seconds(45),
        ] {
            let settings = PreviewSettings {
                auto_preview: true,
                start,
            };
            db.save_preview_settings(&settings).unwrap();
            assert_eq!(db.preview_settings().unwrap(), settings);
        }
    }

    #[test]
    fn preview_settings_leave_the_table_wrap_alone() {
        let mut db = Database::open_in_memory().unwrap();
        let mut layout = db.table_layout().unwrap();
        layout.wrap = true;
        db.save_table_layout(&layout).unwrap();
        db.save_preview_settings(&PreviewSettings {
            auto_preview: true,
            start: PreviewStart::Beginning,
        })
        .unwrap();
        assert!(db.table_layout().unwrap().wrap);
        db.save_table_layout(&layout).unwrap();
        assert!(db.preview_settings().unwrap().auto_preview);
    }
}
