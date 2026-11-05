//! Application menu and (optional) tray icon, labelled from the shared locale
//! files. Rebuilt when the user changes language.
//!
//! Menu/tray clicks are routed by item id through [`handle`], which either
//! navigates the main window (`starter://navigate`) or performs the action.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::i18n::t_with;

pub const TRAY_ID: &str = "main-tray";
pub const NAVIGATE_EVENT: &str = "starter://navigate";

fn app_name(app: &AppHandle) -> String {
    app.package_info().name.clone()
}

pub fn build(app: &AppHandle, locale: &str) -> tauri::Result<Menu<Wry>> {
    let name = app_name(app);
    let l = |key: &str| t_with(locale, key, &[("app", &name)]);

    let settings = MenuItem::with_id(
        app,
        "settings",
        l("menu.settings"),
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let check = MenuItem::with_id(
        app,
        "check_updates",
        l("menu.check_updates"),
        true,
        None::<&str>,
    )?;

    let mut top: Vec<Submenu<Wry>> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        top.push(Submenu::with_items(
            app,
            &name,
            true,
            &[
                &PredefinedMenuItem::about(app, Some(&l("menu.about")), None)?,
                &PredefinedMenuItem::separator(app)?,
                &settings,
                &check,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, Some(&l("menu.hide")))?,
                &PredefinedMenuItem::hide_others(app, None)?,
                &PredefinedMenuItem::show_all(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, Some(&l("menu.quit")))?,
            ],
        )?);
        top.push(Submenu::with_items(
            app,
            l("menu.file"),
            true,
            &[&PredefinedMenuItem::close_window(
                app,
                Some(&l("menu.close_window")),
            )?],
        )?);
    }
    #[cfg(not(target_os = "macos"))]
    {
        top.push(Submenu::with_items(
            app,
            l("menu.file"),
            true,
            &[
                &settings,
                &check,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::close_window(app, Some(&l("menu.close_window")))?,
                &PredefinedMenuItem::quit(app, Some(&l("menu.quit")))?,
            ],
        )?);
    }

    top.push(Submenu::with_items(
        app,
        l("menu.edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?);
    top.push(Submenu::with_items(
        app,
        l("menu.view"),
        true,
        &[&PredefinedMenuItem::fullscreen(
            app,
            Some(&l("menu.fullscreen")),
        )?],
    )?);
    top.push(Submenu::with_items(
        app,
        l("menu.window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(&l("menu.minimize")))?,
            &PredefinedMenuItem::maximize(app, Some(&l("menu.zoom")))?,
        ],
    )?);
    top.push(Submenu::with_items(
        app,
        l("menu.help"),
        true,
        &[
            &MenuItem::with_id(app, "docs", l("menu.docs"), true, None::<&str>)?,
            &MenuItem::with_id(
                app,
                "report_issue",
                l("menu.report_issue"),
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "show_logs", l("menu.show_logs"), true, None::<&str>)?,
        ],
    )?);

    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = top
        .iter()
        .map(|s| s as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();
    Menu::with_items(app, &refs)
}

fn tray_menu(app: &AppHandle, locale: &str) -> tauri::Result<Menu<Wry>> {
    let name = app_name(app);
    let l = |key: &str| t_with(locale, key, &[("app", &name)]);
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "show", l("menu.show"), true, None::<&str>)?,
            &MenuItem::with_id(app, "settings", l("menu.settings"), true, None::<&str>)?,
            &MenuItem::with_id(
                app,
                "check_updates",
                l("menu.check_updates"),
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", l("menu.quit"), true, None::<&str>)?,
        ],
    )
}

pub fn build_tray(app: &AppHandle, locale: &str) -> tauri::Result<()> {
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;
    tauri::tray::TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(app_name(app))
        .menu(&tray_menu(app, locale)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle(app, event.id().as_ref()))
        .build(app)?;
    Ok(())
}

/// Re-label everything after a locale change.
pub fn rebuild(app: &AppHandle, locale: &str) -> tauri::Result<()> {
    app.set_menu(build(app, locale)?)?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(tray_menu(app, locale)?))?;
    }
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn navigate(app: &AppHandle, view: &str) {
    show_main(app);
    let _ = app.emit(NAVIGATE_EVENT, serde_json::json!({ "view": view }));
}

pub fn handle(app: &AppHandle, id: &str) {
    use tauri_plugin_opener::OpenerExt;
    let cfg = crate::config::get();
    match id {
        "show" => show_main(app),
        "settings" => navigate(app, "settings"),
        "check_updates" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::updater::check_and_notify(&app, true).await
            });
        }
        "docs" => {
            let _ = app.opener().open_url(&cfg.app.docs_url, None::<&str>);
        }
        "report_issue" => {
            let _ = app.opener().open_url(&cfg.app.support_url, None::<&str>);
        }
        "show_logs" => {
            if let Some(state) = app.try_state::<crate::AppState>() {
                let _ = app
                    .opener()
                    .open_path(state.paths.logs.to_string_lossy(), None::<&str>);
            }
        }
        "quit" => app.exit(0),
        _ => tracing::debug!(target: "menu", id, "unhandled menu item"),
    }
}

#[cfg(test)]
mod tests {
    use crate::i18n::t;

    #[test]
    fn every_menu_id_has_a_label_in_the_default_locale() {
        for key in [
            "menu.settings",
            "menu.check_updates",
            "menu.docs",
            "menu.report_issue",
            "menu.show_logs",
            "menu.show",
            "menu.quit",
        ] {
            assert_ne!(t("en", key), key, "missing label for {key}");
        }
    }
}
