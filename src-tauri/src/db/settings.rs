//! App settings: one typed document stored as JSON under the `app` key, plus a
//! generic key/value API for anything else. `#[serde(default)]` means a field
//! added in a later version simply takes its default on old installs.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Database, DbError, now_millis};

const APP_KEY: &str = "app";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// Mirrored by `AppSettings` in `src/lib/ipc.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// `None` = follow the OS language (see `i18n::resolve`).
    pub locale: Option<String>,
    pub theme: Theme,
    /// Telemetry is opt-in; nothing leaves the device until this is true.
    pub telemetry_enabled: bool,
    pub autostart: bool,
    /// Anonymous, random, per-install id attached to telemetry. Not derived
    /// from hardware or account data; "erase all data" rotates it.
    pub device_id: String,
    pub onboarding_done: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            locale: None,
            theme: Theme::System,
            telemetry_enabled: crate::config::get().telemetry.default_enabled,
            autostart: false,
            device_id: uuid::Uuid::new_v4().to_string(),
            onboarding_done: false,
        }
    }
}

/// Load the typed settings, creating the row (with a fresh device id) on first
/// run. A corrupt document falls back to defaults rather than failing startup.
pub fn load(db: &Database) -> Result<AppSettings, DbError> {
    match get_raw(db, APP_KEY)? {
        Some(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
        None => {
            let s = AppSettings::default();
            save(db, &s)?;
            Ok(s)
        }
    }
}

pub fn save(db: &Database, s: &AppSettings) -> Result<(), DbError> {
    set_raw(db, APP_KEY, &serde_json::to_string(s)?)
}

pub fn get_raw(db: &Database, key: &str) -> Result<Option<String>, DbError> {
    db.with(|c| {
        c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()
    })
}

pub fn set_raw(db: &Database, key: &str, value: &str) -> Result<(), DbError> {
    db.with(|c| {
        c.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_millis()],
        )
    })?;
    Ok(())
}

pub fn delete(db: &Database, key: &str) -> Result<bool, DbError> {
    Ok(db.with(|c| c.execute("DELETE FROM settings WHERE key = ?1", [key]))? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_load_creates_defaults_with_a_stable_device_id() {
        let db = Database::open_in_memory().unwrap();
        let a = load(&db).unwrap();
        let b = load(&db).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.device_id.len(), 36);
        assert!(!a.telemetry_enabled, "telemetry must be opt-in");
    }

    #[test]
    fn save_roundtrips_and_unknown_fields_are_tolerated() {
        let db = Database::open_in_memory().unwrap();
        let mut s = load(&db).unwrap();
        s.theme = Theme::Dark;
        s.locale = Some("es".into());
        save(&db, &s).unwrap();
        assert_eq!(load(&db).unwrap(), s);

        // A newer app wrote a field we don't know: still loads.
        set_raw(&db, APP_KEY, r#"{"theme":"light","future_field":1}"#).unwrap();
        assert_eq!(load(&db).unwrap().theme, Theme::Light);
    }

    #[test]
    fn raw_kv_upserts() {
        let db = Database::open_in_memory().unwrap();
        set_raw(&db, "k", "1").unwrap();
        set_raw(&db, "k", "2").unwrap();
        assert_eq!(get_raw(&db, "k").unwrap().as_deref(), Some("2"));
        assert!(delete(&db, "k").unwrap());
        assert!(get_raw(&db, "k").unwrap().is_none());
    }
}
