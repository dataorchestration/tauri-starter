//! Rust-side strings (menu labels, native notifications) from the SAME locale
//! files the webview uses (`src/i18n/locales/*.json`). Add a language: drop a
//! JSON file in that folder, `include_str!` it below, and list it in
//! `starter.config.json` → `i18n.supported`.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::config;

const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../../src/i18n/locales/en.json")),
    ("es", include_str!("../../src/i18n/locales/es.json")),
];

fn table() -> &'static HashMap<&'static str, Value> {
    static T: OnceLock<HashMap<&'static str, Value>> = OnceLock::new();
    T.get_or_init(|| {
        LOCALES
            .iter()
            .filter_map(|(code, raw)| {
                serde_json::from_str::<Value>(raw)
                    .map(|v| (*code, v))
                    .map_err(|e| eprintln!("locale {code} is invalid JSON: {e}"))
                    .ok()
            })
            .collect()
    })
}

/// Locales we actually have strings for, intersected with the config list.
pub fn supported() -> Vec<String> {
    let have = table();
    config::get()
        .i18n
        .supported
        .iter()
        .filter(|c| have.contains_key(c.as_str()))
        .cloned()
        .collect()
}

/// Pick the locale to use: the user's saved choice if supported, else the OS
/// language (matched on the primary subtag, so `es-MX` → `es`), else default.
pub fn resolve(saved: Option<&str>) -> String {
    let supported = supported();
    let default = config::get().i18n.default.clone();
    let pick = |cand: &str| -> Option<String> {
        let primary = cand.split(['-', '_']).next().unwrap_or(cand).to_lowercase();
        supported
            .iter()
            .find(|s| s.eq_ignore_ascii_case(cand) || s.eq_ignore_ascii_case(&primary))
            .cloned()
    };
    saved
        .and_then(pick)
        .or_else(|| sys_locale::get_locale().and_then(|l| pick(&l)))
        .unwrap_or(default)
}

fn lookup<'a>(root: &'a Value, key: &str) -> Option<&'a str> {
    key.split('.')
        .try_fold(root, |node, part| node.get(part))
        .and_then(Value::as_str)
}

/// Translate `key` ("menu.settings") in `locale`, falling back to the default
/// locale, then to the key itself so a missing string is visible, not a panic.
pub fn t(locale: &str, key: &str) -> String {
    let tbl = table();
    let default = config::get().i18n.default.as_str();
    tbl.get(locale)
        .and_then(|v| lookup(v, key))
        .or_else(|| tbl.get(default).and_then(|v| lookup(v, key)))
        .unwrap_or(key)
        .to_string()
}

/// [`t`] plus `{{name}}` interpolation (same syntax as i18next on the webview).
pub fn t_with(locale: &str, key: &str, vars: &[(&str, &str)]) -> String {
    let mut s = t(locale, key);
    for (k, v) in vars {
        s = s.replace(&format!("{{{{{k}}}}}"), v);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_has_every_english_key() {
        fn keys(prefix: &str, v: &Value, out: &mut Vec<String>) {
            if let Value::Object(map) = v {
                for (k, child) in map {
                    let full = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    keys(&full, child, out);
                }
            } else {
                out.push(prefix.to_string());
            }
        }
        let tbl = table();
        let mut en_keys = Vec::new();
        keys("", &tbl["en"], &mut en_keys);
        assert!(!en_keys.is_empty());
        for (code, v) in tbl {
            for k in &en_keys {
                assert!(lookup(v, k).is_some(), "locale {code} is missing key {k}");
            }
        }
    }

    #[test]
    fn falls_back_to_default_then_key() {
        assert_eq!(t("en", "menu.settings"), "Settings…");
        assert_eq!(t("xx", "menu.settings"), "Settings…");
        assert_eq!(t("en", "nope.missing"), "nope.missing");
    }

    #[test]
    fn interpolates_variables() {
        assert_eq!(t_with("en", "menu.quit", &[("app", "Demo")]), "Quit Demo");
        assert_eq!(
            t_with("es", "menu.quit", &[("app", "Demo")]),
            "Salir de Demo"
        );
    }

    #[test]
    fn resolve_prefers_saved_then_primary_subtag() {
        assert_eq!(resolve(Some("es")), "es");
        assert_eq!(resolve(Some("es-MX")), "es");
        // An unsupported saved locale must not win over the fallback chain.
        let r = resolve(Some("zz"));
        assert!(supported().contains(&r));
    }
}
