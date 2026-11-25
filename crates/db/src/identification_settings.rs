use resonance_core::providers::IdentificationSettings;
use rusqlite::{OptionalExtension, params};

use crate::{Database, DbError};

const VERIFY_AUDIO_PREVIEWS: &str = "verify_audio_previews";

impl Database {
    /// How identification behaves, as the user last configured it, or the defaults.
    pub fn identification_settings(&self) -> Result<IdentificationSettings, DbError> {
        let value: Option<i64> = self
            .connection
            .query_row(
                "SELECT value FROM table_setting WHERE name = ?1",
                [VERIFY_AUDIO_PREVIEWS],
                |row| row.get(0),
            )
            .optional()?;
        Ok(IdentificationSettings {
            verify_audio_previews: value.is_none_or(|raw| raw != 0),
        })
    }

    pub fn save_identification_settings(
        &mut self,
        settings: &IdentificationSettings,
    ) -> Result<(), DbError> {
        self.connection.execute(
            "INSERT INTO table_setting (name, value) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET value = excluded.value",
            params![
                VERIFY_AUDIO_PREVIEWS,
                i64::from(settings.verify_audio_previews)
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_library_enables_preview_verification_by_default() {
        let mut db = Database::open_in_memory().unwrap();
        assert_eq!(
            db.identification_settings().unwrap(),
            IdentificationSettings {
                verify_audio_previews: true
            }
        );

        let updated = IdentificationSettings {
            verify_audio_previews: false,
        };
        db.save_identification_settings(&updated).unwrap();
        assert_eq!(db.identification_settings().unwrap(), updated);
    }
}
