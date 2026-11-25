//! Real command isolation.
//!
//! [`SandboxRunner`] turns a [`SandboxConfig`] into an actual isolated command:
//!
//! * **Filesystem** — Linux uses bubblewrap (`bwrap`) to mount the system
//!   read-only, hide the user's home (so `~/.ssh`, `~/.aws`, browser profiles, …
//!   are simply not present), and bind only the configured allowed paths plus
//!   the working directory read-write. The Docker tier (Linux and macOS) runs the command
//!   in a read-only container with the same workspace mounts. macOS uses a
//!   default-deny Seatbelt profile with the same workspace-only write rule.
//! * **Namespaces** — PID, UTS, IPC (and network when policy blocks it) are
//!   unshared, with `--die-with-parent` so no orphan survives the run.
//! * **Resource limits** — a POSIX `ulimit` shim applies CPU-time, address-space,
//!   file-size and process-count ceilings, inherited by everything the command
//!   spawns.
//!
//! When no backend is available (or the [`SandboxTier::Host`] escape hatch is
//! selected) agent commands fail closed. The report says exactly what was
//! applied; it never claims isolation that was not enforced.

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
    /// macOS seatbelt profile (`sandbox-exec`)
    Seatbelt(PathBuf),
    /// Docker container (`docker run`) on Linux or macOS
    Docker(PathBuf),
    /// No OS isolation — limits only (host tier, or a backend is unavailable)
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
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if config.tier == SandboxTier::Docker {
            if let Some(path) = find_on_path("docker") {
                return SandboxBackend::Docker(path);
            }
            return SandboxBackend::None;
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        if config.tier == SandboxTier::Docker {
            return SandboxBackend::None;
        }
        #[cfg(target_os = "macos")]
        if let Some(path) = find_on_path("sandbox-exec") {
            if seatbelt_is_usable(&path) {
                return SandboxBackend::Seatbelt(path);
            }
            tracing::warn!(
                "sandbox-exec exists but cannot apply a profile on this macOS; \
                 refusing to run an unsandboxed command"
            );
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
        matches!(
            self.backend,
            SandboxBackend::Bubblewrap(_) | SandboxBackend::Seatbelt(_) | SandboxBackend::Docker(_)
        )
    }

    /// Whether network access will be truly blocked.
    pub fn network_isolated(&self) -> bool {
        self.is_isolated() && !self.config.network_policy.enabled
    }

    /// Describe the effective protection for the UI.
    pub fn report(&self) -> SandboxReport {
        match &self.backend {
            SandboxBackend::Bubblewrap(path) => SandboxReport {
                backend: format!("bubblewrap ({})", path.display()),
                filesystem_isolated: true,
                network_isolated: !self.config.network_policy.enabled,
                resource_limits: true,
                note: if self.config.network_policy.enabled {
                    "Filesystem + PID/IPC/UTS namespaces enforced; network shared (domain allowlist applies to HTTP tools).".to_string()
                } else {
                    "Filesystem + PID/IPC/UTS + network namespaces enforced.".to_string()
                },
            },
            SandboxBackend::Seatbelt(path) => SandboxReport {
                backend: format!("seatbelt ({})", path.display()),
                filesystem_isolated: true,
                network_isolated: !self.config.network_policy.enabled,
                resource_limits: true,
                note: if self.config.network_policy.enabled {
                    "Seatbelt filesystem profile enforced; network shared (domain allowlist applies to HTTP tools).".to_string()
                } else {
                    "Seatbelt filesystem and network profile enforced.".to_string()
                },
            },
            SandboxBackend::Docker(path) => SandboxReport {
                backend: format!("docker ({})", path.display()),
                filesystem_isolated: true,
                network_isolated: !self.config.network_policy.enabled,
                resource_limits: true,
                note: format!(
                    "Docker container image {}; read-only root, writable workspace mounts, capabilities dropped, network {}. The image must exist or be pullable.",
                    self.docker_image(),
                    if self.config.network_policy.enabled {
                        "bridge"
                    } else {
                        "none"
                    }
                ),
            },
            SandboxBackend::None => {
                let reason = if self.config.tier == SandboxTier::Host {
                    "host tier selected (explicit opt-in)"
                } else if cfg!(target_os = "windows") {
                    "Windows sandbox backend is not implemented; commands are blocked"
                } else if cfg!(target_os = "macos") {
                    "sandbox-exec is missing or cannot apply a profile on this macOS, so commands are blocked"
                } else if self.config.tier == SandboxTier::Docker {
                    "Docker is not installed or not on PATH, so commands are blocked"
                } else {
                    "bubblewrap not found on PATH, so commands are blocked"
                };
                SandboxReport {
                    backend: "none".to_string(),
                    filesystem_isolated: false,
                    network_isolated: false,
                    resource_limits: true,
                    note: format!(
                        "Unsandboxed execution is not allowed: {}. {}{}",
                        reason,
                        if cfg!(target_os = "macos") {
                            "Select the Docker tier (Docker Desktop) or explicitly choose Host for this agent. "
                        } else if cfg!(target_os = "windows") {
                            "There is no Windows backend yet; explicitly choose Host only if you accept unsandboxed execution. "
                        } else {
                            "Install bubblewrap or Docker, or explicitly choose Host for this agent. "
                        },
                        "RAVENBOT_ALLOW_UNSANDBOXED=1 is a deliberate global override."
                    ),
                }
            }
        }
    }

    /// Build the macOS seatbelt profile. The default is deny; system libraries
    /// are readable, and only the workspace paths are writable. Network is
    /// allowed only when the policy enables it.
    fn seatbelt_profile(&self, cwd: Option<&Path>) -> String {
        fn quote(path: &str) -> String {
            format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
        }
        let mut writable: Vec<String> = self.config.allowed_paths.clone();
        if let Some(cwd) = cwd {
            writable.push(cwd.to_string_lossy().to_string());
        }
        // Only this user's temp directory, not the whole /private/var/folders
        // tree shared by every account.
        writable.push(std::env::temp_dir().to_string_lossy().to_string());
        writable.sort();
        writable.dedup();

        let mut rules = vec![
            "(version 1)".to_string(),
            "(deny default)".to_string(),
            "(allow process*)".to_string(),
            "(allow signal)".to_string(),
            "(allow sysctl-read)".to_string(),
            "(allow mach-lookup)".to_string(),
            "(allow file-read-metadata)".to_string(),
            "(allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/Library\") (subpath \"/opt\") (subpath \"/etc\") (subpath \"/private/etc\") (subpath \"/dev\"))".to_string(),
            "(allow file-write* (subpath \"/tmp\") (subpath \"/private/tmp\") (literal \"/dev/null\") (literal \"/dev/stdout\") (literal \"/dev/stderr\"))".to_string(),
        ];
        for path in writable {
            let quoted = quote(&path);
            rules.push(format!("(allow file-read* file-write* (subpath {quoted}))"));
        }
        if self.config.network_policy.enabled {
            rules.push("(allow network*)".to_string());
        }
        rules.join("\n")
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
        self.ulimit_shim_with_process_limit(true)
    }

    /// Docker already enforces its own PID limit. Applying `ulimit -u` inside
    /// the container counts the image's init and shell against the same small
    /// budget and makes ordinary commands fail to fork, so that one limit is
    /// left to `--pids-limit`.
    fn ulimit_shim_with_process_limit(&self, include_process_limit: bool) -> String {
        let limits = &self.config.resource_limits;
        // -t CPU seconds, -v virtual KB, -f file size (512B blocks), -u procs.
        let process_limit = if include_process_limit {
            format!("ulimit -u {} 2>/dev/null; ", limits.max_processes.max(1))
        } else {
            String::new()
        };
        format!(
            "ulimit -t {cpu} 2>/dev/null; \
             ulimit -v {vmem} 2>/dev/null; \
             ulimit -f {fsize} 2>/dev/null; \
             {process_limit}exec \"$0\" \"$@\"",
            cpu = limits.max_task_duration_secs.max(1),
            vmem = limits.max_memory_mb.saturating_mul(1024).max(1024),
            fsize = limits.max_disk_write_mb.saturating_mul(2048).max(2048),
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
        self.build_command(program, args, cwd, false)
    }

    /// Build a command for an application-owned renderer (Pandoc, rsvg-convert)
    /// whose arguments are generated by this app, not by the model. It still
    /// uses isolation and limits when available; on a host without a backend it
    /// may fall back to limits-only so document/slide export keeps working.
    pub fn command_trusted(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
    ) -> Result<Command, String> {
        self.build_command(program, args, cwd, true)
    }

    fn build_command(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        trusted: bool,
    ) -> Result<Command, String> {
        let explicit_host = self.config.tier == SandboxTier::Host;
        let unsandboxed_opt_in = std::env::var("RAVENBOT_ALLOW_UNSANDBOXED")
            .ok()
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        if matches!(self.backend, SandboxBackend::None)
            && !explicit_host
            && !unsandboxed_opt_in
            && !trusted
        {
            let reason = if self.config.tier == SandboxTier::Docker {
                "Docker is not installed or not on PATH"
            } else {
                "bubblewrap is not installed or not on PATH"
            };
            return Err(format!(
                "Refusing to run an unsandboxed command: {reason}. Install bubblewrap on Linux, set this agent's sandbox tier to Host to opt in to host execution, or set RAVENBOT_ALLOW_UNSANDBOXED=1 for a deliberate global override."
            ));
        }
        // Docker has `--pids-limit`. A host process has no PID namespace, so
        // `ulimit -u` would count the user's entire process tree and make an
        // ordinary `fork` fail on a busy desktop. Bubblewrap does unshare PID,
        // so its per-namespace process limit is meaningful.
        let shim = if matches!(
            &self.backend,
            SandboxBackend::Docker(_) | SandboxBackend::None
        ) {
            self.ulimit_shim_with_process_limit(false)
        } else {
            self.ulimit_shim()
        };

        let mut cmd = match &self.backend {
            SandboxBackend::Bubblewrap(bwrap) => {
                let mut cmd = Command::new(bwrap);
                self.apply_bwrap_args(&mut cmd, cwd);
                cmd.arg("--");
                cmd.arg("/bin/sh").arg("-c").arg(&shim).arg(program);
                cmd.args(args);
                cmd
            }
            SandboxBackend::Seatbelt(sandbox_exec) => {
                let profile = self.seatbelt_profile(cwd);
                let mut cmd = Command::new(sandbox_exec);
                cmd.arg("-p")
                    .arg(profile)
                    .arg("/bin/sh")
                    .arg("-c")
                    .arg(&shim)
                    .arg(program);
                cmd.args(args);
                cmd
            }
            SandboxBackend::Docker(docker) => {
                let mut cmd = Command::new(docker);
                self.apply_docker_args(&mut cmd, cwd);
                cmd.arg("--entrypoint")
                    .arg("/bin/sh")
                    .arg(self.docker_image())
                    .arg("-c")
                    .arg(&shim)
                    .arg(program);
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
        // The app process holds provider keys and other credentials in its
        // environment (ProviderManager reads OPENAI_API_KEY, ANTHROPIC_API_KEY,
        // …). A sandboxed command must not inherit them: `env` inside the
        // sandbox would otherwise print every key. Keep only the variables an
        // ordinary tool needs to run.
        cmd.env_clear();
        for (key, value) in std::env::vars() {
            if allowed_child_env(&key) {
                cmd.env(key, value);
            }
        }
        cmd.kill_on_drop(true);
        Ok(cmd)
    }

    fn docker_image(&self) -> String {
        self.config
            .docker_image
            .clone()
            .or_else(|| std::env::var("RAVENBOT_DOCKER_IMAGE").ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "debian:bookworm-slim".to_string())
    }

    #[cfg(unix)]
    fn apply_docker_args(&self, cmd: &mut Command, cwd: Option<&Path>) {
        use std::os::unix::fs::MetadataExt;
        cmd.args([
            "run",
            "--rm",
            "--init",
            "--read-only",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--env",
            "HOME=/tmp",
            "--env",
            "TMPDIR=/tmp",
        ]);

        let limits = &self.config.resource_limits;
        if limits.max_memory_mb != u64::MAX {
            cmd.args(["--memory", &format!("{}m", limits.max_memory_mb.max(32))]);
        }
        if limits.max_processes != u32::MAX {
            cmd.args(["--pids-limit", &limits.max_processes.max(1).to_string()]);
        }
        if limits.max_task_duration_secs != u64::MAX {
            cmd.args([
                "--ulimit",
                &format!("cpu={}", limits.max_task_duration_secs.max(1)),
            ]);
        }
        if limits.max_disk_write_mb != u64::MAX {
            cmd.args([
                "--ulimit",
                &format!(
                    "fsize={}",
                    limits.max_disk_write_mb.saturating_mul(2048).max(2048)
                ),
            ]);
        }
        let cpus = (limits.max_cpu_percent / 100.0).clamp(0.1, 64.0);
        cmd.args(["--cpus", &format!("{cpus:.2}")]);
        let tmp_mb = limits.max_disk_write_mb.clamp(16, 4096);
        cmd.args([
            "--tmpfs",
            &format!("/tmp:rw,exec,nosuid,nodev,size={tmp_mb}m"),
            "--network",
            if self.config.network_policy.enabled {
                "bridge"
            } else {
                "none"
            },
        ]);

        if let Ok(meta) = std::fs::metadata("/proc/self") {
            cmd.args(["--user", &format!("{}:{}", meta.uid(), meta.gid())]);
        }

        let mut paths: Vec<PathBuf> = Vec::new();
        if let Some(dir) = cwd {
            paths.push(expand_path(&dir.to_string_lossy()));
        }
        for allowed in &self.config.allowed_paths {
            paths.push(expand_path(allowed));
        }
        let mut mounted: Vec<PathBuf> = Vec::new();
        for path in paths {
            if !path.exists()
                || path == Path::new("/tmp")
                || self.is_blocked(&path)
                || mounted.iter().any(|existing| existing == &path)
            {
                continue;
            }
            let as_text = path.to_string_lossy().to_string();
            cmd.args(["-v", &format!("{as_text}:{as_text}:rw")]);
            mounted.push(path);
        }

        if let Some(dir) = cwd {
            cmd.args([
                "--workdir",
                &expand_path(&dir.to_string_lossy()).to_string_lossy(),
            ]);
        } else {
            cmd.args(["--workdir", "/tmp"]);
        }
    }

    #[cfg(not(unix))]
    fn apply_docker_args(&self, _cmd: &mut Command, _cwd: Option<&Path>) {}

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

/// Environment variables a child process may inherit.
///
/// Anything that looks like a credential is dropped even if it is not in this
/// list; the allowlist is deliberate rather than a deny-list of known keys.
/// Whether `sandbox-exec` can actually apply a profile here.
///
/// macOS 26 still ships the binary in some images but refuses to apply a
/// profile. Treating "the file exists" as "isolation works" would let the app
/// claim a sandbox and then fail every command — or, worse, tempt a fallback to
/// the host. Probe once per process and fail closed when the probe rejects.
#[cfg(target_os = "macos")]
fn seatbelt_is_usable(path: &std::path::Path) -> bool {
    use std::sync::OnceLock;
    static USABLE: OnceLock<bool> = OnceLock::new();
    *USABLE.get_or_init(|| {
        std::process::Command::new(path)
            .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

fn allowed_child_env(key: &str) -> bool {
    matches!(
        key,
        "PATH"
            | "HOME"
            | "USER"
            | "LOGNAME"
            | "SHELL"
            | "LANG"
            | "LANGUAGE"
            | "TERM"
            | "TMPDIR"
            | "TZ"
            | "CARGO_HOME"
            | "RUSTUP_HOME"
            | "DOCKER_HOST"
            | "DOCKER_CONTEXT"
            | "DOCKER_CONFIG"
            | "XDG_RUNTIME_DIR"
    ) || key.starts_with("LC_")
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

    #[cfg(unix)]
    #[test]
    fn docker_backend_builds_a_locked_down_command() {
        let work = std::env::temp_dir().join(format!("ravenbot-docker-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&work).unwrap();
        let runner = SandboxRunner {
            config: SandboxConfig {
                tier: SandboxTier::Docker,
                resource_limits: ResourceLimits::default(),
                network_policy: NetworkPolicy::blocked(),
                allowed_paths: Vec::new(),
                docker_image: Some("alpine:3.20".to_string()),
                ..Default::default()
            },
            backend: SandboxBackend::Docker(PathBuf::from("docker")),
        };
        let cmd = runner
            .command(
                "sh",
                &["-c".to_string(), "echo ok".to_string()],
                Some(&work),
            )
            .expect("docker command");
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        for expected in [
            "run",
            "--rm",
            "--read-only",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--network",
            "none",
            "--memory",
            "--pids-limit",
            "--cpus",
            "alpine:3.20",
            "/bin/sh",
        ] {
            assert!(
                args.iter().any(|arg| arg == expected),
                "missing {expected}: {args:?}"
            );
        }
        #[cfg(target_os = "linux")]
        assert!(
            args.iter().any(|arg| arg == "--user"),
            "Linux Docker runs as the invoking uid/gid: {args:?}"
        );
        assert!(args.iter().any(|arg| arg == "--tmpfs"));
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "-v" && pair[1].contains(&work.to_string_lossy().to_string())));
        assert!(runner.is_isolated());
        assert!(runner.report().backend.contains("docker"));
        assert!(runner.report().filesystem_isolated);
        assert!(runner.report().network_isolated);
        let _ = std::fs::remove_dir_all(work);
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
        if !runner.is_isolated() {
            eprintln!("bubblewrap unavailable; skipping command execution");
            return;
        }
        let (stdout, _stderr, ok) = run(&runner, "echo hello-sandbox").await;
        assert!(ok);
        assert!(stdout.contains("hello-sandbox"));
    }

    #[test]
    fn seatbelt_profile_denies_by_default_and_only_opens_the_workspace() {
        let mut runner = SandboxRunner {
            config: SandboxConfig {
                tier: SandboxTier::OsLevel,
                resource_limits: ResourceLimits::default(),
                network_policy: NetworkPolicy::blocked(),
                allowed_paths: vec!["/tmp/office".to_string()],
                ..Default::default()
            },
            backend: SandboxBackend::Seatbelt(PathBuf::from("/usr/bin/sandbox-exec")),
        };
        let profile = runner.seatbelt_profile(Some(Path::new("/tmp/office/sub")));
        assert!(profile.contains("(deny default)"));
        assert!(profile.contains("(subpath \"/tmp/office\")"));
        assert!(profile.contains("(subpath \"/tmp/office/sub\")"));
        assert!(!profile.contains("(allow network*)"));
        assert!(runner.report().backend.contains("seatbelt"));
        assert!(runner.is_isolated());
        assert!(runner.command("sh", &[], None).is_ok());

        runner.config.network_policy = NetworkPolicy::permissive();
        assert!(runner.seatbelt_profile(None).contains("(allow network*)"));
    }

    #[test]
    fn non_host_tier_without_a_backend_refuses_to_run() {
        let config = SandboxConfig {
            tier: SandboxTier::Docker,
            resource_limits: ResourceLimits::default(),
            network_policy: NetworkPolicy::default(),
            ..Default::default()
        };
        let blocked = SandboxRunner {
            config,
            backend: SandboxBackend::None,
        };
        let error = blocked
            .command("sh", &[], None)
            .expect_err("docker without a backend must fail closed");
        assert!(error.contains("Refusing to run an unsandboxed command"));

        let host = SandboxRunner {
            config: SandboxConfig {
                tier: SandboxTier::Host,
                resource_limits: ResourceLimits::default(),
                network_policy: NetworkPolicy::default(),
                ..Default::default()
            },
            backend: SandboxBackend::None,
        };
        assert!(
            host.command("sh", &[], None).is_ok(),
            "the Host tier is the explicit unsandboxed opt-in"
        );
        assert!(
            blocked.command_trusted("sh", &[], None).is_ok(),
            "app-owned renderers may fall back to limits-only"
        );
    }

    #[tokio::test]
    async fn provider_keys_are_not_inherited_by_a_sandboxed_command() {
        let key = format!("RAVENBOT_TEST_SECRET_{}", uuid::Uuid::new_v4());
        std::env::set_var(&key, "do-not-leak");
        let runner = runner(false);
        if !runner.is_isolated() {
            std::env::remove_var(&key);
            eprintln!("bubblewrap unavailable; skipping environment isolation");
            return;
        }
        let (stdout, _stderr, _ok) = run(&runner, &format!("printf %s \"${key}\"")).await;
        std::env::remove_var(&key);
        assert!(
            !stdout.contains("do-not-leak"),
            "a credential leaked into the child: {stdout}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "requires Docker and alpine:3.20; run with RAVENBOT_DOCKER_TEST=1"]
    async fn docker_tier_runs_in_a_real_isolated_container() {
        let work =
            std::env::temp_dir().join(format!("ravenbot-docker-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("marker.txt"), b"docker-marker").unwrap();
        let runner = SandboxRunner::new(SandboxConfig {
            tier: SandboxTier::Docker,
            resource_limits: ResourceLimits::default(),
            network_policy: NetworkPolicy::blocked(),
            allowed_paths: vec![work.to_string_lossy().to_string()],
            docker_image: Some("alpine:3.20".to_string()),
            ..Default::default()
        });
        if !matches!(runner.backend(), SandboxBackend::Docker(_)) {
            eprintln!("docker unavailable; skipping live docker test");
            let _ = std::fs::remove_dir_all(work);
            return;
        }
        let mut cmd = runner
            .command(
                "sh",
                &[
                    "-c".to_string(),
                    "cat marker.txt; test ! -e /root/.ssh && echo no-ssh; if nslookup example.com >/dev/null 2>&1; then echo NET-LEAK; else echo net-blocked; fi; echo docker-ok"
                        .to_string(),
                ],
                Some(&work),
            )
            .expect("docker command");
        let out = cmd.output().await.expect("run docker");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "docker failed: {stderr}");
        assert!(stdout.contains("docker-marker"), "{stdout} {stderr}");
        assert!(stdout.contains("no-ssh"), "{stdout} {stderr}");
        assert!(stdout.contains("net-blocked"), "{stdout} {stderr}");
        assert!(!stdout.contains("NET-LEAK"), "{stdout} {stderr}");
        assert!(stdout.contains("docker-ok"), "{stdout} {stderr}");
        let _ = std::fs::remove_dir_all(work);
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
        assert!(
            !stdout.contains("LEAK:"),
            "secrets must be hidden: {stdout}"
        );
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
        assert!(
            stdout.contains("BLOCKED"),
            "external DNS should fail: {stdout}"
        );
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
        assert!(
            ok && stdout.contains("FOUND"),
            "cargo must be reachable: {stdout}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn host_execution_does_not_apply_a_user_wide_process_limit() {
        let mut config = SandboxConfig::default();
        config.tier = SandboxTier::Host;
        let runner = SandboxRunner::new(config);
        // `ulimit -u 10` counts the user's entire process tree and makes a
        // normal fork fail on a busy desktop. Host execution must not set it.
        let (stdout, stderr, ok) = run(&runner, "sh -c 'echo forked-ok'").await;
        assert!(ok, "{stdout} {stderr}");
        assert!(stdout.contains("forked-ok"), "{stdout} {stderr}");
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

#[cfg(test)]
mod platform_contract_tests {
    use super::*;

    #[test]
    fn a_missing_backend_never_claims_filesystem_or_network_isolation() {
        let runner = SandboxRunner {
            config: SandboxConfig {
                tier: SandboxTier::OsLevel,
                ..Default::default()
            },
            backend: SandboxBackend::None,
        };
        let report = runner.report();
        assert_eq!(report.backend, "none");
        assert!(!report.filesystem_isolated);
        assert!(!report.network_isolated);
        assert!(report.note.contains("Unsandboxed execution is not allowed"));
        assert!(!runner.is_isolated());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_fails_closed_without_a_sandbox_backend() {
        let runner = SandboxRunner::from_tier(SandboxTier::OsLevel);
        assert!(matches!(runner.backend(), SandboxBackend::None));
        let error = runner
            .command("cmd", &["/C".to_string(), "echo hello".to_string()], None)
            .expect_err("Windows agent commands must fail closed");
        assert!(error.contains("Refusing to run an unsandboxed command"));
        assert!(runner.report().note.contains("Windows"));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore = "requires a real macOS host and sandbox-exec; run in the macOS contract job"]
    async fn seatbelt_live_contract() {
        let work =
            std::env::temp_dir().join(format!("ravenbot-seatbelt-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&work).expect("work dir");
        std::fs::write(work.join("marker.txt"), b"seatbelt-marker").expect("marker");
        let runner = SandboxRunner::new(SandboxConfig {
            tier: SandboxTier::OsLevel,
            resource_limits: ResourceLimits::default(),
            network_policy: NetworkPolicy::blocked(),
            allowed_paths: vec![work.to_string_lossy().to_string()],
            ..Default::default()
        });
        if !matches!(runner.backend(), SandboxBackend::Seatbelt(_)) {
            eprintln!(
                "sandbox-exec is unavailable or cannot apply a profile on this macOS; \
                 skipping the live seatbelt test"
            );
            let _ = std::fs::remove_dir_all(&work);
            return;
        }
        assert!(runner.is_isolated());
        assert!(runner.network_isolated());

        let mut cmd = runner
            .command(
                "sh",
                &[
                    "-c".to_string(),
                    "cat marker.txt; test ! -e \"$HOME/.ssh\" && echo no-ssh; if curl -m 2 -sS https://example.com >/dev/null 2>&1; then echo NET-LEAK; else echo net-blocked; fi; echo seatbelt-ok".to_string(),
                ],
                Some(&work),
            )
            .expect("build seatbelt command");
        let out = cmd.output().await.expect("run seatbelt command");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "seatbelt failed: {stderr}");
        assert!(stdout.contains("seatbelt-marker"), "{stdout} {stderr}");
        assert!(stdout.contains("no-ssh"), "{stdout} {stderr}");
        assert!(stdout.contains("net-blocked"), "{stdout} {stderr}");
        assert!(!stdout.contains("NET-LEAK"), "{stdout} {stderr}");
        assert!(stdout.contains("seatbelt-ok"), "{stdout} {stderr}");
        let _ = std::fs::remove_dir_all(work);
    }
}
