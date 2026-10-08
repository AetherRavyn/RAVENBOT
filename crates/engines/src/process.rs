//! Process helpers shared by every engine: binary discovery, version probes,
//! and a cancellable line-streaming reader over a spawned child.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::{CancelToken, EngineError};

/// Locate a binary on PATH (or an absolute/relative path that exists).
pub fn find_binary(command: &str) -> Option<PathBuf> {
    let candidate = Path::new(command);
    if candidate.is_absolute() || command.contains(std::path::MAIN_SEPARATOR) {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .map(|s| s.to_lowercase())
            .collect()
    } else {
        Vec::new()
    };

    for dir in std::env::split_paths(&path_var) {
        let direct = dir.join(command);
        if direct.is_file() {
            return Some(direct);
        }
        if cfg!(windows) {
            for ext in &exts {
                let with_ext = dir.join(format!("{}{}", command, ext));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }
    None
}

/// Run `command args` briefly and return the first non-empty output line.
pub async fn version_of(command: &str, args: &[&str]) -> Option<String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        Command::new(command)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    let text = if output.stdout.iter().any(|b| !b.is_ascii_whitespace()) {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        String::from_utf8_lossy(&output.stderr).to_string()
    };
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Spawn a child with the inherited environment plus `env`, stdin piped so the
/// caller can write the prompt, stdout/stderr piped, and `kill_on_drop` so a
/// cancelled or panicked turn can never leak an agent process.
pub fn spawn_with_stdin(
    command: &str,
    args: &[String],
    cwd: Option<&str>,
    env: &std::collections::HashMap<String, String>,
) -> Result<Child, EngineError> {
    let build = || {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd
    };

    // ETXTBSY ("Text file busy", os error 26) is a fork/exec race that hits
    // just-written or just-replaced executables (a wrapper script the user
    // generated). It is transient: the writer's descriptor clears momentarily.
    // Retry briefly rather than failing the whole turn.
    const ETXTBSY: i32 = 26;
    let mut last_err = None;
    for attempt in 0..10 {
        match build().spawn() {
            Ok(child) => return Ok(child),
            Err(e) if e.raw_os_error() == Some(ETXTBSY) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
            }
            Err(e) => {
                return Err(EngineError::spawn(format!(
                    "failed to spawn `{}`: {}",
                    command, e
                )))
            }
        }
    }
    Err(EngineError::spawn(format!(
        "failed to spawn `{}`: {}",
        command,
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "text file busy".to_string())
    )))
}

/// Read one line at a time from a child's stdout, invoking `on_line` for each
/// non-empty line. Returns when stdout closes, the child exits, or `cancel` is
/// requested. The child is killed on cancellation.
pub async fn stream_lines<F>(
    mut child: Child,
    mut on_line: F,
    cancel: &CancelToken,
) -> Result<(), EngineError>
where
    F: FnMut(&str),
{
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EngineError::spawn("child has no stdout"))?;
    let stderr = child.stderr.take();
    // Drain stderr on a side task so a chatty CLI can't fill the pipe buffer
    // and deadlock the turn.
    let stderr_task = stderr.map(|stderr| {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut tail = String::new();
            while let Ok(Some(line)) = reader.next_line().await {
                if !line.trim().is_empty() {
                    tracing::debug!(target: "ravenbot_engines", "engine stderr: {}", line);
                    tail.push_str(&line);
                    tail.push('\n');
                    if tail.len() > 8192 {
                        let cut = tail.len() - 4096;
                        tail.drain(..cut);
                    }
                }
            }
            tail
        })
    });

    let mut reader = BufReader::new(stdout).lines();
    let status: Option<std::process::ExitStatus> = loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(EngineError::cancelled());
            }
            line = reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if !line.trim().is_empty() {
                            on_line(&line);
                        }
                    }
                    Ok(None) => break child.wait().await.ok(),
                    Err(e) => {
                        let _ = child.kill().await;
                        return Err(EngineError::protocol(format!("failed to read engine stdout: {}", e)));
                    }
                }
            }
        }
    };

    let stderr_tail = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };

    match status {
        Some(s) if s.success() => Ok(()),
        Some(s) => Err(EngineError::upstream(format!(
            "engine exited with {}{}",
            s,
            if stderr_tail.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", stderr_tail.trim())
            }
        ))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_sh_on_unix() {
        if cfg!(unix) {
            assert!(find_binary("sh").is_some());
        }
        assert!(find_binary("definitely-not-a-real-binary-xyz").is_none());
    }

    #[tokio::test]
    async fn streams_child_stdout_and_reports_success() {
        let child = spawn_with_stdin("sh", &["-c".into(), "printf 'a\\nb\\n'".into()], None, &Default::default())
            .expect("spawn sh");
        let mut lines = Vec::new();
        let cancel = CancelToken::new();
        stream_lines(child, |l| lines.push(l.to_string()), &cancel)
            .await
            .expect("stream");
        assert_eq!(lines, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn cancellation_kills_the_child() {
        let child = spawn_with_stdin("sh", &["-c".into(), "sleep 30".into()], None, &Default::default())
            .expect("spawn");
        let cancel = CancelToken::new();
        cancel.cancel();
        let err = stream_lines(child, |_| {}, &cancel).await.unwrap_err();
        assert_eq!(err.code, crate::EngineErrorCode::Cancelled);
    }
}
