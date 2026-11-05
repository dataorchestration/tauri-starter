//! Startup / on-demand health check: can the app actually do its job? The UI
//! shows this on the Hello screen and drives onboarding from
//! `missing_required`.

use serde::Serialize;
use tauri::AppHandle;

use crate::AppState;
use crate::permissions::{self, Permission, PermissionReport};

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub ok: bool,
    pub detail: String,
}

impl Check {
    fn ok(detail: impl Into<String>) -> Self {
        Self {
            ok: true,
            detail: detail.into(),
        }
    }
    fn fail(detail: impl Into<String>) -> Self {
        Self {
            ok: false,
            detail: detail.into(),
        }
    }
}

/// Mirrored by `HealthReport` in `src/lib/ipc.ts`.
#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    /// Everything required is in place (db + keychain + required permissions).
    pub ok: bool,
    pub db: Check,
    pub keychain: Check,
    pub permissions: Vec<PermissionReport>,
    pub missing_required: Vec<Permission>,
    pub updater_configured: bool,
    pub telemetry_enabled: bool,
    pub telemetry_sinks: Vec<&'static str>,
    pub pending_crashes: usize,
    pub schema_version: i64,
}

/// True when the updater feature is on AND the config carries a real public key.
pub fn updater_configured(app: &AppHandle) -> bool {
    if !crate::config::get().features.updater {
        return false;
    }
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.is_empty() && !k.starts_with("REPLACE_WITH"))
}

pub async fn run(app: &AppHandle, state: &AppState) -> HealthReport {
    let (db, schema_version) = {
        let guard = state.db.read().await;
        let version = match guard.as_ref() {
            Some(db) => db.schema_version().await,
            None => Err(crate::db::DbError::Validation(
                "database unavailable".into(),
            )),
        };
        match version {
            Ok(v) => {
                let encrypted = crate::db::file_is_encrypted(&state.paths.db_file());
                if encrypted {
                    (Check::ok(format!("schema v{v}, encrypted")), v)
                } else {
                    (Check::fail("database file is not encrypted"), v)
                }
            }
            Err(e) => (Check::fail(e.to_string()), 0),
        }
    };
    let keychain = if crate::db::key::exists(&state.meta.identifier) {
        Check::ok("key present")
    } else {
        Check::fail("database key missing from keychain")
    };
    let perms = permissions::report(app);
    let missing_required = permissions::missing_required(&perms);
    HealthReport {
        ok: db.ok && keychain.ok && missing_required.is_empty(),
        db,
        keychain,
        permissions: perms,
        missing_required,
        updater_configured: updater_configured(app),
        telemetry_enabled: state.telemetry.is_enabled(),
        telemetry_sinks: state.telemetry.sink_names().to_vec(),
        pending_crashes: crate::crash::pending(&state.paths.crashes).len(),
        schema_version,
    }
}
