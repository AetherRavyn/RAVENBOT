//! Background process registry for `task_runner`.
//!
//! A long build or test run should not block a model round just to be observed.
//! The registry owns the child process, captures a bounded output tail, and lets
//! a later `wait` release on exit, a matching pattern, or a deadline.
//!
//! The registry is process-global because skills have no runtime handle. It is
//! intentionally in-memory: a background process belongs to the app instance
//! that started it, and `kill_on_drop` prevents an orphan when the app exits.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::AsyncReadExt;
use tokio::process::Child;
use uuid::Uuid;

const MAX_OUTPUT_CHARS: usize = 200_000;
const FINISHED_RETENTION_SECS: i64 = 30 * 60;

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundSnapshot {
    pub process_id: String,
    pub command: String,
    pub cwd: String,
    pub pid: Option<u32>,
    pub status: String,
    pub exit_code: Option<i32>,
    pub output: String,
    pub started_at: String,
    pub updated_at: String,
    pub pattern_matched: bool,
    pub bot_id: String,
    pub thread_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone)]
pub struct WaitOutcome {
    pub snapshot: BackgroundSnapshot,
    pub matched_pattern: bool,
    pub timed_out: bool,
}

struct ProcessState {
    status: String,
    exit_code: Option<i32>,
    output: String,
    pattern: Option<String>,
    pattern_matched: bool,
    stop_requested: bool,
    updated_at: DateTime<Utc>,
}

struct Entry {
    id: Uuid,
    command: String,
    cwd: String,
    pid: Option<u32>,
    started_at: DateTime<Utc>,
    bot_id: Uuid,
    thread_id: Uuid,
    run_id: Uuid,
    state: Arc<Mutex<ProcessState>>,
    /// Signalling the supervisor is how a process is stopped. The supervisor
    /// owns the child, so aborting it on runtime shutdown drops the child and
    /// `kill_on_drop` prevents an orphan.
    stop_tx: tokio::sync::mpsc::Sender<()>,
}

impl Entry {
    fn snapshot(&self) -> BackgroundSnapshot {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        BackgroundSnapshot {
            process_id: self.id.to_string(),
            command: self.command.clone(),
            cwd: self.cwd.clone(),
            pid: self.pid,
            status: state.status.clone(),
            exit_code: state.exit_code,
            output: state.output.clone(),
            started_at: self.started_at.to_rfc3339(),
            updated_at: state.updated_at.to_rfc3339(),
            pattern_matched: state.pattern_matched,
            bot_id: self.bot_id.to_string(),
            thread_id: self.thread_id.to_string(),
            run_id: self.run_id.to_string(),
        }
    }
}

fn registry() -> &'static Mutex<HashMap<Uuid, Arc<Entry>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<Uuid, Arc<Entry>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start a command that has already been configured with cwd, env, and args.
pub fn start(
    mut command: tokio::process::Command,
    label: String,
    cwd: String,
    bot_id: Uuid,
    thread_id: Uuid,
    run_id: Uuid,
) -> Result<BackgroundSnapshot, String> {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start the background task: {error}"))?;
    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "The background task has no stdout pipe.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "The background task has no stderr pipe.".to_string())?;

    let state = Arc::new(Mutex::new(ProcessState {
        status: "running".to_string(),
        exit_code: None,
        output: String::new(),
        pattern: None,
        pattern_matched: false,
        stop_requested: false,
        updated_at: Utc::now(),
    }));
    let (stop_tx, stop_rx) = tokio::sync::mpsc::channel::<()>(1);
    let entry = Arc::new(Entry {
        id: Uuid::new_v4(),
        command: label,
        cwd,
        pid,
        started_at: Utc::now(),
        bot_id,
        thread_id,
        run_id,
        state: Arc::clone(&state),
        stop_tx,
    });
    {
        let mut map = registry().lock().unwrap_or_else(|error| error.into_inner());
        prune_locked(&mut map);
        map.insert(entry.id, Arc::clone(&entry));
    }
    spawn_reader(stdout, Arc::clone(&state));
    spawn_reader(stderr, Arc::clone(&state));
    spawn_supervisor(child, state, stop_rx);
    Ok(entry.snapshot())
}

/// Own the child for its whole lifetime. A stop signal or runtime shutdown
/// drops the task and, through `kill_on_drop`, the process.
fn spawn_supervisor(
    mut child: Child,
    state: Arc<Mutex<ProcessState>>,
    mut stop_rx: tokio::sync::mpsc::Receiver<()>,
) {
    tokio::spawn(async move {
        let status = tokio::select! {
            result = child.wait() => result,
            _ = stop_rx.recv() => {
                let _ = child.start_kill();
                child.wait().await
            }
        };
        let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
        match status {
            Ok(status) => {
                if state.stop_requested {
                    state.status = "stopped".to_string();
                } else if status.success() {
                    state.status = "completed".to_string();
                } else {
                    state.status = "failed".to_string();
                }
                state.exit_code = status.code();
            }
            Err(error) => {
                state.status = "failed".to_string();
                state.output.push_str(&format!(
                    "\nCould not read the background task status: {error}"
                ));
            }
        }
        state.updated_at = Utc::now();
    });
}

fn spawn_reader<R>(mut reader: R, state: Arc<Mutex<ProcessState>>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) => break,
                Ok(read) => {
                    let text = String::from_utf8_lossy(&buffer[..read]);
                    let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                    state.output.push_str(&text);
                    let count = state.output.chars().count();
                    if count > MAX_OUTPUT_CHARS {
                        state.output = state
                            .output
                            .chars()
                            .skip(count - MAX_OUTPUT_CHARS)
                            .collect();
                    }
                    if let Some(pattern) = state.pattern.as_deref() {
                        if !pattern.is_empty() && state.output.contains(pattern) {
                            state.pattern_matched = true;
                        }
                    }
                    state.updated_at = Utc::now();
                }
                Err(_) => break,
            }
        }
    });
}

/// Wait for exit, a matching output pattern, or the timeout.
pub async fn wait(
    id: Uuid,
    timeout_secs: u64,
    pattern: Option<&str>,
) -> Result<WaitOutcome, String> {
    let entry = lookup(id)?;
    {
        let mut state = entry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(pattern) = pattern.map(str::trim).filter(|value| !value.is_empty()) {
            state.pattern = Some(pattern.to_string());
            state.pattern_matched = state.output.contains(pattern);
        }
    }
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs.clamp(1, 3_600));
    loop {
        let (matched, running) = {
            let state = entry
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            (state.pattern_matched, state.status == "running")
        };
        if matched {
            return Ok(WaitOutcome {
                snapshot: entry.snapshot(),
                matched_pattern: true,
                timed_out: false,
            });
        }
        if !running {
            // Give the pipe readers a short moment to drain the final bytes.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            return Ok(WaitOutcome {
                snapshot: entry.snapshot(),
                matched_pattern: false,
                timed_out: false,
            });
        }
        if std::time::Instant::now() >= deadline {
            return Ok(WaitOutcome {
                snapshot: entry.snapshot(),
                matched_pattern: false,
                timed_out: true,
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Stop a running process and wait briefly for it to be reaped.
pub async fn stop(id: Uuid) -> Result<BackgroundSnapshot, String> {
    let entry = lookup(id)?;
    let running = entry
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .status
        == "running";
    if !running {
        return Ok(entry.snapshot());
    }
    {
        let mut state = entry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.stop_requested = true;
        state.status = "stopped".to_string();
        state.updated_at = Utc::now();
    }
    let _ = entry.stop_tx.send(()).await;
    // The supervisor normally reports within one poll interval.
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let running = entry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .status
            == "running";
        if !running {
            break;
        }
    }
    Ok(entry.snapshot())
}

pub fn snapshot(id: Uuid) -> Result<BackgroundSnapshot, String> {
    Ok(lookup(id)?.snapshot())
}

/// List live and recently finished processes for one conversation.
pub fn list(thread_id: Option<Uuid>, bot_id: Option<Uuid>) -> Vec<BackgroundSnapshot> {
    let mut map = registry().lock().unwrap_or_else(|error| error.into_inner());
    prune_locked(&mut map);
    let mut entries: Vec<Arc<Entry>> = map
        .values()
        .filter(|entry| thread_id.is_none_or(|id| entry.thread_id == id))
        .filter(|entry| bot_id.is_none_or(|id| entry.bot_id == id))
        .cloned()
        .collect();
    entries.sort_by_key(|entry| entry.started_at);
    entries.iter().map(|entry| entry.snapshot()).collect()
}

fn lookup(id: Uuid) -> Result<Arc<Entry>, String> {
    let mut map = registry().lock().unwrap_or_else(|error| error.into_inner());
    prune_locked(&mut map);
    map.get(&id)
        .cloned()
        .ok_or_else(|| format!("Background process {id} was not found (it may have expired)."))
}

fn prune_locked(map: &mut HashMap<Uuid, Arc<Entry>>) {
    let cutoff = Utc::now() - chrono::Duration::seconds(FINISHED_RETENTION_SECS);
    map.retain(|_, entry| {
        let state = entry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.status == "running" || state.updated_at >= cutoff
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_direct_process_is_reaped_and_reported() {
        let mut command = tokio::process::Command::new("sh");
        command.args(["-c", "exit 0"]);
        let started = start(
            command,
            "exit".to_string(),
            "/tmp".to_string(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("start");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let snapshot = snapshot(Uuid::parse_str(&started.process_id).unwrap()).expect("snapshot");
        assert_eq!(snapshot.status, "completed");
        assert_eq!(snapshot.exit_code, Some(0));
    }
}

#[cfg(test)]
pub fn clear() {
    let mut map = registry().lock().unwrap_or_else(|error| error.into_inner());
    map.clear();
}
