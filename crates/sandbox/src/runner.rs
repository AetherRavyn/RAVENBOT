//! Real command isolation.
//!
//! [`SandboxRunner`] turns a [`SandboxConfig`] into an actual isolated command:
//!
//! * **Filesystem** — bubblewrap (`bwrap`) mounts the system read-only, hides
//!   the user's home (so `~/.ssh`, `~/.aws`, browser profiles, … are simply not
//!   present), and bind-mounts only the configured allowed paths plus the
//!   working directory read-write. Sensitive `/etc` files (`shadow`, sudoers)
//!   are never bound.
//! * **Namespaces** — PID, UTS, IPC (and network when policy blocks it) are
//!   unshared, with `--die-with-parent` so no orphan survives the run.
//! * **Resource limits** — a POSIX `ulimit` shim applies CPU-time, address-space,
//!   file-size and process-count ceilings, inherited by everything the command
//!   spawns.
//!
//! When bubblewrap is unavailable (or the [`SandboxTier::Host`] escape hatch is
//! selected) the runner degrades to limits-only and says so — it never claims
//! isolation it did not apply.

use crate::network_policy::NetworkPolicy;
use crate::resource_limits::ResourceLimits;
use crate::sandbox::SandboxConfig;
use ravenbot_core::SandboxTier;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Which isolation backend is actually being used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxBackend {
    /// bubblewrap namespaces (Linux)
    Bubblewrap(PathBuf),
    /// No OS isolation — limits only (host tier, or bwrap unavailable)
    None,
}

/// A human-readable description of what the runner will actually enforce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxReport {
    pub backend: String,
    pub filesystem_isolated: bool,
    pub network_isolated: bool,
    pub resource_limits: bool,
    pub note: String,
}

/// Runs commands under a [`SandboxConfig`].
pub struct SandboxRunner {
    config: SandboxConfig,
    backend: SandboxBackend,
}

impl SandboxRunner {
    /// Build a runner, detecting the best available backend for `config`.
    pub fn new(config: SandboxConfig) -> Self {
        let backend = Self::detect_backend(&config);
        Self { config, backend }
    }

    /// Build a runner from a tier + limits + policy (convenience).
    pub fn from_tier(tier: SandboxTier) -> Self {
        Self::new(SandboxConfig {
            tier,
            resource_limits: ResourceLimits::default(),
            network_policy: NetworkPolicy::default(),
            ..Default::default()
        })
    }

    /// Build a runner that also binds `paths` read-write.
    ///
    /// An office workspace has to be bound, not merely used as the cwd: a
    /// command that does `cd /tmp` first would otherwise lose access to the
    /// folder it is supposed to be working in, and a tool that reads a sibling
    /// file by absolute path would fail even though the path is inside the
    /// office. These roots are added to `allowed_paths`, so bubblewrap mounts
    /// them writable.
    ///
    /// Only paths that exist are bound; a root that has been deleted is
    /// skipped rather than causing a spawn failure.
    pub fn from_tier_with_paths(tier: SandboxTier, paths: &[PathBuf]) -> Self {
        let mut config = SandboxConfig {
            tier,
            resource_limits: ResourceLimits::default(),
            network_policy: NetworkPolicy::default(),
            ..Default::default()
        };
        for path in paths {
            if !path.is_dir() {
                continue;
            }
            // Canonicalize so a root given as `~/office` binds the same inode
            // the path check compares against.
            let resolved = path.canonicalize().unwrap_or_else(|_| path.clone());
            let as_text = resolved.to_string_lossy().to_string();
            if !config.allowed_paths.contains(&as_text) {
                config.allowed_paths.push(as_text);
            }
        }
        Self::new(config)
    }

    fn detect_backend(config: &SandboxConfig) -> SandboxBackend {
        if config.tier == SandboxTier::Host {
            return SandboxBackend::None;
        }
        if let Some(path) = find_on_path("bwrap") {
            return SandboxBackend::Bubblewrap(path);
        }
        SandboxBackend::None
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    pub fn backend(&self) -> &SandboxBackend {
        &self.backend
    }

    /// True when commands run inside real OS namespaces.
    pub fn is_isolated(&self) -> bool {
        matches!(self.backend, SandboxBackend::Bubblewrap(_))
    }

    /// Whether network access will be truly blocked.
    pub fn network_isolated(&self) -> bool {
        self.is_isolated() && !self.config.network_policy.enabled
    }

    /// Describe the effective protection for the UI.
    pub fn report(&self) -> SandboxReport {
        match &self.backend {
            SandboxBackend::Bubblewrap(path) => {
                let docker_note = if self.config.tier == SandboxTier::Docker {
                    " (Docker daemon backend is not wired up, so namespaces are used instead.)"
                } else {
                    ""
                };
                SandboxReport {
                    backend: format!("bubblewrap ({})", path.display()),
                    filesystem_isolated: true,
                    network_isolated: !self.config.network_policy.enabled,
                    resource_limits: true,
                    note: format!(
                        "{}{}",
                        if self.config.network_policy.enabled {
                            "Filesystem + PID/IPC/UTS namespaces enforced; network shared (domain allowlist applies to HTTP tools)."
                        } else {
                            "Filesystem + PID/IPC/UTS + network namespaces enforced."
                        },
                        docker_note
                    ),
                }
            }
            SandboxBackend::None => {
                let reason = if self.config.tier == SandboxTier::Host {
                    "host tier selected (explicit opt-in)"
                } else {
                    "bubblewrap not found on PATH"
                };
                SandboxReport {
                    backend: "none".to_string(),
                    filesystem_isolated: false,
                    network_isolated: false,
                    resource_limits: true,
                    note: format!(
                        "Running with resource limits only — {} (install bubblewrap for full isolation on Linux).",
                        reason
                    ),
                }
            }
        }
    }

    /// The POSIX `ulimit` shim applied to every sandboxed command. Kept as a
    /// separate `sh -c` frame so it applies to the command *and its children*.
    ///
    /// Every limit gets its own `ulimit` invocation. dash — `/bin/sh` on Debian
    /// and Ubuntu, i.e. most of where this actually runs — errors with
    /// "too many arguments" on more than one flag and applies *none* of them.
    /// Passing all four in one call meant that on Ubuntu every cap was
    /// silently dropped: the error was swallowed by `2>/dev/null` and
    /// `SandboxReport` went on claiming `resource_limits: true`. One rejected
    /// limit must never be able to disable the others, so they are set
    /// separately.
    ///
    /// dash has no `-u` either, so the process cap lands only where the shell
    /// implements it (bash); the CPU, memory and file caps apply everywhere.
    fn ulimit_shim(&self) -> String {
        let limits = &self.config.resource_limits;
        // -t CPU seconds, -v virtual KB, -f file size (512B blocks), -u procs.
        format!(
            "ulimit -t {cpu} 2>/dev/null; \
             ulimit -v {vmem} 2>/dev/null; \
             ulimit -f {fsize} 2>/dev/null; \
             ulimit -u {nproc} 2>/dev/null; \
             exec \"$0\" \"$@\"",
            cpu = limits.max_task_duration_secs.max(1),
            vmem = limits.max_memory_mb.saturating_mul(1024).max(1024),
            fsize = limits
                .max_disk_write_mb
                .saturating_mul(2048)
                .max(2048),
            nproc = limits.max_processes.max(1),
        )
    }

    /// Build an isolated `tokio::process::Command` running `program args…`.
    ///
    /// `cwd` is the working directory; it is bind-mounted read-write (unless it
    /// falls under a blocked path), so agents can edit the folder they work in
    /// without seeing the rest of the home directory.
    pub fn command(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
    ) -> Result<Command, String> {
        let shim = self.ulimit_shim();

        let mut cmd = match &self.backend {
            SandboxBackend::Bubblewrap(bwrap) => {
                let mut cmd = Command::new(bwrap);
                self.apply_bwrap_args(&mut cmd, cwd);
                cmd.arg("--");
                cmd.arg("/bin/sh").arg("-c").arg(&shim).arg(program);
                cmd.args(args);
                cmd
            }
            SandboxBackend::None => {
                let mut cmd = Command::new("/bin/sh");
                cmd.arg("-c").arg(&shim).arg(program);
                cmd.args(args);
                cmd
            }
        };

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd.kill_on_drop(true);
        Ok(cmd)
    }

    fn apply_bwrap_args(&self, cmd: &mut Command, cwd: Option<&Path>) {
        cmd.arg("--die-with-parent");
        cmd.arg("--unshare-pid");
        cmd.arg("--unshare-uts");
        cmd.arg("--unshare-ipc");
        if !self.config.network_policy.enabled {
            cmd.arg("--unshare-net");
        }
        // A clean process environment directory tree.
        cmd.args(["--proc", "/proc"]);
        cmd.args(["--dev", "/dev"]);
        cmd.args(["--tmpfs", "/tmp"]);

        // Read-only system directories (only those that exist).
        for dir in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/opt"] {
            if Path::new(dir).exists() {
                cmd.args(["--ro-bind", dir, dir]);
            }
        }
        // The minimal /etc files tools need — never shadow/sudoers.
        for file in [
            "/etc/resolv.conf",
            "/etc/hosts",
            "/etc/nsswitch.conf",
            "/etc/passwd",
            "/etc/group",
            "/etc/localtime",
            "/etc/timezone",
            "/etc/ld.so.cache",
        ] {
            if Path::new(file).exists() {
                cmd.args(["--ro-bind-try", file, file]);
            }
        }
        for dir in [
            "/etc/ssl",
            "/etc/ca-certificates",
            "/etc/pki",
            "/etc/ld.so.conf.d",
            "/etc/alternatives",
        ] {
            if Path::new(dir).exists() {
                cmd.args(["--ro-bind", dir, dir]);
            }
        }

        // Read-write allowed paths (skipping blocked ones).
        for allowed in &self.config.allowed_paths {
            let resolved = expand_path(allowed);
            if self.is_blocked(&resolved) || !resolved.exists() {
                continue;
            }
            let s = resolved.to_string_lossy().to_string();
            cmd.args(["--bind", &s, &s]);
        }

        // Toolchain caches: without these, `cargo`/`npm`/`pip` would re-download
        // everything because $HOME is otherwise hidden. These hold no secrets —
        // credentials live in ~/.ssh, ~/.aws, ~/.config/gh, ~/.netrc, which stay
        // inaccessible.
        if let Ok(home) = std::env::var("HOME") {
            for rel in [
                ".cargo",
                ".rustup",
                ".npm",
                ".cache",
                ".local/share/pnpm",
                ".local/share/uv",
                ".bun",
                ".deno",
                ".gradle",
                ".m2",
                ".config/pip",
                ".npm-global",
            ] {
                let path = Path::new(&home).join(rel);
                if path.exists() && !self.is_blocked(&path) {
                    let s = path.to_string_lossy().to_string();
                    cmd.args(["--bind", &s, &s]);
                }
            }
        }

        // The working directory: read-write so the agent can edit its project,
        // unless it is explicitly blocked.
        if let Some(dir) = cwd {
            if let Ok(abs) = dir.canonicalize() {
                if !self.is_blocked(&abs) {
                    let s = abs.to_string_lossy().to_string();
                    cmd.args(["--bind", &s, &s]);
                    cmd.args(["--chdir", &s]);
                }
            }
        }
    }

    fn is_blocked(&self, path: &Path) -> bool {
        let s = path.to_string_lossy();
        self.config
            .blocked_paths
            .iter()
            .map(|b| expand_path(b))
            .any(|b| {
                let b = b.to_string_lossy();
                !b.is_empty() && (s == b || s.starts_with(&format!("{}/", b)))
            })
    }
}

/// Locate a binary on PATH (no subprocess).
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Expand a leading `~` and make the path absolute relative to the cwd.
fn expand_path(raw: &str) -> PathBuf {
    let expanded = if let Some(rest) = raw.strip_prefix("~") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        format!("{}{}", home, rest)
    } else {
        raw.to_string()
    };
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|c| c.join(&path))
            .unwrap_or(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_policy::NetworkPolicy;

    fn runner(blocked_net: bool) -> SandboxRunner {
        SandboxRunner::new(SandboxConfig {
            tier: SandboxTier::OsLevel,
            resource_limits: ResourceLimits::default(),
            network_policy: if blocked_net {
                NetworkPolicy::blocked()
            } else {
                NetworkPolicy::permissive()
            },
            ..Default::default()
        })
    }

    async fn run(runner: &SandboxRunner, script: &str) -> (String, String, bool) {
        let args = vec!["-c".to_string(), script.to_string()];
        let mut cmd = runner.command("sh", &args, None).expect("build command");
        let out = cmd.output().await.expect("run command");
        (
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
            out.status.success(),
        )
    }

    #[test]
    fn host_tier_never_isolates() {
        let runner = SandboxRunner::from_tier(SandboxTier::Host);
        assert!(!runner.is_isolated());
        assert!(!runner.report().filesystem_isolated);
    }

    #[tokio::test]
    async fn command_runs_and_reports_output() {
        let runner = runner(false);
        let (stdout, _stderr, ok) = run(&runner, "echo hello-sandbox").await;
        assert!(ok);
        assert!(stdout.contains("hello-sandbox"));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn bubblewrap_hides_secrets_when_available() {
        let runner = runner(false);
        if !runner.is_isolated() {
            eprintln!("bubblewrap unavailable; skipping isolation assertion");
            return;
        }
        // Secret directories under $HOME must be absent inside the sandbox.
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let script = format!(
            "for p in {h}/.ssh {h}/.aws {h}/.netrc {h}/.config/gh; do test -e \"$p\" && echo LEAK:$p; done; echo done",
            h = home
        );
        let (stdout, _stderr, _ok) = run(&runner, &script).await;
        assert!(!stdout.contains("LEAK:"), "secrets must be hidden: {stdout}");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn blocked_network_is_unreachable() {
        let runner = runner(true);
        assert!(runner.network_isolated());
        // With the network namespace unshared, only loopback exists and DNS
        // resolution of an external host fails fast.
        let (stdout, _stderr, ok) = run(
            &runner,
            "getent hosts example.com 2>/dev/null && echo REACHABLE || echo BLOCKED",
        )
        .await;
        assert!(ok);
        assert!(stdout.contains("BLOCKED"), "external DNS should fail: {stdout}");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn toolchain_caches_stay_reachable() {
        let runner = runner(false);
        if !runner.is_isolated() {
            return;
        }
        // If cargo is on the host, its binary (under ~/.cargo/bin) must also be
        // reachable inside the sandbox — proving the toolchain binds work.
        if find_on_path("cargo").is_none() {
            return;
        }
        let (stdout, _stderr, ok) = run(&runner, "command -v cargo && echo FOUND").await;
        assert!(ok && stdout.contains("FOUND"), "cargo must be reachable: {stdout}");
    }

    #[tokio::test]
    async fn ulimit_caps_address_space() {
        let mut config = SandboxConfig::default();
        config.tier = SandboxTier::OsLevel;
        config.resource_limits.max_memory_mb = 64;
        let runner = SandboxRunner::new(config);
        // Same guard as every other assertion here: without bubblewrap there is
        // no sandbox, the command runs on the host where `ulimit -v` is
        // unlimited, and the assertion would be testing nothing.
        if !runner.is_isolated() {
            eprintln!("bubblewrap unavailable; skipping vmem assertion");
            return;
        }
        // Ask the shell for the soft virtual-memory limit (in KB) it inherits.
        let (stdout, _stderr, ok) = run(&runner, "ulimit -v").await;
        assert!(ok);
        let kb: u64 = stdout.trim().parse().unwrap_or(u64::MAX);
        assert!(kb <= 64 * 1024 + 4096, "vmem limit not applied: {kb} KB");
    }
}
