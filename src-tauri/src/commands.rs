//! IPC surface. Every `#[tauri::command]` here is thin: validate, call a module,
//! map errors to [`IpcError`]. The TypeScript mirror is `src/lib/ipc.ts` — keep
//! the two in sync (names are snake_case on both sides).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::notifications::{Notification, NotificationKind};
use crate::db::settings::{AppSettings, Theme};
use crate::db::users::{NewUser, User};
use crate::db::{self, DbError};
use crate::health::HealthReport;
use crate::permissions::{Permission, PermissionReport, PermissionState};
use crate::telemetry::Level;
use crate::{AppState, crash, health, i18n, menu, permissions, updater};

/// Error shape the webview receives. `code` is stable for programmatic
/// handling; `message` is for display/logging.
#[derive(Debug, Clone, Serialize)]
pub struct IpcError {
    pub code: &'static str,
    pub message: String,
}

impl IpcError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<DbError> for IpcError {
    fn from(e: DbError) -> Self {
        let code = match e {
            DbError::NotFound => "not_found",
            DbError::WrongKey => "db_wrong_key",
            DbError::Keychain(_) => "keychain",
            _ => "db",
        };
        Self::new(code, e.to_string())
    }
}

impl From<tauri::Error> for IpcError {
    fn from(e: tauri::Error) -> Self {
        Self::new("tauri", e.to_string())
    }
}

impl From<updater::UpdateError> for IpcError {
    fn from(e: updater::UpdateError) -> Self {
        let code = match e {
            updater::UpdateError::NotConfigured => "updater_not_configured",
            updater::UpdateError::Failed(_) => "updater_failed",
        };
        Self::new(code, e.to_string())
    }
}

type IpcResult<T> = Result<T, IpcError>;

fn with_db<T>(
    state: &AppState,
    f: impl FnOnce(&db::Database) -> Result<T, DbError>,
) -> IpcResult<T> {
    let guard = state.db.read().unwrap_or_else(|p| p.into_inner());
    Ok(f(&guard)?)
}

// ---------------------------------------------------------------- app info

/// Mirrored by `AppInfo` in `src/lib/ipc.ts`.
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub os: &'static str,
    pub arch: &'static str,
    pub locale: String,
    pub supported_locales: Vec<String>,
    pub data_dir: String,
    pub log_dir: String,
    pub debug: bool,
    pub features: crate::config::Features,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> IpcResult<AppInfo> {
    let settings = with_db(&state, db::settings::load)?;
    Ok(AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        identifier: app.config().identifier.clone(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        locale: i18n::resolve(settings.locale.as_deref()),
        supported_locales: i18n::supported(),
        data_dir: crate::paths::display(&state.paths.data),
        log_dir: crate::paths::display(&state.paths.logs),
        debug: cfg!(debug_assertions),
        features: crate::config::get().features.clone(),
    })
}

#[tauri::command]
pub fn health_check(app: AppHandle, state: State<'_, AppState>) -> HealthReport {
    health::run(&app, &state)
}

// ---------------------------------------------------------------- settings

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> IpcResult<AppSettings> {
    with_db(&state, db::settings::load)
}

/// Persist settings and apply the side effects (autostart, telemetry switch,
/// menu language, native window theme). Returns the stored value.
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> IpcResult<AppSettings> {
    let previous = with_db(&state, db::settings::load)?;
    // The device id is server-side state; the webview can't change it.
    let settings = AppSettings {
        device_id: previous.device_id.clone(),
        ..settings
    };
    with_db(&state, |d| db::settings::save(d, &settings))?;

    state.telemetry.set_enabled(settings.telemetry_enabled);
    if previous.telemetry_enabled != settings.telemetry_enabled {
        tracing::info!(target: "settings", enabled = settings.telemetry_enabled, "telemetry toggled");
    }

    #[cfg(desktop)]
    if crate::config::get().features.autostart && previous.autostart != settings.autostart {
        use tauri_plugin_autostart::ManagerExt;
        let r = if settings.autostart {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };
        if let Err(e) = r {
            tracing::warn!(target: "settings", error = %e, "autostart change failed");
            return Err(IpcError::new("autostart", e.to_string()));
        }
    }

    if previous.locale != settings.locale {
        let locale = i18n::resolve(settings.locale.as_deref());
        menu::rebuild(&app, &locale)?;
        let _ = app.emit("starter://locale-changed", &locale);
    }

    if previous.theme != settings.theme {
        let theme = match settings.theme {
            Theme::System => None,
            Theme::Light => Some(tauri::Theme::Light),
            Theme::Dark => Some(tauri::Theme::Dark),
        };
        for w in app.webview_windows().values() {
            let _ = w.set_theme(theme);
        }
    }
    Ok(settings)
}

// ---------------------------------------------------------------- users

#[tauri::command]
pub fn list_users(state: State<'_, AppState>) -> IpcResult<Vec<User>> {
    with_db(&state, db::users::list)
}

#[tauri::command]
pub fn create_user(state: State<'_, AppState>, user: NewUser) -> IpcResult<User> {
    if user.display_name.trim().is_empty() {
        return Err(IpcError::new("validation", "display_name is required"));
    }
    with_db(&state, |d| db::users::create(d, user))
}

#[tauri::command]
pub fn update_user(state: State<'_, AppState>, user: User) -> IpcResult<User> {
    with_db(&state, |d| db::users::update(d, &user))
}

#[tauri::command]
pub fn delete_user(state: State<'_, AppState>, id: String) -> IpcResult<bool> {
    with_db(&state, |d| db::users::delete(d, &id))
}

// ---------------------------------------------------------------- notifications

#[derive(Debug, Deserialize)]
pub struct NewNotification {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub kind: NotificationKind,
    /// Also show it as a native OS notification.
    #[serde(default)]
    pub native: bool,
}

#[tauri::command]
pub fn list_notifications(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> IpcResult<Vec<Notification>> {
    with_db(&state, |d| db::notifications::list(d, limit.unwrap_or(100)))
}

#[tauri::command]
pub fn add_notification(
    app: AppHandle,
    state: State<'_, AppState>,
    notification: NewNotification,
) -> IpcResult<Notification> {
    let n = with_db(&state, |d| {
        db::notifications::insert(
            d,
            &notification.title,
            &notification.body,
            notification.kind,
        )
    })?;
    if notification.native {
        crate::notify(&app, &n.title, &n.body);
    }
    let _ = app.emit("starter://notification", &n);
    Ok(n)
}

#[tauri::command]
pub fn mark_notification_read(
    state: State<'_, AppState>,
    id: i64,
    read: Option<bool>,
) -> IpcResult<bool> {
    with_db(&state, |d| {
        db::notifications::mark_read(d, id, read.unwrap_or(true))
    })
}

#[tauri::command]
pub fn mark_all_notifications_read(state: State<'_, AppState>) -> IpcResult<usize> {
    with_db(&state, db::notifications::mark_all_read)
}

#[tauri::command]
pub fn delete_notification(state: State<'_, AppState>, id: i64) -> IpcResult<bool> {
    with_db(&state, |d| db::notifications::delete(d, id))
}

#[tauri::command]
pub fn clear_notifications(state: State<'_, AppState>) -> IpcResult<usize> {
    with_db(&state, db::notifications::clear)
}

#[tauri::command]
pub fn unread_count(state: State<'_, AppState>) -> IpcResult<i64> {
    with_db(&state, db::notifications::unread_count)
}

// ---------------------------------------------------------------- permissions

#[tauri::command]
pub fn permission_report(app: AppHandle) -> Vec<PermissionReport> {
    permissions::report(&app)
}

#[tauri::command]
pub fn request_permission(app: AppHandle, permission: Permission) -> PermissionState {
    permissions::request(&app, permission)
}

#[tauri::command]
pub fn open_permission_settings(app: AppHandle, permission: Permission) -> bool {
    permissions::open_settings(&app, permission)
}

// ---------------------------------------------------------------- updates

/// Settings "Check for updates". `Ok(Some(v))` = staged, `Ok(None)` = up to
/// date; errors distinguish "not configured" from "check failed".
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> IpcResult<Option<String>> {
    Ok(updater::check(&app).await?)
}

// ---------------------------------------------------------------- logging / crashes

/// Frontend logging entry point (`src/lib/log.ts`). Levels map onto `tracing`
/// so they reach the file log and, if enabled, the remote sinks.
#[tauri::command]
pub fn log_event(
    level: String,
    name: Option<String>,
    message: String,
    fields: Option<Map<String, Value>>,
) {
    let target = name.unwrap_or_else(|| "frontend".into());
    let fields = fields.map(Value::Object).unwrap_or(Value::Null);
    match Level::parse(&level) {
        Level::Error => tracing::error!(target: "frontend", %target, fields = %fields, "{message}"),
        Level::Warn => tracing::warn!(target: "frontend", %target, fields = %fields, "{message}"),
        Level::Info => tracing::info!(target: "frontend", %target, fields = %fields, "{message}"),
        Level::Debug => tracing::debug!(target: "frontend", %target, fields = %fields, "{message}"),
    }
}

#[derive(Debug, Deserialize)]
pub struct FrontendError {
    pub message: String,
    #[serde(default)]
    pub stack: String,
    #[serde(default)]
    pub source: String,
}

/// `window.onerror` / unhandled rejections from the webview.
#[tauri::command]
pub fn report_frontend_error(err: FrontendError) {
    tracing::error!(target: "frontend", source = %err.source, stack = %err.stack, "{}", err.message);
}

#[tauri::command]
pub fn pending_crashes(state: State<'_, AppState>) -> Vec<crash::CrashReport> {
    crash::pending(&state.paths.crashes)
}

#[tauri::command]
pub fn dismiss_crash(state: State<'_, AppState>, id: String) -> bool {
    crash::dismiss(&state.paths.crashes, &id)
}

/// Send one pending report through the telemetry sinks (regardless of the
/// opt-in: the user is explicitly choosing to send this one), then delete it.
#[tauri::command]
pub fn send_crash(state: State<'_, AppState>, id: String) -> IpcResult<bool> {
    let Some(report) = crash::find(&state.paths.crashes, &id) else {
        return Ok(false);
    };
    let mut fields = Map::new();
    fields.insert("crash_id".into(), Value::String(report.id.clone()));
    fields.insert(
        "location".into(),
        Value::String(report.location.clone().unwrap_or_default()),
    );
    fields.insert("thread".into(), Value::String(report.thread.clone()));
    fields.insert("backtrace".into(), Value::String(report.backtrace.clone()));
    fields.insert(
        "crashed_version".into(),
        Value::String(report.app_version.clone()),
    );
    let was_enabled = state.telemetry.is_enabled();
    state.telemetry.set_enabled(true);
    state
        .telemetry
        .emit(Level::Error, "crash", &report.message, fields);
    let flushed = state.telemetry.flush(std::time::Duration::from_secs(10));
    state.telemetry.set_enabled(was_enabled);
    if !flushed {
        return Err(IpcError::new(
            "telemetry",
            "could not deliver the crash report",
        ));
    }
    Ok(crash::dismiss(&state.paths.crashes, &id))
}

/// Debug builds only: exercise the crash reporter end to end.
#[tauri::command]
pub fn test_crash() -> IpcResult<()> {
    if cfg!(debug_assertions) {
        panic!("test crash requested from the UI");
    }
    Err(IpcError::new("forbidden", "only available in debug builds"))
}

// ---------------------------------------------------------------- misc

#[tauri::command]
pub fn open_logs_folder(app: AppHandle, state: State<'_, AppState>) -> IpcResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(state.paths.logs.to_string_lossy(), None::<&str>)
        .map_err(|e| IpcError::new("opener", e.to_string()))
}

/// Delete the encrypted database and rotate its key. Settings return to
/// defaults (with a NEW anonymous device id). The webview should reload.
#[tauri::command]
pub fn erase_all_data(state: State<'_, AppState>) -> IpcResult<()> {
    let mut guard = state.db.write().unwrap_or_else(|p| p.into_inner());
    let placeholder = db::Database::open_in_memory()?;
    drop(std::mem::replace(&mut *guard, placeholder));
    let file = state.paths.db_file();
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", file.display()));
    }
    db::key::delete(&state.meta.identifier)?;
    let key = db::key::load_or_create(&state.meta.identifier)?;
    *guard = db::Database::open(&file, &key)?;
    let fresh = db::settings::load(&guard)?;
    state.telemetry.set_enabled(fresh.telemetry_enabled);
    state.telemetry.set_device_id(Some(fresh.device_id));
    tracing::warn!(target: "settings", "all local data erased");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_error_codes_are_stable() {
        assert_eq!(IpcError::from(DbError::NotFound).code, "not_found");
        assert_eq!(IpcError::from(DbError::WrongKey).code, "db_wrong_key");
        assert_eq!(
            IpcError::from(updater::UpdateError::NotConfigured).code,
            "updater_not_configured"
        );
        let json = serde_json::to_value(IpcError::new("x", "y")).unwrap();
        assert_eq!(json["code"], "x");
    }

    #[test]
    fn new_notification_defaults() {
        let n: NewNotification = serde_json::from_str(r#"{"title":"t"}"#).unwrap();
        assert_eq!(n.kind, NotificationKind::Info);
        assert!(!n.native);
        assert_eq!(n.body, "");
    }
}
