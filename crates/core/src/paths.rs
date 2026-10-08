//! Canonical filesystem locations shared by every RAVENBOT process.
//!
//! The desktop app, the CLI (`ravenbot run` / `list-bots` / `mcp-serve`), and
//! in-app tools must all resolve the **same** paths, or a headless command
//! silently operates on a different database than the window it was launched
//! from.
//!
//! Everything RAVENBOT owns lives under one folder the user can find, back up,
//! and delete: `~/RAVENBOT`. Nothing is scattered into `~/.local/share`,
//! `~/.ravenbot`, and the home directory at the same time. The layout:
//!
//! ```text
//! ~/RAVENBOT/
//!   ravenbot.db            the database (+ its -wal / -shm siblings)
//!   config/                app settings, provider key cache
//!   keys/                  signing keys
//!   logs/                  runtime logs
//!   cache/                 scratch + downloaded model weights
//!   projects/<office>/     an office workspace (the agents' room)
//!   agents/<bot>/          a private workspace for 1:1 conversations
//! ```
//!
//! Every helper is pure path arithmetic — nothing here touches the filesystem
//! except [`ensure_dir`], so callers stay in control of when directories
//! appear. `RAVENBOT_HOME` relocates the whole tree; `RAVENBOT_DB` still
//! overrides just the database file.

use std::path::{Path, PathBuf};

/// Tauri app identifier — must match `identifier` in `src-tauri/tauri.conf.json`.
pub const APP_IDENTIFIER: &str = "com.ravenbot.desktop";

/// The OS-specific base data directory (without the app identifier):
/// - macOS: `~/Library/Application Support`
/// - Windows: `%APPDATA%`
/// - Linux/other Unix: `$XDG_DATA_HOME` or `~/.local/share`
///
/// Used only as a fallback when the home directory cannot be determined, and
/// to find a database written by a pre-`~/RAVENBOT` install.
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

/// Expand a leading `~/` against `$HOME`. A bare `~` is left alone rather than
/// guessed at, so a path the user typed is never silently rewritten.
pub fn expand_home(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            if !home.trim().is_empty() {
                return PathBuf::from(home).join(rest);
            }
        }
    }
    PathBuf::from(raw)
}

/// The one folder everything lives in: `RAVENBOT_HOME`, else `~/RAVENBOT`.
///
/// Falls back to `<os-data-dir>/RAVENBOT` and then `./RAVENBOT` so a headless
/// run with no `HOME` still writes somewhere coherent instead of scattering
/// files into the current directory.
pub fn raven_root() -> PathBuf {
    if let Ok(p) = std::env::var("RAVENBOT_HOME") {
        let p = p.trim();
        if !p.is_empty() {
            return expand_home(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home).join("RAVENBOT");
        }
    }
    data_dir()
        .map(|d| d.join("RAVENBOT"))
        .unwrap_or_else(|| PathBuf::from(".").join("RAVENBOT"))
}

/// `~/RAVENBOT/<name>` for a subdirectory of the root.
fn under_root(name: &str) -> PathBuf {
    raven_root().join(name)
}

/// Where settings, cached provider metadata, and UI preferences live.
pub fn config_dir() -> PathBuf {
    under_root("config")
}

/// Where signing keys and other long-lived secrets live.
pub fn keys_dir() -> PathBuf {
    under_root("keys")
}

/// Where runtime logs are written.
pub fn logs_dir() -> PathBuf {
    under_root("logs")
}

/// Where downloaded model weights and other regenerable data live.
pub fn cache_dir() -> PathBuf {
    under_root("cache")
}

/// The parent of every office workspace.
///
/// `RAVENBOT_PROJECTS_DIR` relocates just this subtree, for a user who wants
/// their work on a different disk while the database stays put.
pub fn projects_dir() -> PathBuf {
    if let Ok(p) = std::env::var("RAVENBOT_PROJECTS_DIR") {
        let p = p.trim();
        if !p.is_empty() {
            return expand_home(p);
        }
    }
    under_root("projects")
}

/// The parent of every private single-agent workspace.
pub fn agents_dir() -> PathBuf {
    under_root("agents")
}

/// A lowercased, filesystem-safe form of `name`.
///
/// Every component collapses to a single `-`, and runs are trimmed, so
/// "Acme   Corp" and "acme-corp" cannot produce two different directories.
/// Non-ASCII input can slugify to nothing; callers get `"workspace"` and
/// should treat that as a name collision rather than a distinct folder.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch);
        } else {
            pending_dash = true;
        }
    }
    if out.is_empty() {
        "workspace".to_string()
    } else {
        out
    }
}

/// The workspace directory for an office, created if it does not exist.
///
/// Every office gets a real folder with the conventional subdirectories and
/// charter files, so an agent arriving on its first turn finds a room rather
/// than an empty path. `suffix` disambiguates a name collision instead of
/// silently sharing one directory between two offices.
pub fn office_workspace(name: &str) -> PathBuf {
    let base = slugify(name);
    let mut dir = projects_dir().join(&base);
    let mut n = 2;
    while dir.exists() && !is_office_dir(&dir) {
        dir = projects_dir().join(format!("{base}-{n}"));
        n += 1;
        if n > 50 {
            break;
        }
    }
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Whether `dir` looks like a workspace RAVENBOT created, rather than a
/// pre-existing folder the user happens to have.
fn is_office_dir(dir: &Path) -> bool {
    dir.join("OFFICE.md").is_file() || dir.join(".ravenbot-office").is_file()
}

/// Mark `dir` as a RAVENBOT office so a later `office_workspace` call with the
/// same name reuses it instead of picking a new suffixed directory.
///
/// Returns true when this call created the marker, which is how callers tell a
/// brand-new room from one they have already announced.
pub fn mark_office_dir(dir: &Path) -> bool {
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let marker = dir.join(".ravenbot-office");
    if marker.exists() {
        return false;
    }
    std::fs::write(&marker, b"1").is_ok()
}

/// The private workspace for a single agent, used by 1:1 conversations that
/// are not part of an office.
pub fn agent_workspace(name: &str) -> PathBuf {
    let dir = agents_dir().join(slugify(name));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Create `dir` and return it.
///
/// Best-effort by design: this crate carries no logging dependency, and every
/// caller is a path-building step that must not fail. A directory that cannot
/// be created surfaces later as a real error — `Database::new` on an
/// unwritable path, or a file tool reporting `no such directory` — which is
/// more useful than a warning nobody configured a subscriber to see.
pub fn ensure_dir(dir: &Path) -> PathBuf {
    let _ = std::fs::create_dir_all(dir);
    dir.to_path_buf()
}

/// The directory that holds `ravenbot.db`.
pub fn app_data_dir() -> Option<PathBuf> {
    Some(raven_root())
}

/// The database written by installs that predate the `~/RAVENBOT` layout.
///
/// Checked once at startup so an upgrade keeps the user's fleet instead of
/// silently presenting an empty app. Returns `None` when there is no legacy
/// file, or when the new path already holds a database.
pub fn legacy_db_path() -> Option<PathBuf> {
    let new = default_db_path();
    if new.exists() {
        return None;
    }
    let old = data_dir()?.join(APP_IDENTIFIER).join("ravenbot.db");
    old.exists().then_some(old)
}

/// Resolve the RAVENBOT database path. `RAVENBOT_DB` overrides everything;
/// otherwise this is `~/RAVENBOT/ravenbot.db`.
///
/// The parent directory is created here, so every caller — the desktop app,
/// the CLI, the MCP server — gets a working path on a fresh machine.
pub fn default_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("RAVENBOT_DB") {
        let p = p.trim();
        if !p.is_empty() {
            let path = expand_home(p);
            if let Some(parent) = path.parent() {
                ensure_dir(parent);
            }
            return path;
        }
    }
    ensure_dir(&raven_root());
    raven_root().join("ravenbot.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `std::env::set_var` is process-global, so two tests that both set
    /// `HOME` race and can read each other's value. Every test that touches the
    /// environment takes this lock for its whole body.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Run `body` with the given env applied, restoring whatever was there.
    fn with_env<T>(vars: &[(&str, Option<&str>)], body: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(k, _)| ((*k).to_string(), std::env::var(k).ok()))
            .collect();
        for (k, v) in vars {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
        let out = body();
        for (k, v) in saved {
            match v {
                Some(val) => std::env::set_var(&k, val),
                None => std::env::remove_var(&k),
            }
        }
        out
    }

    #[test]
    fn raven_root_follows_home_then_the_override() {
        with_env(&[("RAVENBOT_HOME", None), ("HOME", Some("/home/tester"))], || {
            assert_eq!(raven_root(), PathBuf::from("/home/tester/RAVENBOT"));
        });
        with_env(
            &[("RAVENBOT_HOME", Some("/srv/data")), ("HOME", Some("/home/tester"))],
            || {
                assert_eq!(raven_root(), PathBuf::from("/srv/data"));
            },
        );
        // A blank override must not be read as "the current directory".
        with_env(
            &[("RAVENBOT_HOME", Some("   ")), ("HOME", Some("/home/tester"))],
            || {
                assert_eq!(raven_root(), PathBuf::from("/home/tester/RAVENBOT"));
            },
        );
    }

    #[test]
    fn every_subdirectory_hangs_off_the_root() {
        let tmp = std::env::temp_dir().join(format!("rb-root-{}", uuid::Uuid::new_v4()));
        with_env(
            &[
                ("RAVENBOT_HOME", Some(tmp.to_str().unwrap())),
                ("RAVENBOT_DB", None),
                ("RAVENBOT_PROJECTS_DIR", None),
            ],
            || {
                assert_eq!(default_db_path(), tmp.join("ravenbot.db"));
                assert_eq!(projects_dir(), tmp.join("projects"));
                assert_eq!(agents_dir(), tmp.join("agents"));
                assert_eq!(config_dir(), tmp.join("config"));
                assert_eq!(keys_dir(), tmp.join("keys"));
                assert_eq!(logs_dir(), tmp.join("logs"));
                assert_eq!(cache_dir(), tmp.join("cache"));
            },
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn projects_dir_honours_its_own_override() {
        let tmp = std::env::temp_dir().join(format!("rb-proj-{}", uuid::Uuid::new_v4()));
        with_env(
            &[
                ("RAVENBOT_HOME", Some(tmp.to_str().unwrap())),
                ("RAVENBOT_PROJECTS_DIR", Some("/mnt/work")),
                ("RAVENBOT_DB", None),
            ],
            || assert_eq!(projects_dir(), PathBuf::from("/mnt/work")),
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn ravenbot_db_overrides_and_expands_home() {
        with_env(
            &[
                ("RAVENBOT_DB", Some("/tmp/explicit.db")),
                ("RAVENBOT_HOME", None),
            ],
            || assert_eq!(default_db_path(), PathBuf::from("/tmp/explicit.db")),
        );
        with_env(
            &[
                ("RAVENBOT_DB", Some("~/notes/raven.db")),
                ("HOME", Some("/home/tester")),
            ],
            || {
                assert_eq!(
                    default_db_path(),
                    PathBuf::from("/home/tester/notes/raven.db")
                );
            },
        );
    }

    #[test]
    fn legacy_db_is_offered_only_when_the_new_one_is_absent() {
        let tmp = std::env::temp_dir().join(format!("rb-legacy-{}", uuid::Uuid::new_v4()));
        let os_root = tmp.join("osdata").join(APP_IDENTIFIER);
        std::fs::create_dir_all(&os_root).unwrap();
        let old = os_root.join("ravenbot.db");
        std::fs::write(&old, b"old").unwrap();

        // Point the OS data dir at our temp tree on every platform the helper
        // consults, so the test does not depend on the host.
        let vars: Vec<(&'static str, Option<String>)> = vec![
            ("HOME", Some(tmp.to_str().unwrap().to_string())),
            ("XDG_DATA_HOME", Some(os_root.parent().unwrap().to_str().unwrap().to_string())),
            ("APPDATA", Some(os_root.parent().unwrap().to_str().unwrap().to_string())),
            ("RAVENBOT_HOME", Some(tmp.join("new").to_str().unwrap().to_string())),
            ("RAVENBOT_DB", None),
        ];
        let borrowed: Vec<(&str, Option<&str>)> = vars
            .iter()
            .map(|(k, v)| (*k, v.as_deref()))
            .collect();
        with_env(&borrowed, || {
            assert_eq!(legacy_db_path().as_deref(), Some(old.as_path()));

            // Once the new database exists, the legacy one is no longer
            // offered — a second run must not re-adopt it.
            std::fs::write(default_db_path(), b"new").unwrap();
            assert_eq!(legacy_db_path(), None);
        });
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn slugify_collapses_runs_and_never_returns_empty() {
        assert_eq!(slugify("My Office"), "my-office");
        assert_eq!(slugify("  Acme   Corp  "), "acme-corp");
        assert_eq!(slugify("Acme---Corp"), "acme-corp");
        assert_eq!(slugify("R&D / Legal"), "r-d-legal");
        // Non-ASCII collapses to nothing usable, so it gets the documented
        // placeholder rather than an empty directory name.
        assert_eq!(slugify("日本語"), "workspace");
        assert_eq!(slugify(""), "workspace");
        assert_eq!(slugify("!!!"), "workspace");
    }

    #[test]
    fn expand_home_leaves_a_bare_tilde_alone() {
        with_env(&[("HOME", Some("/home/tester"))], || {
            assert_eq!(expand_home("~/x"), PathBuf::from("/home/tester/x"));
            assert_eq!(expand_home("~"), PathBuf::from("~"));
            assert_eq!(expand_home("/abs"), PathBuf::from("/abs"));
        });
    }

    #[test]
    fn office_workspace_reuses_a_marked_directory() {
        let tmp = std::env::temp_dir().join(format!("rb-office-{}", uuid::Uuid::new_v4()));
        with_env(
            &[
                ("RAVENBOT_HOME", Some(tmp.to_str().unwrap())),
                ("RAVENBOT_PROJECTS_DIR", None),
                ("RAVENBOT_DB", None),
            ],
            || {
                let first = office_workspace("Acme Corp");
                assert!(first.ends_with("projects/acme-corp"), "got {}", first.display());
                assert!(first.is_dir());
                mark_office_dir(&first);
                assert!(first.join(".ravenbot-office").is_file());

                // The same name resolves to the same room, not a new one.
                assert_eq!(office_workspace("Acme Corp"), first);

                // An unmarked folder of the same name is stepped over rather
                // than adopted, so RAVENBOT never writes into a directory it
                // did not create.
                let stranger = projects_dir().join("other");
                std::fs::create_dir_all(&stranger).unwrap();
                assert_eq!(office_workspace("other"), projects_dir().join("other-2"));
            },
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn agent_workspace_is_separate_from_office_projects() {
        let tmp = std::env::temp_dir().join(format!("rb-agent-{}", uuid::Uuid::new_v4()));
        with_env(
            &[
                ("RAVENBOT_HOME", Some(tmp.to_str().unwrap())),
                ("RAVENBOT_PROJECTS_DIR", None),
            ],
            || {
                let ws = agent_workspace("Raven Prime");
                assert!(ws.ends_with("agents/raven-prime"), "got {}", ws.display());
                assert!(ws.is_dir());
                assert!(ws.starts_with(raven_root()));
            },
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
