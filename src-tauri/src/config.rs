//! Feature toggles from `starter.config.json` at the repo root.
//!
//! The file is embedded at compile time so the Rust core and the webview (which
//! gets it through Vite's `define`) always read the same configuration. App
//! name / identifier / version are NOT here — they come from `tauri.conf.json`
//! via `tauri::Config` (see [`crate::commands::app_info`]).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::permissions::Permission;

const RAW: &str = include_str!("../../starter.config.json");

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StarterConfig {
    pub app: AppMeta,
    pub features: Features,
    pub permissions: PermissionsCfg,
    pub telemetry: TelemetryCfg,
    pub i18n: I18nCfg,
    pub ui: UiCfg,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppMeta {
    pub org: String,
    pub homepage: String,
    pub support_url: String,
    pub docs_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Features {
    pub tray: bool,
    pub single_instance: bool,
    pub window_state: bool,
    pub autostart: bool,
    pub updater: bool,
    pub notifications: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            tray: true,
            single_instance: true,
            window_state: true,
            autostart: true,
            updater: false,
            notifications: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PermissionsCfg {
    pub required: Vec<Permission>,
    pub optional: Vec<Permission>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TelemetryCfg {
    pub default_enabled: bool,
    pub sinks: Sinks,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sinks {
    pub file: FileSinkCfg,
    pub sentry: SentrySinkCfg,
    pub http: HttpSinkCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FileSinkCfg {
    pub enabled: bool,
    pub max_files: usize,
}

impl Default for FileSinkCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            max_files: 7,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SentrySinkCfg {
    pub dsn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HttpSinkCfg {
    pub endpoint: String,
    pub api_key_header: String,
    pub api_key: String,
    pub batch_size: usize,
    pub flush_interval_secs: u64,
}

impl Default for HttpSinkCfg {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            api_key_header: "x-api-key".into(),
            api_key: String::new(),
            batch_size: 20,
            flush_interval_secs: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct I18nCfg {
    pub default: String,
    pub supported: Vec<String>,
}

impl Default for I18nCfg {
    fn default() -> Self {
        Self {
            default: "en".into(),
            supported: vec!["en".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiCfg {
    pub default_theme: String,
}

impl Default for UiCfg {
    fn default() -> Self {
        Self {
            default_theme: "system".into(),
        }
    }
}

/// Parse a config document. Unknown keys (like `$comment`) are ignored and
/// every field has a default, so a partial file is fine.
pub fn parse(raw: &str) -> Result<StarterConfig, serde_json::Error> {
    serde_json::from_str(raw)
}

/// The embedded config, parsed once. A malformed file falls back to defaults
/// (and says so on stderr) rather than refusing to start.
pub fn get() -> &'static StarterConfig {
    static CFG: OnceLock<StarterConfig> = OnceLock::new();
    CFG.get_or_init(|| {
        parse(RAW).unwrap_or_else(|e| {
            eprintln!("starter.config.json is invalid ({e}); using defaults");
            StarterConfig::default()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_config_parses() {
        let cfg = parse(RAW).expect("starter.config.json must be valid");
        assert!(cfg.i18n.supported.contains(&cfg.i18n.default));
        assert!(cfg.telemetry.sinks.http.batch_size > 0);
    }

    #[test]
    fn partial_document_fills_defaults() {
        let cfg = parse(r#"{"features":{"tray":false}}"#).unwrap();
        assert!(!cfg.features.tray);
        assert!(cfg.features.single_instance);
        assert_eq!(cfg.i18n.default, "en");
        assert_eq!(cfg.telemetry.sinks.http.api_key_header, "x-api-key");
    }

    #[test]
    fn permissions_use_snake_case_wire_names() {
        let cfg = parse(
            r#"{"permissions":{"required":["screen_recording"],"optional":["notifications"]}}"#,
        )
        .unwrap();
        assert_eq!(cfg.permissions.required, vec![Permission::ScreenRecording]);
        assert_eq!(cfg.permissions.optional, vec![Permission::Notifications]);
    }
}
