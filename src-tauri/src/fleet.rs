//! One fleet owner at a time.
//!
//! The desktop app and the headless daemon both own the same SQLite database,
//! scheduler, office loops and webhook receiver. Running both would duplicate
//! every timer and every writer. This module takes an exclusive lock on
//! `~/RAVENBOT/fleet.lock`; the lock is released by the OS even after a crash,
//! so a stale file is harmless.
//!
//! A desktop launch that finds the daemon owning the fleet can take over: it
//! stops the user service, waits for the lock, and continues. A second desktop
//! launch is left to the single-instance plugin.

use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct FleetLock {
    file: File,
}

#[derive(Debug)]
pub enum FleetLockError {
    Busy { owner: Option<String> },
    Io(String),
}

impl std::fmt::Display for FleetLockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy { owner: Some(owner) } => {
                write!(formatter, "another fleet owner is running ({owner})")
            }
            Self::Busy { owner: None } => write!(formatter, "another fleet owner is running"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl FleetLock {
    pub fn acquire(mode: &str) -> Result<Self, FleetLockError> {
        Self::acquire_at(&lock_path(), mode)
    }

    pub fn acquire_at(path: &Path, mode: &str) -> Result<Self, FleetLockError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| FleetLockError::Io(error.to_string()))?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| FleetLockError::Io(error.to_string()))?;
        match file.try_lock_exclusive() {
            Ok(()) => {
                let _ = file.set_len(0);
                let _ = file.seek(SeekFrom::Start(0));
                let _ = writeln!(
                    file,
                    "pid={} mode={mode} since={}",
                    std::process::id(),
                    chrono::Utc::now().to_rfc3339()
                );
                let _ = file.flush();
                Ok(Self { file })
            }
            Err(error) if is_contended(&error) => Err(FleetLockError::Busy {
                owner: read_owner(path),
            }),
            Err(error) => Err(FleetLockError::Io(error.to_string())),
        }
    }
}

impl Drop for FleetLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn is_contended(error: &std::io::Error) -> bool {
    if error.kind() == ErrorKind::WouldBlock {
        return true;
    }
    // Windows returns ERROR_LOCK_VIOLATION (33); some Unix libcs use EAGAIN.
    matches!(error.raw_os_error(), Some(33) | Some(11) | Some(35))
}

pub fn lock_path() -> PathBuf {
    // Tie the lock to the database location, including a `RAVENBOT_DB`
    // override, so two processes cannot own the same SQLite file through
    // different homes.
    ravenbot_core::default_db_path()
        .parent()
        .map(|parent| parent.join("fleet.lock"))
        .unwrap_or_else(|| ravenbot_core::raven_root().join("fleet.lock"))
}

fn read_owner(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn owner_mode(owner: &str) -> Option<&str> {
    owner
        .lines()
        .find_map(|line| line.strip_prefix("mode=").map(str::trim))
}

pub fn wait_for_lock(path: &Path, mode: &str, timeout: Duration) -> Result<FleetLock, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match FleetLock::acquire_at(path, mode) {
            Ok(lock) => return Ok(lock),
            Err(FleetLockError::Busy { .. }) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(FleetLockError::Busy { owner }) => {
                return Err(format!(
                    "the background service still owns the fleet{}",
                    owner.map(|owner| format!(" ({owner})")).unwrap_or_default()
                ));
            }
            Err(FleetLockError::Io(error)) => return Err(error),
        }
    }
}

/// Acquire the desktop lock, offering to stop a running background daemon.
pub fn acquire_for_desktop() -> Result<FleetLock, String> {
    let path = lock_path();
    match FleetLock::acquire_at(&path, "desktop") {
        Ok(lock) => Ok(lock),
        Err(FleetLockError::Busy { owner }) => {
            let owner = owner.unwrap_or_else(|| "unknown process".to_string());
            if owner_mode(&owner) == Some("desktop") {
                return Err("RAVENBOT is already running in another window.".to_string());
            }
            if !confirm_takeover(&owner) {
                return Err(format!(
                    "RAVENBOT is running as a background service ({owner}). Stop it with `ravenbot service stop` and try again."
                ));
            }
            if let Err(error) = crate::service::stop_running() {
                tracing::warn!(%error, "Could not stop the background service");
            }
            wait_for_lock(&path, "desktop", Duration::from_secs(10)).map_err(|error| {
                format!("Could not take over from the background service: {error}")
            })
        }
        Err(FleetLockError::Io(error)) => Err(format!("Cannot create the fleet lock: {error}")),
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new(program).args(args).output().ok()
}

/// Yes means take over. If no native dialog is available, the desktop launch
/// proceeds (the user explicitly opened the app and the fleet keeps running).
fn confirm_takeover(owner: &str) -> bool {
    let message = format!(
        "RAVENBOT is running as a background service.\n\n{owner}\n\nStop it and open this desktop window? The fleet keeps running either way."
    );
    #[cfg(target_os = "linux")]
    {
        if let Some(output) = command_output(
            "zenity",
            &[
                "--question",
                "--title",
                "RAVENBOT",
                "--text",
                &message,
                "--ok-label",
                "Take over",
                "--cancel-label",
                "Keep service",
            ],
        ) {
            return output.status.success();
        }
        if let Some(output) = command_output(
            "kdialog",
            &[
                "--title",
                "RAVENBOT",
                "--yesno",
                &message,
                "--yes-label",
                "Take over",
                "--no-label",
                "Keep service",
            ],
        ) {
            return output.status.success();
        }
    }
    #[cfg(target_os = "macos")]
    {
        let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
        if let Some(output) = command_output(
            "osascript",
            &[
                "-e",
                &format!(
                    "display dialog \"{escaped}\" buttons {{\"Keep service\", \"Take over\"}} default button \"Take over\" with title \"RAVENBOT\""
                ),
            ],
        ) {
            return output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("Take over");
        }
    }
    #[cfg(target_os = "windows")]
    {
        let escaped = message.replace('\'', "''");
        if let Some(output) = command_output(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                &format!(
                    "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{escaped}','RAVENBOT','YesNo','Question') -eq 'Yes'"
                ),
            ],
        ) {
            return output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("True");
        }
    }
    tracing::warn!("No native confirmation dialog; taking over the fleet");
    true
}

pub fn show_error(title: &str, message: &str) {
    eprintln!("{title}: {message}");
    #[cfg(target_os = "linux")]
    {
        if command_output("zenity", &["--error", "--title", title, "--text", message]).is_some() {
            return;
        }
        let _ = command_output("kdialog", &["--title", title, "--error", message]);
    }
    #[cfg(target_os = "macos")]
    {
        let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = command_output(
            "osascript",
            &[
                "-e",
                &format!("display dialog \"{escaped}\" buttons {{\"OK\"}} with icon stop with title \"{title}\""),
            ],
        );
    }
    #[cfg(target_os = "windows")]
    {
        let escaped = message.replace('\'', "''");
        let _ = command_output(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                &format!(
                    "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{escaped}','{title}','OK','Error')"
                ),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lock() -> PathBuf {
        std::env::temp_dir().join(format!("ravenbot-fleet-{}.lock", uuid::Uuid::new_v4()))
    }

    #[test]
    fn a_second_owner_is_denied_and_the_owner_is_readable() {
        let path = temp_lock();
        let first = FleetLock::acquire_at(&path, "daemon").expect("first lock");
        let error = FleetLock::acquire_at(&path, "desktop").expect_err("must be busy");
        match error {
            FleetLockError::Busy { owner } => {
                let owner = owner.expect("owner text");
                assert!(owner.contains("mode=daemon"));
            }
            other => panic!("expected busy, got {other}"),
        }
        drop(first);
        let second = FleetLock::acquire_at(&path, "desktop").expect("released lock");
        drop(second);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn waiting_for_a_released_lock_succeeds() {
        let path = temp_lock();
        let first = FleetLock::acquire_at(&path, "daemon").expect("first lock");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(first);
        });
        let lock = wait_for_lock(&path, "desktop", Duration::from_secs(2)).expect("takeover");
        drop(lock);
        let _ = std::fs::remove_file(path);
    }
}
