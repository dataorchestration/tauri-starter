//! Hello Desktop — Tauri starter core.
//!
//! Startup order matters and is documented inline in [`run`]: paths → keychain
//! key → encrypted DB → settings → telemetry (so Sentry's panic hook exists) →
//! crash hook (chains onto it) → menu/tray → state → launch update check.

pub mod commands;
pub mod config;
pub mod crash;
pub mod db;
pub mod health;
pub mod i18n;
pub mod menu;
pub mod paths;
pub mod permissions;
pub mod telemetry;
pub mod updater;

use tokio::sync::RwLock;

use tauri::{AppHandle, Manager};

use crate::db::Database;
use crate::paths::AppPaths;
use crate::telemetry::{Meta, Telemetry};

/// Everything commands need, managed by Tauri (`tauri::State<AppState>`).
pub struct AppState {
    /// Async lifecycle lock: erase drains readers before closing the engine.
    /// None means a reset failed; operations must fail closed until recovery.
    pub db: RwLock<Option<Database>>,
    pub paths: AppPaths,
    pub telemetry: Telemetry,
    pub meta: Meta,
}

/// The locale currently in effect (saved choice → OS language → default).
pub async fn current_locale(app: &AppHandle) -> String {
    let saved = if let Some(state) = app.try_state::<AppState>() {
        let db = state.db.read().await;
        if let Some(db) = db.as_ref() {
            db::settings::load(db).await.ok().and_then(|st| st.locale)
        } else {
            None
        }
    } else {
        None
    };
    i18n::resolve(saved.as_deref())
}

/// Native OS notification, best effort. No-op when the feature is off; a
/// missing permission is requested once and otherwise just logged.
pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if !config::get().features.notifications {
        return;
    }
    use tauri_plugin_notification::{NotificationExt, PermissionState};
    let n = app.notification();
    if !matches!(n.permission_state(), Ok(PermissionState::Granted)) {
        let _ = n.request_permission();
    }
    if let Err(e) = n.builder().title(title).body(body).show() {
        tracing::warn!(target: "notify", error = %e, "native notification failed");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::get();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        // Must be the first plugin so a second launch is intercepted before
        // anything else initialises.
        if cfg.features.single_instance {
            builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                menu::show_main(app);
            }));
        }
        if cfg.features.window_state {
            builder = builder.plugin(tauri_plugin_window_state::Builder::new().build());
        }
        if cfg.features.autostart {
            builder = builder.plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ));
        }
        if cfg.features.updater {
            builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
        }
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let info = app.package_info();
            let meta = Meta::new(
                &info.name,
                &info.version.to_string(),
                &app.config().identifier,
            );

            let paths = AppPaths::resolve(&handle)?;

            // Encrypted store: key from the keychain, DB under app data.
            let key = db::key::load_or_create(&meta.identifier)?;
            // setup is synchronous and runs before commands are accepted. This
            // is the sole sync/async bridge; command paths await directly.
            let (database, settings) = tauri::async_runtime::block_on(async {
                let database = Database::open_for_app(&paths, &key).await?;
                let settings = db::settings::load(&database).await?;
                Ok::<_, db::DbError>((database, settings))
            })?;

            // Telemetry before the crash hook so Sentry's panic integration is
            // the "previous hook" ours chains to.
            let telemetry = telemetry::init(
                &paths,
                &cfg.telemetry,
                meta.clone(),
                settings.telemetry_enabled,
                settings.device_id.clone(),
            );
            crash::install(paths.crashes.clone(), meta.clone());

            let locale = i18n::resolve(settings.locale.as_deref());
            app.set_menu(menu::build(&handle, &locale)?)?;
            app.on_menu_event(|app, event| menu::handle(app, event.id().as_ref()));
            if cfg.features.tray {
                menu::build_tray(&handle, &locale)?;
            }

            app.manage(AppState {
                db: RwLock::new(Some(database)),
                paths,
                telemetry,
                meta,
            });

            tracing::info!(target: "app", locale, "started");
            updater::spawn_launch_check(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // macOS convention with a tray: closing the window keeps the app
            // running; the tray / dock / ⌘Q quit it.
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && config::get().features.tray
                && window.label() == "main"
            {
                let _ = window.hide();
                api.prevent_close();
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (window, event);
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::health_check,
            commands::get_settings,
            commands::save_settings,
            commands::list_users,
            commands::create_user,
            commands::update_user,
            commands::delete_user,
            commands::list_notifications,
            commands::add_notification,
            commands::mark_notification_read,
            commands::mark_all_notifications_read,
            commands::delete_notification,
            commands::clear_notifications,
            commands::unread_count,
            commands::create_vector_collection,
            commands::upsert_vector,
            commands::get_vector,
            commands::search_vectors,
            commands::delete_vector,
            commands::delete_vector_collection,
            commands::permission_report,
            commands::request_permission,
            commands::open_permission_settings,
            commands::check_for_update,
            commands::log_event,
            commands::report_frontend_error,
            commands::pending_crashes,
            commands::dismiss_crash,
            commands::send_crash,
            commands::test_crash,
            commands::open_logs_folder,
            commands::erase_all_data,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the application")
        .run(|app, event| {
            // Dock icon click with no visible window (macOS) re-shows main.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } = event
            {
                menu::show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
