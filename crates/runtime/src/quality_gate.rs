//! Deterministic verification for office completion.
//!
//! The lead's completion review is a model call. A quality gate is evidence it
//! cannot overrule: a known project command (`cargo test`, `npm test`,
//! `npm run build`) must exit 0 before the office may call the product done.
//!
//! Only fixed commands are executed. There is no shell string, no user-supplied
//! command, and no environment expansion. The project's own scripts can still
//! run project code, which is exactly what a build or test gate is for.

use ravenbot_core::GoalGate;
use serde::Serialize;
use std::path::Path;
use std::time::{Duration, Instant};

/// Running every gate in a large repo can be expensive. Two are enough to
/// distinguish "it builds" from "it works" without turning completion into a
/// second full CI run.
pub const MAX_GATES: usize = 2;
pub const MAX_CUSTOM_GATES: usize = 4;
pub const MAX_TOTAL_GATES: usize = 6;
pub const OUTPUT_TAIL_CHARS: usize = 4_000;

#[derive(Debug, Clone, Serialize)]
pub struct GateCommand {
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateResult {
    pub command: String,
    pub cwd: String,
    pub exit_code: Option<i32>,
    pub passed: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub output_tail: String,
    /// Optional gates are reported but cannot veto completion.
    pub required: bool,
}

/// Detect the project's verification commands from its manifest.
///
/// Detection is deliberately conservative: only well-known manifests and only
/// the standard `test`, `build`, and `check` script names.
pub fn detect(root: &Path) -> Vec<GateCommand> {
    let mut gates = Vec::new();

    if root.join("Cargo.toml").is_file() {
        gates.push(GateCommand {
            label: "cargo test --quiet".to_string(),
            program: "cargo".to_string(),
            args: vec!["test".to_string(), "--quiet".to_string()],
            env: Vec::new(),
        });
    }

    if root.join("package.json").is_file() {
        if let Some(node_gates) = node_gates(root) {
            gates.extend(node_gates);
        }
    }

    if root.join("go.mod").is_file() && gates.len() < MAX_GATES {
        gates.push(GateCommand {
            label: "go test ./...".to_string(),
            program: "go".to_string(),
            args: vec!["test".to_string(), "./...".to_string()],
            env: Vec::new(),
        });
    }

    if let Some(python) = python_gate(root) {
        if gates.len() < MAX_GATES {
            gates.push(python);
        }
    }

    gates.truncate(MAX_GATES);
    gates
}

/// Detect a Python project's standard test command.
///
/// `pytest` is chosen only when the project actually configures or depends on
/// it; otherwise `unittest` discovery is the zero-dependency baseline. The
/// test command is scoped to `tests/` when that directory exists, so a scratch
/// copy elsewhere in the workspace is not collected by accident.
fn python_gate(root: &Path) -> Option<GateCommand> {
    let tests = root.join("tests");
    let has_tests = tests.is_dir() && contains_python_test(&tests);
    let pytest_configured = [
        root.join("pytest.ini"),
        root.join("conftest.py"),
        tests.join("conftest.py"),
        root.join("tox.ini"),
        root.join("setup.cfg"),
        root.join("pyproject.toml"),
        root.join("requirements.txt"),
        root.join("requirements-dev.txt"),
        root.join("dev-requirements.txt"),
    ]
    .iter()
    .any(|path| path.is_file() && file_mentions_pytest(path));

    if pytest_configured && (has_tests || root.join("pyproject.toml").is_file()) {
        let mut args = vec!["-m".to_string(), "pytest".to_string(), "-q".to_string()];
        if has_tests {
            args.push("tests".to_string());
        }
        return Some(GateCommand {
            label: if has_tests {
                "python3 -m pytest -q tests".to_string()
            } else {
                "python3 -m pytest -q".to_string()
            },
            program: "python3".to_string(),
            args,
            env: vec![("PYTHONDONTWRITEBYTECODE".to_string(), "1".to_string())],
        });
    }

    if has_tests {
        return Some(GateCommand {
            label: "python3 -m unittest discover -s tests -q".to_string(),
            program: "python3".to_string(),
            args: vec![
                "-m".to_string(),
                "unittest".to_string(),
                "discover".to_string(),
                "-s".to_string(),
                "tests".to_string(),
                "-q".to_string(),
            ],
            env: vec![("PYTHONDONTWRITEBYTECODE".to_string(), "1".to_string())],
        });
    }

    None
}

fn contains_python_test(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if contains_python_test(&path) {
                return true;
            }
            continue;
        }
        let is_python = path.extension().and_then(|value| value.to_str()) == Some("py");
        if !is_python {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if name.starts_with("test") || name.ends_with("_test.py") {
            return true;
        }
    }
    false
}

fn file_mentions_pytest(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|text| text.to_ascii_lowercase().contains("pytest"))
        .unwrap_or(false)
}

fn node_gates(root: &Path) -> Option<Vec<GateCommand>> {
    let text = std::fs::read_to_string(root.join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let scripts = json.get("scripts")?.as_object()?;
    let script = |name: &str| {
        scripts
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let env = vec![
        ("CI".to_string(), "1".to_string()),
        ("NO_COLOR".to_string(), "1".to_string()),
        ("FORCE_COLOR".to_string(), "0".to_string()),
    ];
    let mut gates = Vec::new();

    // The default npm placeholder is not a test. Treat it as absent so a new
    // project is told to add tests rather than told its placeholder failed.
    let test =
        script("test").filter(|value| !value.to_ascii_lowercase().contains("no test specified"));
    if let Some(_test) = test {
        gates.push(GateCommand {
            label: "npm test".to_string(),
            program: "npm".to_string(),
            args: vec!["test".to_string()],
            env: env.clone(),
        });
    }

    let second = script("build")
        .map(|_| {
            (
                "npm run build".to_string(),
                vec!["run".to_string(), "build".to_string()],
            )
        })
        .or_else(|| {
            script("check").map(|_| {
                (
                    "npm run check".to_string(),
                    vec!["run".to_string(), "check".to_string()],
                )
            })
        });
    if let Some((label, args)) = second {
        gates.push(GateCommand {
            label,
            program: "npm".to_string(),
            args,
            env,
        });
    }

    if gates.is_empty() {
        None
    } else {
        Some(gates)
    }
}

/// Run every detected gate in `root`. A failure is a result, not an error.
pub async fn run_workspace_gates(root: &Path, timeout: Duration) -> Vec<GateResult> {
    run_commands(root, detect(root), timeout).await
}

async fn run_commands(root: &Path, gates: Vec<GateCommand>, timeout: Duration) -> Vec<GateResult> {
    let mut results = Vec::new();
    for gate in gates {
        let started = Instant::now();
        let mut command = tokio::process::Command::new(&gate.program);
        command
            .args(&gate.args)
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // A timed-out child must not outlive the check that started it.
            .kill_on_drop(true);
        for (key, value) in &gate.env {
            command.env(key, value);
        }
        let output = tokio::time::timeout(timeout, command.output()).await;
        let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        match output {
            Ok(Ok(output)) => {
                let mut text = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !text.is_empty() && !stderr.is_empty() {
                    text.push('\n');
                }
                text.push_str(&stderr);
                results.push(GateResult {
                    command: gate.label,
                    cwd: root.display().to_string(),
                    exit_code: output.status.code(),
                    passed: output.status.success(),
                    timed_out: false,
                    duration_ms,
                    output_tail: tail(&text, OUTPUT_TAIL_CHARS),
                    required: true,
                });
            }
            Ok(Err(error)) => results.push(GateResult {
                command: gate.label,
                cwd: root.display().to_string(),
                exit_code: None,
                passed: false,
                timed_out: false,
                duration_ms,
                output_tail: format!("Could not start the command: {error}"),
                required: true,
            }),
            Err(_) => results.push(GateResult {
                command: gate.label,
                cwd: root.display().to_string(),
                exit_code: None,
                passed: false,
                timed_out: true,
                duration_ms,
                output_tail: format!(
                    "The verification did not finish within {} seconds.",
                    timeout.as_secs()
                ),
                required: true,
            }),
        }
    }
    results
}

/// Run the contract's custom gates, then the detected manifest gates.
///
/// Custom gates run through a shell so a user can write `npm test -- --run`, but
/// the command is checked by the same safety policy as `shell_exec` first. A
/// destructive command is refused before it can run unattended.
pub async fn run_workspace_gates_with_contract(
    root: &Path,
    custom: &[GoalGate],
    roots: &[std::path::PathBuf],
    timeout: Duration,
) -> Vec<GateResult> {
    let mut results = run_goal_gates(root, custom, roots, timeout).await;
    let remaining = MAX_TOTAL_GATES.saturating_sub(results.len());
    if remaining > 0 {
        results.extend(
            run_commands(
                root,
                detect(root).into_iter().take(remaining).collect(),
                timeout,
            )
            .await,
        );
    }
    results
}

async fn run_goal_gates(
    root: &Path,
    gates: &[GoalGate],
    roots: &[std::path::PathBuf],
    timeout: Duration,
) -> Vec<GateResult> {
    let mut results = Vec::new();
    for gate in gates.iter().take(MAX_CUSTOM_GATES) {
        let command = gate.command.trim();
        if command.is_empty() {
            continue;
        }
        let cwd = match gate
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            None => root.to_path_buf(),
            Some(relative) => {
                let path = Path::new(relative);
                if path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
                {
                    results.push(GateResult {
                        command: command.to_string(),
                        cwd: path.display().to_string(),
                        exit_code: None,
                        passed: false,
                        timed_out: false,
                        duration_ms: 0,
                        output_tail: "The gate working directory is outside the office workspace."
                            .to_string(),
                        required: gate.required,
                    });
                    continue;
                }
                if path.is_absolute() && !path.starts_with(root) {
                    results.push(GateResult {
                        command: command.to_string(),
                        cwd: path.display().to_string(),
                        exit_code: None,
                        passed: false,
                        timed_out: false,
                        duration_ms: 0,
                        output_tail: "The gate working directory is outside the office workspace."
                            .to_string(),
                        required: gate.required,
                    });
                    continue;
                }
                let joined = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    root.join(path)
                };
                if !joined.starts_with(root) {
                    results.push(GateResult {
                        command: command.to_string(),
                        cwd: joined.display().to_string(),
                        exit_code: None,
                        passed: false,
                        timed_out: false,
                        duration_ms: 0,
                        output_tail: "The gate working directory is outside the office workspace."
                            .to_string(),
                        required: gate.required,
                    });
                    continue;
                }
                joined
            }
        };
        if let Some(reason) = ravenbot_skills::safety::dangerous_command_reason(command, roots) {
            results.push(GateResult {
                command: command.to_string(),
                cwd: cwd.display().to_string(),
                exit_code: None,
                passed: false,
                timed_out: false,
                duration_ms: 0,
                output_tail: format!("The verification command was blocked: {reason}"),
                required: gate.required,
            });
            continue;
        }
        let timeout = Duration::from_secs(
            gate.timeout_secs
                .clamp(1, 3_600)
                .min(timeout.as_secs().max(1)),
        );
        let started = Instant::now();
        #[cfg(unix)]
        let mut process = {
            let mut process = tokio::process::Command::new("sh");
            process.args(["-c", command]);
            process
        };
        #[cfg(windows)]
        let mut process = {
            let mut process = tokio::process::Command::new("cmd");
            process.args(["/C", command]);
            process
        };
        process
            .current_dir(&cwd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env("CI", "1")
            .env("NO_COLOR", "1")
            .kill_on_drop(true);
        let output = tokio::time::timeout(timeout, process.output()).await;
        let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        match output {
            Ok(Ok(output)) => {
                let mut text = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !text.is_empty() && !stderr.is_empty() {
                    text.push('\n');
                }
                text.push_str(&stderr);
                results.push(GateResult {
                    command: command.to_string(),
                    cwd: cwd.display().to_string(),
                    exit_code: output.status.code(),
                    passed: output.status.success(),
                    timed_out: false,
                    duration_ms,
                    output_tail: tail(&text, OUTPUT_TAIL_CHARS),
                    required: gate.required,
                });
            }
            Ok(Err(error)) => results.push(GateResult {
                command: command.to_string(),
                cwd: cwd.display().to_string(),
                exit_code: None,
                passed: false,
                timed_out: false,
                duration_ms,
                output_tail: format!("Could not start the command: {error}"),
                required: gate.required,
            }),
            Err(_) => results.push(GateResult {
                command: command.to_string(),
                cwd: cwd.display().to_string(),
                exit_code: None,
                passed: false,
                timed_out: true,
                duration_ms,
                output_tail: format!(
                    "The verification did not finish within {} seconds.",
                    timeout.as_secs()
                ),
                required: gate.required,
            }),
        }
    }
    results
}

/// The next continuation instruction when a gate is red. `None` means every
/// required gate passed (or none were detected).
pub fn gate_failure(gates: &[GateResult]) -> Option<String> {
    let failed: Vec<&GateResult> = gates
        .iter()
        .filter(|gate| !gate.passed && gate.required)
        .collect();
    if failed.is_empty() {
        return None;
    }
    let names = failed
        .iter()
        .map(|gate| format!("`{}`", gate.command))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!(
        "Fix the failing verification before the product can be marked complete. Failed: {names}.\n"
    );
    for gate in failed {
        out.push_str(&format!(
            "\nCommand: {}\nDirectory: {}\n",
            gate.command, gate.cwd
        ));
        out.push_str(if gate.timed_out {
            "Result: timed out\n"
        } else {
            "Result: non-zero exit\n"
        });
        if !gate.output_tail.trim().is_empty() {
            out.push_str("Output tail:\n");
            out.push_str(gate.output_tail.trim());
            out.push('\n');
        }
    }
    Some(tail(&out, 6_000))
}

/// A compact report for the office conversation.
pub fn gate_report_markdown(gates: &[GateResult]) -> String {
    if gates.is_empty() {
        return "**Quality gates:** no recognized test or build command in the workspace."
            .to_string();
    }
    let mut out = String::from("**Quality gates**\n");
    for gate in gates {
        let icon = if gate.passed { "✅" } else { "❌" };
        let seconds = gate.duration_ms as f64 / 1000.0;
        out.push_str(&format!(
            "- {icon} `{}` — {:.1}s{}{}\n",
            gate.command,
            seconds,
            if gate.timed_out { " (timed out)" } else { "" },
            if gate.required { "" } else { " (optional)" }
        ));
    }
    if let Some(failure) = gate_failure(gates) {
        out.push_str("\n```text\n");
        out.push_str(&failure);
        out.push_str("\n```\n");
    } else {
        out.push_str("\nThe workspace verification passed.\n");
    }
    out
}

fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    text.chars().skip(count - max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("ravenbot-gate-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn detects_cargo_and_node_scripts_without_running_the_npm_placeholder() {
        let cargo = temp_dir("cargo");
        std::fs::write(cargo.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let gates = detect(&cargo);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].program, "cargo");
        assert_eq!(gates[0].args, vec!["test", "--quiet"]);

        let node = temp_dir("node");
        std::fs::write(
            node.join("package.json"),
            r#"{"scripts":{"test":"vitest run","build":"vite build","check":"svelte-check"}}"#,
        )
        .unwrap();
        let gates = detect(&node);
        assert_eq!(gates.len(), 2);
        assert_eq!(gates[0].label, "npm test");
        assert_eq!(gates[1].label, "npm run build");

        let placeholder = temp_dir("placeholder");
        std::fs::write(
            placeholder.join("package.json"),
            r#"{"scripts":{"test":"echo \"Error: no test specified\" && exit 1","check":"svelte-check"}}"#,
        )
        .unwrap();
        let gates = detect(&placeholder);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].label, "npm run check");
    }

    #[test]
    fn detects_python_unittest_and_pytest_projects() {
        let unittest_root = temp_dir("python-unittest");
        std::fs::create_dir_all(unittest_root.join("tests")).unwrap();
        std::fs::write(unittest_root.join("tests/test_ok.py"), "import unittest\n").unwrap();
        let gates = detect(&unittest_root);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].label, "python3 -m unittest discover -s tests -q");
        assert_eq!(gates[0].program, "python3");

        let pytest_root = temp_dir("python-pytest");
        std::fs::create_dir_all(pytest_root.join("tests")).unwrap();
        std::fs::write(
            pytest_root.join("tests/test_ok.py"),
            "def test_ok():\n    assert True\n",
        )
        .unwrap();
        std::fs::write(pytest_root.join("pytest.ini"), "[pytest]\n").unwrap();
        let gates = detect(&pytest_root);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].label, "python3 -m pytest -q tests");
        assert_eq!(gates[0].program, "python3");
    }

    #[test]
    fn a_red_gate_produces_actionable_continuation_text() {
        let passed = GateResult {
            command: "cargo test --quiet".into(),
            cwd: "/tmp/app".into(),
            exit_code: Some(0),
            passed: true,
            timed_out: false,
            duration_ms: 12,
            output_tail: String::new(),
            required: true,
        };
        assert!(gate_failure(&[passed]).is_none());

        let failed = GateResult {
            command: "npm test".into(),
            cwd: "/tmp/app".into(),
            exit_code: Some(1),
            passed: false,
            timed_out: false,
            duration_ms: 900,
            output_tail: "FAIL src/app.test.ts".into(),
            required: true,
        };
        let next = gate_failure(&[failed]).expect("failure");
        assert!(next.contains("npm test"));
        assert!(next.contains("FAIL src/app.test.ts"));
        assert!(next.contains("Fix the failing verification"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_gate_reports_exit_code_output_and_timeout() {
        let root = temp_dir("run");
        let ok = run_commands(
            &root,
            vec![GateCommand {
                label: "ok".into(),
                program: "sh".into(),
                args: vec!["-c".into(), "printf 'all good'".into()],
                env: Vec::new(),
            }],
            Duration::from_secs(3),
        )
        .await;
        assert!(ok[0].passed);
        assert_eq!(ok[0].exit_code, Some(0));
        assert_eq!(ok[0].output_tail, "all good");

        let bad = run_commands(
            &root,
            vec![GateCommand {
                label: "bad".into(),
                program: "sh".into(),
                args: vec!["-c".into(), "printf 'broken' >&2; exit 3".into()],
                env: Vec::new(),
            }],
            Duration::from_secs(3),
        )
        .await;
        assert!(!bad[0].passed);
        assert_eq!(bad[0].exit_code, Some(3));
        assert!(bad[0].output_tail.contains("broken"));

        let slow = run_commands(
            &root,
            vec![GateCommand {
                label: "slow".into(),
                program: "sh".into(),
                args: vec!["-c".into(), "sleep 2".into()],
                env: Vec::new(),
            }],
            Duration::from_millis(80),
        )
        .await;
        assert!(slow[0].timed_out);
        assert!(!slow[0].passed);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn custom_gates_are_safety_checked_and_optional_failures_do_not_veto() {
        let root = temp_dir("custom");
        let ok = GoalGate::new("printf custom-ok");
        let mut optional = GoalGate::new("exit 7");
        optional.required = false;
        let blocked = GoalGate::new("rm -rf .");
        let mut outside = GoalGate::new("printf nope");
        outside.cwd = Some("../escape".to_string());

        let allowed = run_workspace_gates_with_contract(
            &root,
            &[ok, optional],
            std::slice::from_ref(&root),
            Duration::from_secs(3),
        )
        .await;
        assert_eq!(allowed.len(), 2);
        assert!(allowed[0].passed, "{allowed:?}");
        assert_eq!(allowed[0].output_tail, "custom-ok");
        assert!(!allowed[1].passed);
        assert!(
            !allowed[1].required,
            "optional gates are reported, not vetoed"
        );
        assert!(gate_failure(&allowed).is_none());

        let refused = run_workspace_gates_with_contract(
            &root,
            &[blocked, outside],
            std::slice::from_ref(&root),
            Duration::from_secs(3),
        )
        .await;
        assert_eq!(refused.len(), 2);
        assert!(!refused[0].passed);
        assert!(refused[0].required);
        assert!(
            refused[0].output_tail.contains("blocked"),
            "{:?}",
            refused[0]
        );
        assert!(!refused[1].passed);
        assert!(refused[1]
            .output_tail
            .contains("outside the office workspace"));

        let next = gate_failure(&refused).expect("required failure");
        assert!(next.contains("rm -rf ."));
    }

    #[test]
    fn long_output_is_tailed_without_splitting_a_character() {
        let text = "x".repeat(20) + &"é".repeat(10);
        let tailed = tail(&text, 10);
        assert_eq!(tailed.chars().count(), 10);
        assert!(tailed.chars().all(|character| character == 'é'));
    }
}
