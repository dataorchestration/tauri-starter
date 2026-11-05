//! Auto-updates, driven by the Rust core so they work no matter which window
//! is open. Checked once at launch (when the feature is on and configured) and
//! on demand from Settings / the menu. A found update is downloaded and staged;
//! the UI gets `starter://update-ready` with the version and offers a restart.
//!
//! Setup: generate a keypair (`npm run tauri signer generate -- -w ~/.tauri/<app>.key`),
//! paste the public key into `tauri.conf.json` → `plugins.updater.pubkey`,
//! set `features.updater: true` in `starter.config.json`, and publish
//! `latest.json` + the signed bundles at the configured endpoint. See
//! docs/RELEASING.md.

use tauri::{AppHandle, Emitter};

pub const UPDATE_READY_EVENT: &str = "starter://update-ready";

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("the auto-updater is not configured for this build")]
    NotConfigured,
    #[error("{0}")]
    Failed(String),
}

/// Check the feed; download + stage a newer version. `Ok(Some(version))` when
/// something was staged, `Ok(None)` when up to date.
pub async fn check(app: &AppHandle) -> Result<Option<String>, UpdateError> {
    if !crate::health::updater_configured(app) {
        return Err(UpdateError::NotConfigured);
    }
    #[cfg(desktop)]
    {
        use tauri_plugin_updater::UpdaterExt;
        let updater = app
            .updater_builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| UpdateError::Failed(e.to_string()))?;
        match updater
            .check()
            .await
            .map_err(|e| UpdateError::Failed(e.to_string()))?
        {
            Some(update) => {
                let version = update.version.clone();
                tracing::info!(target: "updater", %version, "update available; downloading");
                update
                    .download_and_install(|_, _| {}, || {})
                    .await
                    .map_err(|e| UpdateError::Failed(e.to_string()))?;
                let _ = app.emit(UPDATE_READY_EVENT, &version);
                tracing::info!(target: "updater", %version, "update staged");
                Ok(Some(version))
            }
            None => {
                tracing::info!(target: "updater", "up to date");
                Ok(None)
            }
        }
    }
    #[cfg(not(desktop))]
    {
        Ok(None)
    }
}

/// Run a check and surface the outcome as a native notification. With
/// `verbose` (user-initiated) "up to date" and failures are announced too;
/// the silent launch check only speaks when an update is staged.
pub async fn check_and_notify(app: &AppHandle, verbose: bool) {
    let locale = crate::current_locale(app).await;
    let name = app.package_info().name.clone();
    match check(app).await {
        Ok(Some(v)) => crate::notify(
            app,
            &crate::i18n::t_with(&locale, "update.ready", &[("version", &v)]),
            &crate::i18n::t(&locale, "update.restart"),
        ),
        Ok(None) if verbose => {
            crate::notify(app, &name, &crate::i18n::t(&locale, "settings.up_to_date"))
        }
        Err(e) if verbose => crate::notify(app, &name, &e.to_string()),
        Ok(None) => {}
        Err(e) => tracing::warn!(target: "updater", error = %e, "launch update check failed"),
    }
}

/// Fire-and-forget launch check (only when configured).
pub fn spawn_launch_check(app: AppHandle) {
    if !crate::health::updater_configured(&app) {
        return;
    }
    tauri::async_runtime::spawn(async move { check_and_notify(&app, false).await });
}
