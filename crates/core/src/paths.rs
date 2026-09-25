//! Canonical filesystem locations shared by every RAVENBOT process.
//!
//! The desktop app, the CLI (`ravenbot run` / `list-bots` / `mcp-serve`), and
//! in-app tools must all resolve the **same** database file, or headless
//! commands silently operate on an empty database. Tauri stores app data under
//! `<os-data-dir>/<identifier>/`; these helpers reproduce that exactly.

use std::path::PathBuf;

/// Tauri app identifier — must match `identifier` in `src-tauri/tauri.conf.json`.
pub const APP_IDENTIFIER: &str = "com.ravenbot.desktop";

/// The OS-specific base data directory (without the app identifier):
/// - macOS: `~/Library/Application Support`
/// - Windows: `%APPDATA%`
/// - Linux/other Unix: `$XDG_DATA_HOME` or `~/.local/share`
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".local/share"))
            })
    }
}

/// The directory that holds `ravenbot.db`.
pub fn app_data_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join(APP_IDENTIFIER))
}

/// Resolve the RAVENBOT database path. `RAVENBOT_DB` overrides everything;
/// otherwise this is `<app_data_dir>/ravenbot.db`, matching the desktop app.
pub fn default_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("RAVENBOT_DB") {
        let p = p.trim();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    app_data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ravenbot.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_db_path_is_under_the_app_identifier() {
        // Save/restore so a set RAVENBOT_DB doesn't leak into other tests.
        let saved = std::env::var("RAVENBOT_DB").ok();
        std::env::remove_var("RAVENBOT_DB");

        let path = default_db_path();
        let s = path.to_string_lossy();
        assert!(
            s.ends_with("com.ravenbot.desktop/ravenbot.db")
                || s.ends_with("com.ravenbot.desktop\\ravenbot.db"),
            "got: {s}"
        );

        // Override wins.
        std::env::set_var("RAVENBOT_DB", "/tmp/x.db");
        assert_eq!(default_db_path(), PathBuf::from("/tmp/x.db"));
        std::env::remove_var("RAVENBOT_DB");

        if let Some(v) = saved {
            std::env::set_var("RAVENBOT_DB", v);
        }
    }
}
