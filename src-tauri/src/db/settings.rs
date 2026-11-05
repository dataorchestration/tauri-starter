//! App settings: one typed document stored as JSON under the `app` key, plus a
//! generic key/value API for anything else. `#[serde(default)]` means a field
//! added in a later version simply takes its default on old installs.

use serde::{Deserialize, Serialize};
use turso::params;

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
pub async fn load(db: &Database) -> Result<AppSettings, DbError> {
    match get_raw(db, APP_KEY).await? {
        Some(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
        None => {
            let s = AppSettings::default();
            save(db, &s).await?;
            Ok(s)
        }
    }
}

pub async fn save(db: &Database, s: &AppSettings) -> Result<(), DbError> {
    set_raw(db, APP_KEY, &serde_json::to_string(s)?).await
}

pub async fn get_raw(db: &Database, key: &str) -> Result<Option<String>, DbError> {
    let conn = db.connection().await;
    let mut rows = conn
        .query("SELECT value FROM settings WHERE key = ?1", [key])
        .await?;
    Ok(rows.next().await?.map(|r| r.get(0)).transpose()?)
}

pub async fn set_raw(db: &Database, key: &str, value: &str) -> Result<(), DbError> {
    db.connection()
        .await
        .execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_millis()],
        )
        .await?;
    Ok(())
}

pub async fn delete(db: &Database, key: &str) -> Result<bool, DbError> {
    Ok(db
        .connection()
        .await
        .execute("DELETE FROM settings WHERE key = ?1", [key])
        .await?
        > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn first_load_creates_defaults_with_a_stable_device_id() {
        let db = Database::open_in_memory().await.unwrap();
        let a = load(&db).await.unwrap();
        let b = load(&db).await.unwrap();
        assert_eq!(a, b);
        assert_eq!(a.device_id.len(), 36);
        assert!(!a.telemetry_enabled, "telemetry must be opt-in");
    }

    #[tokio::test]
    async fn save_roundtrips_and_unknown_fields_are_tolerated() {
        let db = Database::open_in_memory().await.unwrap();
        let mut s = load(&db).await.unwrap();
        s.theme = Theme::Dark;
        s.locale = Some("es".into());
        save(&db, &s).await.unwrap();
        assert_eq!(load(&db).await.unwrap(), s);

        // A newer app wrote a field we don't know: still loads.
        set_raw(&db, APP_KEY, r#"{"theme":"light","future_field":1}"#)
            .await
            .unwrap();
        assert_eq!(load(&db).await.unwrap().theme, Theme::Light);
    }

    #[tokio::test]
    async fn raw_kv_upserts() {
        let db = Database::open_in_memory().await.unwrap();
        set_raw(&db, "k", "1").await.unwrap();
        set_raw(&db, "k", "2").await.unwrap();
        assert_eq!(get_raw(&db, "k").await.unwrap().as_deref(), Some("2"));
        assert!(delete(&db, "k").await.unwrap());
        assert!(get_raw(&db, "k").await.unwrap().is_none());
    }
}
