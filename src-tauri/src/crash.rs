//! Crash reporting.
//!
//! A panic hook writes a JSON [`CrashReport`] (message, location, backtrace,
//! version, OS — nothing else) to the crashes dir, then chains to the previous
//! hook (Sentry's, when enabled) and lets the process abort. On the next launch
//! the UI finds the pending reports and asks the user to send or dismiss them.
//! Frontend errors come in through [`crate::commands::report_frontend_error`].

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::telemetry::Meta;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashReport {
    pub id: String,
    pub ts_ms: i64,
    pub app_version: String,
    pub os: String,
    pub message: String,
    pub location: Option<String>,
    pub thread: String,
    pub backtrace: String,
}

/// Install the panic hook. Idempotent in effect: calling twice chains twice.
pub fn install(dir: PathBuf, meta: Meta) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = from_panic(info, &meta);
        match write_report(&dir, &report) {
            Ok(p) => eprintln!("crash report written to {}", p.display()),
            Err(e) => eprintln!("failed to write crash report: {e}"),
        }
        prev(info);
    }));
}

pub fn from_panic(info: &std::panic::PanicHookInfo<'_>, meta: &Meta) -> CrashReport {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic with non-string payload".into());
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
    build(meta, message, location)
}

/// Build a report from parts (also used for tests and for non-panic fatal
/// errors you want surfaced the same way).
pub fn build(meta: &Meta, message: String, location: Option<String>) -> CrashReport {
    CrashReport {
        id: uuid::Uuid::new_v4().to_string(),
        ts_ms: crate::db::now_millis(),
        app_version: meta.app_version.clone(),
        os: format!("{} {}", meta.os, meta.arch),
        message,
        location,
        thread: std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string(),
        backtrace: std::backtrace::Backtrace::force_capture().to_string(),
    }
}

fn file_for(dir: &Path, report: &CrashReport) -> PathBuf {
    dir.join(format!("crash-{}-{}.json", report.ts_ms, report.id))
}

pub fn write_report(dir: &Path, report: &CrashReport) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = file_for(dir, report);
    let json = serde_json::to_vec_pretty(report).map_err(io::Error::other)?;
    fs::write(&path, json)?;
    Ok(path)
}

/// Reports not yet sent or dismissed, oldest first. Unreadable files are
/// skipped (and left in place).
pub fn pending(dir: &Path) -> Vec<CrashReport> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<CrashReport> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| fs::read(e.path()).ok())
        .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
        .collect();
    out.sort_by_key(|r| r.ts_ms);
    out
}

pub fn find(dir: &Path, id: &str) -> Option<CrashReport> {
    pending(dir).into_iter().find(|r| r.id == id)
}

/// Delete the report file for `id`. Returns whether one was removed.
pub fn dismiss(dir: &Path, id: &str) -> bool {
    match find(dir, id) {
        Some(r) => fs::remove_file(file_for(dir, &r)).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("starter-crash-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn write_pending_find_dismiss_roundtrip() {
        let dir = tmp();
        let meta = Meta::new("T", "1.2.3", "com.example.t");
        let a = build(&meta, "first".into(), Some("src/x.rs:1:1".into()));
        let b = build(&meta, "second".into(), None);
        write_report(&dir, &b).unwrap();
        write_report(&dir, &a).unwrap();
        // Garbage file is ignored.
        fs::write(dir.join("junk.json"), b"{not json").unwrap();

        let p = pending(&dir);
        assert_eq!(p.len(), 2);
        assert!(p[0].ts_ms <= p[1].ts_ms);
        assert_eq!(find(&dir, &a.id).unwrap(), a);
        assert!(a.backtrace.len() > 10, "backtrace captured");
        assert_eq!(a.app_version, "1.2.3");

        assert!(dismiss(&dir, &a.id));
        assert!(!dismiss(&dir, &a.id));
        assert_eq!(pending(&dir).len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_dir_is_empty_not_an_error() {
        assert!(pending(Path::new("/definitely/not/here")).is_empty());
    }
}
