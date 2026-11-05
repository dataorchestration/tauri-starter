//! Where the app keeps its files. Resolved once at startup from Tauri's path
//! resolver (platform-correct: `~/Library/Application Support/<name>` on macOS,
//! `%APPDATA%\<identifier>` on Windows, `~/.local/share/<identifier>` on Linux).

use std::fs;
use std::path::{Path, PathBuf};

use tauri::Manager;

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Database, markers, anything persistent.
    pub data: PathBuf,
    /// Rolling log files.
    pub logs: PathBuf,
    /// Crash reports awaiting the user's decision (send / dismiss).
    pub crashes: PathBuf,
}

impl AppPaths {
    pub fn resolve(app: &tauri::AppHandle) -> tauri::Result<Self> {
        let data = app.path().app_data_dir()?;
        let logs = app.path().app_log_dir()?;
        Self::in_dirs(data, logs)
    }

    /// Build the layout under explicit roots (tests use a temp dir).
    pub fn in_dirs(data: PathBuf, logs: PathBuf) -> tauri::Result<Self> {
        let crashes = data.join("crashes");
        for d in [&data, &logs, &crashes] {
            fs::create_dir_all(d)?;
        }
        Ok(Self {
            data,
            logs,
            crashes,
        })
    }

    pub fn legacy_db_file(&self) -> PathBuf {
        self.data.join("app.sqlite")
    }

    pub fn turso_db_file(&self) -> PathBuf {
        self.data.join("app.turso.sqlite")
    }

    pub fn db_file(&self) -> PathBuf {
        if cfg!(feature = "database-turso") {
            self.turso_db_file()
        } else {
            self.legacy_db_file()
        }
    }
}

/// Strip the home directory for display ("~/Library/…") so paths in the UI and
/// in telemetry don't leak the account name.
pub fn display(path: &Path) -> String {
    let s = path.to_string_lossy().into_owned();
    match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        Some(home) => {
            let home = home.to_string_lossy();
            s.strip_prefix(home.as_ref())
                .map(|rest| format!("~{rest}"))
                .unwrap_or(s)
        }
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_the_layout() {
        let root = std::env::temp_dir().join(format!("starter-paths-{}", uuid::Uuid::new_v4()));
        let p = AppPaths::in_dirs(root.join("data"), root.join("logs")).unwrap();
        assert!(p.crashes.is_dir());
        assert!(p.logs.is_dir());
        assert_eq!(
            p.db_file(),
            if cfg!(feature = "database-turso") {
                p.turso_db_file()
            } else {
                p.legacy_db_file()
            }
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn display_hides_home() {
        if let Some(home) = std::env::var_os("HOME") {
            let p = Path::new(&home).join("Library").join("x");
            assert!(display(&p).starts_with("~/"));
        }
        assert_eq!(display(Path::new("/tmp/x")), "/tmp/x");
    }
}
