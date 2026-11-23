//! Per-user background service installation.
//!
//! The desktop app already keeps the fleet alive while its window is hidden.
//! This module adds the next step: a user-level service that starts the app
//! hidden at login and restarts it after a crash.
//!
//! It deliberately does **not** start a second copy while the current window is
//! open. The install flow writes and enables the unit; the UI then offers a
//! hand-off that exits this process and starts the service in its place. That
//! keeps one scheduler, one webhook receiver and one SQLite writer.
//!
//! Scope is honest: these are *user-session* services. They start at login and
//! keep running with no window, but a graphical webview still needs a
//! graphical session. True headless operation after logout requires the
//! headless runtime planned with the remote API.

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const SERVICE_NAME: &str = "ravenbot";
pub const SERVICE_LABEL: &str = "dev.ravenbot.background";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServicePlatform {
    LinuxSystemd,
    MacosLaunchd,
    WindowsTaskScheduler,
    Unsupported,
}

impl ServicePlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::LinuxSystemd
        } else if cfg!(target_os = "macos") {
            Self::MacosLaunchd
        } else if cfg!(target_os = "windows") {
            Self::WindowsTaskScheduler
        } else {
            Self::Unsupported
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LinuxSystemd => "systemd (user)",
            Self::MacosLaunchd => "launchd (user agent)",
            Self::WindowsTaskScheduler => "Task Scheduler (logon)",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceOptions {
    pub platform: ServicePlatform,
    pub executable: PathBuf,
    /// The user's home directory.
    pub home: PathBuf,
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    pub config_home: PathBuf,
    /// RAVENBOT's data root (`~/RAVENBOT`).
    pub data_root: PathBuf,
    /// Directory for launchd stdout/stderr on macOS.
    pub log_dir: PathBuf,
    /// Numeric user id, used by `launchctl gui/<uid>`.
    pub uid: u32,
    /// User name, used by `loginctl enable-linger`.
    pub user: String,
    /// Unit/task name. The owner keeps `ravenbot`; account units are suffixed.
    pub service_name: String,
    /// launchd label. The owner keeps the legacy label.
    pub label: String,
    /// Extra environment for a per-workspace daemon.
    pub environment: Vec<(String, String)>,
}

impl ServiceOptions {
    pub fn current() -> Result<Self, String> {
        let platform = ServicePlatform::current();
        // Windows has no implementation yet. Do not fail before the caller can
        // report that honestly just because `$HOME`/`$USER` are Unix-shaped.
        if platform == ServicePlatform::Unsupported {
            return Ok(Self {
                platform,
                executable: std::env::current_exe().unwrap_or_default(),
                home: PathBuf::new(),
                config_home: PathBuf::new(),
                data_root: PathBuf::new(),
                log_dir: PathBuf::new(),
                uid: 0,
                user: String::new(),
                service_name: SERVICE_NAME.to_string(),
                label: SERVICE_LABEL.to_string(),
                environment: Vec::new(),
            });
        }
        let home = home_dir()?;
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        // Inside an AppImage, `current_exe` points into a temporary mount that
        // disappears when the app exits. The AppImage file itself is the stable
        // path a service must launch.
        let executable = std::env::var("APPIMAGE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
            .ok_or_else(|| "Cannot find the RAVENBOT executable".to_string())?;
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .or_else(|_| std::env::var("USERNAME"))
            .map_err(|_| "Cannot determine the current user".to_string())?;
        #[cfg(unix)]
        let uid = {
            // `id -u` avoids adding a libc dependency just for getuid.
            let output = std::process::Command::new("id")
                .arg("-u")
                .output()
                .map_err(|error| format!("Cannot determine the user id: {error}"))?;
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
                .map_err(|_| "Cannot parse the current user id".to_string())?
        };
        #[cfg(not(unix))]
        let uid = 0;
        let service_name = std::env::var("RAVENBOT_SERVICE_NAME")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| SERVICE_NAME.to_string());
        let label = std::env::var("RAVENBOT_SERVICE_LABEL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| SERVICE_LABEL.to_string());
        let mut environment = Vec::new();
        for key in [
            "RAVENBOT_BASE_HOME",
            "RAVENBOT_DB",
            "RAVENBOT_WORKSPACE_ID",
            "RAVENBOT_KEYCHAIN_NAMESPACE",
            "RAVENBOT_WEBHOOK_PORT",
            "RAVENBOT_CHAT_WEBHOOK_PORT",
            "RAVENBOT_REMOTE_PORT",
        ] {
            if let Ok(value) = std::env::var(key) {
                if !value.trim().is_empty() {
                    environment.push((key.to_string(), value));
                }
            }
        }
        environment.push(("RAVENBOT_SERVICE_NAME".to_string(), service_name.clone()));
        environment.push(("RAVENBOT_SERVICE_LABEL".to_string(), label.clone()));
        Ok(Self {
            platform,
            executable,
            config_home,
            home,
            data_root: ravenbot_core::raven_root(),
            log_dir: ravenbot_core::logs_dir(),
            uid,
            user,
            service_name,
            label,
            environment,
        })
    }

    pub fn service_path(&self) -> PathBuf {
        match self.platform {
            ServicePlatform::LinuxSystemd => self
                .config_home
                .join("systemd/user")
                .join(format!("{}.service", self.service_name)),
            ServicePlatform::MacosLaunchd => self
                .home
                .join("Library/LaunchAgents")
                .join(format!("{}.plist", self.label)),
            ServicePlatform::WindowsTaskScheduler => {
                if self.service_name == SERVICE_NAME {
                    ravenbot_core::config_dir().join("ravenbot-task.xml")
                } else {
                    ravenbot_core::config_dir()
                        .join(format!("ravenbot-task-{}.xml", self.service_name))
                }
            }
            ServicePlatform::Unsupported => self.home.join(".ravenbot-service"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    pub supported: bool,
    pub platform: String,
    pub installed: bool,
    pub enabled: bool,
    pub running: bool,
    /// `None` where lingering is not a concept (macOS/Windows).
    pub linger: Option<bool>,
    pub path: String,
    pub detail: String,
}

/// Quote a value for a systemd `ExecStart` line.
fn systemd_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

/// `Environment=` takes a quoted assignment, not a quoted value. Quoting only
/// the value made systemd see the quotes as part of the path.
fn systemd_environment(name: &str, value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    format!("Environment=\"{name}={escaped}\"")
}

/// Escape XML text and attribute values in a launchd plist.
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The systemd unit. `Restart=on-failure` means a clean tray Quit (exit 0)
/// stays quit; a crash is retried.
pub fn render_systemd_unit(options: &ServiceOptions) -> String {
    let exe = systemd_quote(&options.executable.to_string_lossy());
    let root = systemd_environment("RAVENBOT_HOME", &options.data_root.to_string_lossy());
    let extra = options
        .environment
        .iter()
        .map(|(key, value)| systemd_environment(key, value))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "[Unit]\n\
Description=RAVENBOT headless fleet daemon\n\
Documentation=https://github.com/AetherRavyn/RAVENBOT\n\
After=network-online.target\n\
Wants=network-online.target\n\
\n\
[Service]\n\
Type=simple\n\
ExecStart={exe} daemon\n\
{root}\n\
{extra}\n\
Environment=\"RUST_LOG=info\"\n\
Restart=on-failure\n\
RestartSec=5\n\
TimeoutStopSec=20\n\
KillMode=mixed\n\
NoNewPrivileges=true\n\
PrivateTmp=true\n\
ProtectSystem=full\n\
ProtectKernelTunables=true\n\
ProtectKernelModules=true\n\
ProtectKernelLogs=true\n\
ProtectControlGroups=true\n\
ProtectClock=true\n\
ProtectHostname=true\n\
RestrictRealtime=true\n\
LockPersonality=true\n\
LimitCORE=0\n\
\n\
[Install]\n\
WantedBy=default.target\n"
    )
}

/// The macOS LaunchAgent. `KeepAlive` with `SuccessfulExit=false` mirrors the
/// systemd policy: crashes restart, a clean Quit does not.
pub fn render_launchd_plist(options: &ServiceOptions) -> String {
    let exe = xml_escape(&options.executable.to_string_lossy());
    let workdir = xml_escape(&options.home.to_string_lossy());
    let root = xml_escape(&options.data_root.to_string_lossy());
    let label = xml_escape(&options.label);
    let stdout = xml_escape(&options.log_dir.join("service.log").to_string_lossy());
    let stderr = xml_escape(&options.log_dir.join("service.error.log").to_string_lossy());
    let mut environment = format!("    <key>RAVENBOT_HOME</key>\n    <string>{root}</string>\n");
    for (key, value) in &options.environment {
        environment.push_str(&format!(
            "    <key>{}</key>\n    <string>{}</string>\n",
            xml_escape(key),
            xml_escape(value)
        ));
    }
    environment.push_str("    <key>RUST_LOG</key>\n    <string>info</string>\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>Label</key>\n\
  <string>{label}</string>\n\
  <key>ProgramArguments</key>\n\
  <array>\n\
    <string>{exe}</string>\n\
    <string>daemon</string>\n\
  </array>\n\
  <key>WorkingDirectory</key>\n\
  <string>{workdir}</string>\n\
  <key>EnvironmentVariables</key>\n\
  <dict>\n\
{environment}  </dict>\n\
  <key>RunAtLoad</key>\n\
  <true/>\n\
  <key>KeepAlive</key>\n\
  <dict>\n\
    <key>SuccessfulExit</key>\n\
    <false/>\n\
  </dict>\n\
  <key>ProcessType</key>\n\
  <string>Background</string>\n\
  <key>StandardOutPath</key>\n\
  <string>{stdout}</string>\n\
  <key>StandardErrorPath</key>\n\
  <string>{stderr}</string>\n\
</dict>\n\
</plist>\n"
    )
}

/// Windows Task Scheduler XML. This is a per-user logon task, not a Windows
/// service: it needs no administrator rights, runs the headless daemon, and
/// stops when the user session ends. A true service would need a service
/// wrapper and admin installation.
pub fn render_windows_task_xml(options: &ServiceOptions) -> String {
    let exe = xml_escape(&options.executable.to_string_lossy());
    let home = xml_escape(&options.home.to_string_lossy());
    let user = std::env::var("USERDOMAIN")
        .ok()
        .filter(|domain| !domain.trim().is_empty())
        .map(|domain| format!("{domain}\\{}", options.user))
        .unwrap_or_else(|| options.user.clone());
    let user = xml_escape(&user);
    let service_name = xml_escape(&options.service_name);
    let (command, arguments) = if options.environment.is_empty() {
        (exe.clone(), "daemon".to_string())
    } else {
        (
            "cmd.exe".to_string(),
            format!(
                "/c \"{}\"",
                xml_escape(&windows_wrapper_path(options).to_string_lossy())
            ),
        )
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n\
<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n\
  <RegistrationInfo>\n\
    <Description>RAVENBOT headless fleet daemon ({service_name})</Description>\n\
  </RegistrationInfo>\n\
  <Triggers>\n\
    <LogonTrigger>\n\
      <Enabled>true</Enabled>\n\
      <UserId>{user}</UserId>\n\
    </LogonTrigger>\n\
  </Triggers>\n\
  <Principals>\n\
    <Principal id=\"Author\">\n\
      <UserId>{user}</UserId>\n\
      <LogonType>InteractiveToken</LogonType>\n\
      <RunLevel>LeastPrivilege</RunLevel>\n\
    </Principal>\n\
  </Principals>\n\
  <Settings>\n\
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n\
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n\
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n\
    <AllowHardTerminate>true</AllowHardTerminate>\n\
    <StartWhenAvailable>true</StartWhenAvailable>\n\
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>\n\
    <IdleSettings>\n\
      <StopOnIdleEnd>false</StopOnIdleEnd>\n\
      <RestartOnIdle>false</RestartOnIdle>\n\
    </IdleSettings>\n\
    <AllowStartOnDemand>true</AllowStartOnDemand>\n\
    <Enabled>true</Enabled>\n\
    <Hidden>true</Hidden>\n\
    <RunOnlyIfIdle>false</RunOnlyIfIdle>\n\
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n\
    <RestartOnFailure>\n\
      <Interval>PT1M</Interval>\n\
      <Count>999</Count>\n\
    </RestartOnFailure>\n\
  </Settings>\n\
  <Actions Context=\"Author\">\n\
    <Exec>\n\
      <Command>{command}</Command>\n\
      <Arguments>{arguments}</Arguments>\n\
      <WorkingDirectory>{home}</WorkingDirectory>\n\
    </Exec>\n\
  </Actions>\n\
</Task>\n"
    )
}

fn write_windows_wrapper(options: &ServiceOptions) -> Result<(), String> {
    let path = windows_wrapper_path(options);
    let mut lines = vec!["@echo off".to_string()];
    for (key, value) in &options.environment {
        lines.push(format!("set \"{key}={}\"", value.replace('"', "")));
    }
    lines.push(format!("\"{}\" daemon", options.executable.display()));
    let contents = lines.join("\r\n") + "\r\n";
    write_if_changed(&path, &contents)?;
    Ok(())
}

fn write_windows_task(path: &Path, xml: &str) -> Result<bool, String> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    if std::fs::read(path).ok().as_deref() == Some(bytes.as_slice()) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    }
    std::fs::write(path, bytes)
        .map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
    Ok(true)
}

pub fn render(options: &ServiceOptions) -> Result<String, String> {
    match options.platform {
        ServicePlatform::LinuxSystemd => Ok(render_systemd_unit(options)),
        ServicePlatform::MacosLaunchd => Ok(render_launchd_plist(options)),
        ServicePlatform::WindowsTaskScheduler => Ok(render_windows_task_xml(options)),
        ServicePlatform::Unsupported => Err(
            "Background service installation is not implemented on this platform; use the tray and autostart."
                .to_string(),
        ),
    }
}

/// Write only when the content changed, so reinstalling is idempotent.
fn write_if_changed(path: &Path, contents: &str) -> Result<bool, String> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if existing == contents {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    }
    std::fs::write(path, contents)
        .map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
    Ok(true)
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("Could not run {program}: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(if stdout.is_empty() { stderr } else { stdout })
    } else {
        Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            if stderr.is_empty() { stdout } else { stderr }
        ))
    }
}

fn try_run(program: &str, args: &[&str]) -> (bool, String) {
    match std::process::Command::new(program).args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            (
                output.status.success(),
                if stdout.is_empty() { stderr } else { stdout },
            )
        }
        Err(error) => (false, error.to_string()),
    }
}

fn systemd_unit(options: &ServiceOptions) -> String {
    format!("{}.service", options.service_name)
}

fn windows_task_name(options: &ServiceOptions) -> String {
    if options.service_name == SERVICE_NAME {
        "RAVENBOT".to_string()
    } else {
        format!("RAVENBOT-{}", options.service_name)
    }
}

fn windows_wrapper_path(options: &ServiceOptions) -> PathBuf {
    ravenbot_core::config_dir().join(format!("{}.cmd", options.service_name))
}

fn linux_status(options: &ServiceOptions) -> ServiceStatus {
    let path = options.service_path();
    let installed = path.is_file();
    let (enabled, _) = try_run(
        "systemctl",
        &["--user", "is-enabled", &systemd_unit(options)],
    );
    let (active, detail) = try_run(
        "systemctl",
        &["--user", "is-active", &systemd_unit(options)],
    );
    let linger = try_run("loginctl", &["show-user", &options.user, "-p", "Linger"])
        .1
        .split('=')
        .nth(1)
        .map(|value| value.trim().eq_ignore_ascii_case("yes"));
    ServiceStatus {
        supported: true,
        platform: options.platform.label().to_string(),
        installed,
        enabled,
        running: active,
        linger,
        path: path.display().to_string(),
        detail: if installed {
            detail
        } else {
            "not installed".to_string()
        },
    }
}

fn macos_status(options: &ServiceOptions) -> ServiceStatus {
    let path = options.service_path();
    let installed = path.is_file();
    let target = format!("gui/{}/{}", options.uid, options.label);
    let (running, detail) = try_run("launchctl", &["print", &target]);
    ServiceStatus {
        supported: true,
        platform: options.platform.label().to_string(),
        installed,
        enabled: installed,
        running,
        linger: None,
        path: path.display().to_string(),
        detail: if installed {
            detail
        } else {
            "not installed".to_string()
        },
    }
}

fn windows_status(options: &ServiceOptions) -> ServiceStatus {
    let path = options.service_path();
    let task = windows_task_name(options);
    let (queried, _) = try_run("schtasks", &["/Query", "/TN", &task, "/FO", "LIST"]);
    let installed = queried || path.is_file();
    let (power_ok, state) = try_run(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            &format!("(Get-ScheduledTask -TaskName '{}').State", task),
        ],
    );
    let running = power_ok && state.contains("Running");
    ServiceStatus {
        supported: true,
        platform: options.platform.label().to_string(),
        installed,
        enabled: installed,
        running,
        linger: None,
        path: path.display().to_string(),
        detail: if installed {
            state
        } else {
            "not installed".to_string()
        },
    }
}

pub fn status_with(options: &ServiceOptions) -> ServiceStatus {
    match options.platform {
        ServicePlatform::LinuxSystemd => linux_status(options),
        ServicePlatform::MacosLaunchd => macos_status(options),
        ServicePlatform::WindowsTaskScheduler => windows_status(options),
        ServicePlatform::Unsupported => ServiceStatus {
            supported: false,
            platform: options.platform.label().to_string(),
            installed: false,
            enabled: false,
            running: false,
            linger: None,
            path: options.service_path().display().to_string(),
            detail: "Background service installation is not implemented on this platform."
                .to_string(),
        },
    }
}

pub fn status() -> Result<ServiceStatus, String> {
    let options = ServiceOptions::current()?;
    Ok(status_with(&options))
}

/// Write and enable the unit. Does not start a second instance while the
/// desktop app is open; `handoff` performs the restart.
pub fn install_with(options: &ServiceOptions) -> Result<ServiceStatus, String> {
    match options.platform {
        ServicePlatform::Unsupported => Err(
            "Background service installation is not implemented on this platform; use the tray and autostart."
                .to_string(),
        ),
        ServicePlatform::LinuxSystemd => {
            let unit = render(options)?;
            write_if_changed(&options.service_path(), &unit)?;
            run("systemctl", &["--user", "daemon-reload"])?;
            run("systemctl", &["--user", "enable", &systemd_unit(options)])?;
            // Lingering is what lets the user manager exist without a login
            // session. It may require polkit; failure is reported, not hidden.
            let _ = run("loginctl", &["enable-linger", &options.user]);
            Ok(linux_status(options))
        }
        ServicePlatform::MacosLaunchd => {
            std::fs::create_dir_all(&options.log_dir).map_err(|error| {
                format!(
                    "Cannot create the service log directory {}: {error}",
                    options.log_dir.display()
                )
            })?;
            let plist = render(options)?;
            write_if_changed(&options.service_path(), &plist)?;
            // Bootstrapping now would collide with this process. The plist's
            // RunAtLoad handles the next login, and handoff starts it now.
            Ok(macos_status(options))
        }
        ServicePlatform::WindowsTaskScheduler => {
            let path = options.service_path();
            let xml = render_windows_task_xml(options);
            write_windows_task(&path, &xml)?;
            if !options.environment.is_empty() {
                write_windows_wrapper(options)?;
            }
            let path_text = path.to_string_lossy().to_string();
            let task = windows_task_name(options);
            run(
                "schtasks",
                &["/Create", "/TN", &task, "/XML", &path_text, "/F"],
            )?;
            // Creating the task does not start it; handoff or the next logon
            // does. Starting now would collide with this process.
            Ok(windows_status(options))
        }
    }
}

pub fn install() -> Result<ServiceStatus, String> {
    let options = ServiceOptions::current()?;
    install_with(&options)
}

/// Start the installed service. The daemon is a no-op if another fleet owner
/// holds the lock, so this is safe to run when the desktop app is open.
pub fn start_running() -> Result<(), String> {
    let options = ServiceOptions::current()?;
    match options.platform {
        ServicePlatform::LinuxSystemd => {
            run("systemctl", &["--user", "start", &systemd_unit(&options)])?;
            Ok(())
        }
        ServicePlatform::MacosLaunchd => {
            let domain = format!("gui/{}", options.uid);
            let path = options.service_path();
            run(
                "launchctl",
                &["bootstrap", &domain, &path.to_string_lossy()],
            )?;
            Ok(())
        }
        ServicePlatform::WindowsTaskScheduler => {
            let task = windows_task_name(&options);
            run("schtasks", &["/Run", "/TN", &task])?;
            Ok(())
        }
        ServicePlatform::Unsupported => {
            Err("Background service control is not implemented on this platform.".to_string())
        }
    }
}

/// Stop the installed service without removing it. Used when the desktop app
/// is launched while the headless daemon owns the fleet.
pub fn stop_running() -> Result<(), String> {
    let options = ServiceOptions::current()?;
    match options.platform {
        ServicePlatform::LinuxSystemd => {
            run("systemctl", &["--user", "stop", &systemd_unit(&options)])?;
            Ok(())
        }
        ServicePlatform::MacosLaunchd => {
            let target = format!("gui/{}/{}", options.uid, options.label);
            let _ = try_run("launchctl", &["bootout", &target]);
            Ok(())
        }
        ServicePlatform::WindowsTaskScheduler => {
            // Ending an already-stopped task is not an error for the caller.
            let task = windows_task_name(&options);
            let _ = try_run("schtasks", &["/End", "/TN", &task]);
            Ok(())
        }
        ServicePlatform::Unsupported => {
            Err("Background service control is not implemented on this platform.".to_string())
        }
    }
}

pub fn uninstall_with(options: &ServiceOptions) -> Result<ServiceStatus, String> {
    match options.platform {
        ServicePlatform::Unsupported => Ok(status_with(options)),
        ServicePlatform::LinuxSystemd => {
            // Order matters. Disable and remove the file first, then stop while
            // systemd still has the unit loaded, then reload. If this process
            // *is* the service, `stop` kills us after uninstall is complete;
            // reloading before `stop` would make the unit unknown and leave the
            // process running with no file.
            let _ = try_run("systemctl", &["--user", "disable", &systemd_unit(options)]);
            let path = options.service_path();
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|error| format!("Cannot remove {}: {error}", path.display()))?;
            }
            let _ = try_run("systemctl", &["--user", "stop", &systemd_unit(options)]);
            let _ = try_run("systemctl", &["--user", "daemon-reload"]);
            Ok(status_with(options))
        }
        ServicePlatform::MacosLaunchd => {
            // Remove the plist first for the same reason: bootout may terminate
            // this process when it is the service, and the file must already be
            // gone by then.
            let path = options.service_path();
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|error| format!("Cannot remove {}: {error}", path.display()))?;
            }
            let target = format!("gui/{}/{}", options.uid, options.label);
            let _ = try_run("launchctl", &["bootout", &target]);
            Ok(macos_status(options))
        }
        ServicePlatform::WindowsTaskScheduler => {
            let task = windows_task_name(options);
            let _ = try_run("schtasks", &["/Delete", "/TN", &task, "/F"]);
            let path = options.service_path();
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|error| format!("Cannot remove {}: {error}", path.display()))?;
            }
            let wrapper = windows_wrapper_path(options);
            if wrapper.exists() {
                let _ = std::fs::remove_file(wrapper);
            }
            // Stop a running instance only after the task and file are gone.
            let _ = try_run("schtasks", &["/End", "/TN", &task]);
            Ok(windows_status(options))
        }
    }
}

pub fn uninstall() -> Result<ServiceStatus, String> {
    let options = ServiceOptions::current()?;
    uninstall_with(&options)
}

/// Start the service after this process exits.
///
/// The two-second delay gives the app time to close its SQLite handles and
/// release the fleet lock before the service starts.
pub fn handoff_with(options: &ServiceOptions) -> Result<(), String> {
    match options.platform {
        ServicePlatform::LinuxSystemd => {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(format!(
                    "sleep 2; systemctl --user start {}",
                    systemd_unit(options)
                ))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|error| format!("Cannot schedule the service start: {error}"))?;
            Ok(())
        }
        ServicePlatform::MacosLaunchd => {
            let plist = options.service_path();
            let domain = format!("gui/{}", options.uid);
            std::process::Command::new("sh")
                .arg("-c")
                .arg(format!(
                    "sleep 2; launchctl bootstrap {} {}",
                    shell_quote(&domain),
                    shell_quote(&plist.to_string_lossy())
                ))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|error| format!("Cannot schedule the service start: {error}"))?;
            Ok(())
        }
        ServicePlatform::WindowsTaskScheduler => {
            std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-WindowStyle",
                    "Hidden",
                    "-Command",
                    &format!(
                        "Start-Sleep -Seconds 2; schtasks /Run /TN \"{}\"",
                        windows_task_name(options)
                    ),
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|error| format!("Cannot schedule the task start: {error}"))?;
            Ok(())
        }
        ServicePlatform::Unsupported => {
            Err("Background service installation is not implemented on this platform.".to_string())
        }
    }
}

pub fn handoff() -> Result<(), String> {
    let options = ServiceOptions::current()?;
    handoff_with(&options)
}

pub fn set_linger(enabled: bool) -> Result<ServiceStatus, String> {
    let options = ServiceOptions::current()?;
    if options.platform != ServicePlatform::LinuxSystemd {
        return Err("Lingering is only available on Linux.".to_string());
    }
    if enabled {
        run("loginctl", &["enable-linger", &options.user])?;
    } else {
        run("loginctl", &["disable-linger", &options.user])?;
    }
    Ok(linux_status(&options))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn home_dir() -> Result<PathBuf, String> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var(variable)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("Cannot determine the home directory ({variable})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(platform: ServicePlatform, home: &Path) -> ServiceOptions {
        ServiceOptions {
            platform,
            executable: home.join("bin/ravenbot"),
            home: home.to_path_buf(),
            config_home: home.join(".config"),
            data_root: home.join("RAVENBOT"),
            log_dir: home.join("RAVENBOT/logs"),
            uid: 1000,
            user: "tester".to_string(),
            service_name: SERVICE_NAME.to_string(),
            label: SERVICE_LABEL.to_string(),
            environment: Vec::new(),
        }
    }

    fn temp_home(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("ravenbot-service-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("temp home");
        path
    }

    #[test]
    fn the_systemd_unit_runs_the_headless_daemon_and_restarts_on_failure() {
        let home = temp_home("systemd");
        let unit = render_systemd_unit(&options(ServicePlatform::LinuxSystemd, &home));
        assert!(unit.contains("ExecStart=\""));
        assert!(unit.contains("daemon"));
        assert!(!unit.contains("--hidden"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(!unit.contains("graphical-session.target"));
        assert!(unit.contains("network-online.target"));
        assert!(!unit.contains("RAVENBOT_START_HIDDEN"));
        assert!(unit.contains("Environment=\"RAVENBOT_HOME="));
        assert!(unit.contains("NoNewPrivileges=true"));
        assert!(unit.contains("PrivateTmp=true"));
        assert!(unit.contains("ProtectSystem=full"));
        assert!(unit.contains("ProtectKernelTunables=true"));
        assert!(unit.contains("ProtectControlGroups=true"));
        assert!(unit.contains("LimitCORE=0"));
        // A clean tray quit is exit 0 and must not be restarted.
        assert!(!unit.contains("Restart=always"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn generated_unit_passes_systemd_analyze_verify() {
        let home = temp_home("systemd-verify");
        let mut options = options(ServicePlatform::LinuxSystemd, &home);
        options.executable = PathBuf::from("/bin/true");
        let unit = render_systemd_unit(&options);
        let path = home.join("ravenbot.service");
        std::fs::write(&path, unit).expect("write unit");

        match std::process::Command::new("systemd-analyze")
            .arg("verify")
            .arg(&path)
            .output()
        {
            Ok(output) => assert!(
                output.status.success(),
                "systemd-analyze verify failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("systemd-analyze unavailable; skipping unit verification");
            }
            Err(error) => panic!("could not run systemd-analyze: {error}"),
        }
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_per_account_unit_has_its_own_name_ports_and_keychain_namespace() {
        let home = temp_home("account-unit");
        let mut options = options(ServicePlatform::LinuxSystemd, &home);
        options.service_name = "ravenbot-alice".to_string();
        options.label = "dev.ravenbot.background.alice".to_string();
        options.environment = vec![
            (
                "RAVENBOT_KEYCHAIN_NAMESPACE".to_string(),
                "account-1".to_string(),
            ),
            ("RAVENBOT_WEBHOOK_PORT".to_string(), "8810".to_string()),
            ("RAVENBOT_CHAT_WEBHOOK_PORT".to_string(), "8811".to_string()),
            ("RAVENBOT_REMOTE_PORT".to_string(), "8812".to_string()),
        ];
        let unit = render_systemd_unit(&options);
        assert!(unit.contains("RAVENBOT_KEYCHAIN_NAMESPACE=account-1"));
        assert!(unit.contains("RAVENBOT_WEBHOOK_PORT=8810"));
        assert!(unit.contains("RAVENBOT_CHAT_WEBHOOK_PORT=8811"));
        assert!(unit.contains("RAVENBOT_REMOTE_PORT=8812"));
        assert!(options
            .service_path()
            .to_string_lossy()
            .ends_with("ravenbot-alice.service"));
        let plist = render_launchd_plist(&options);
        assert!(plist.contains("<string>dev.ravenbot.background.alice</string>"));
        assert!(plist.contains("<key>RAVENBOT_WEBHOOK_PORT</key>"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn systemd_paths_are_quoted_so_spaces_and_percent_survive() {
        let home = temp_home("quotes");
        let mut options = options(ServicePlatform::LinuxSystemd, &home);
        options.executable = home.join("my build/raven bot%1");
        let unit = render_systemd_unit(&options);
        assert!(unit.contains("raven bot%%1"));
        assert!(unit.contains("ExecStart=\""));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_launchagent_is_valid_plist_and_keeps_a_crashed_process_alive() {
        let home = temp_home("launchd");
        let plist = render_launchd_plist(&options(ServicePlatform::MacosLaunchd, &home));
        let parsed = plist::Value::from_reader(std::io::Cursor::new(plist.as_bytes()))
            .expect("valid launchd plist");
        assert!(matches!(parsed, plist::Value::Dictionary(_)));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<key>SuccessfulExit</key>"));
        assert!(plist.contains("<false/>"));
        assert!(plist.contains("<string>daemon</string>"));
        assert!(!plist.contains("--hidden"));
        assert!(!plist.contains("RAVENBOT_START_HIDDEN"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn launchd_values_are_xml_escaped() {
        let home = temp_home("xml");
        let mut options = options(ServicePlatform::MacosLaunchd, &home);
        options.executable = home.join("A & B <bot>.app");
        let plist = render_launchd_plist(&options);
        assert!(plist.contains("A &amp; B &lt;bot&gt;.app"));
        assert!(!plist.contains("A & B <bot>.app"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn installing_writes_only_when_the_unit_changed() {
        let home = temp_home("idempotent");
        let path = home.join(".config/systemd/user/ravenbot.service");
        assert!(write_if_changed(&path, "one").expect("write"));
        assert!(!write_if_changed(&path, "one").expect("rewrite"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one");
        assert!(write_if_changed(&path, "two").expect("change"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_windows_task_runs_the_daemon_at_logon_as_a_limited_user() {
        let home = temp_home("windows");
        let xml = render_windows_task_xml(&options(ServicePlatform::WindowsTaskScheduler, &home));
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains("<Arguments>daemon</Arguments>"));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(xml.contains("<Hidden>true</Hidden>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<RestartOnFailure>"));
        assert!(!xml.contains("--hidden"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_service_path_matches_each_platform_layout() {
        let home = temp_home("paths");
        let linux = options(ServicePlatform::LinuxSystemd, &home);
        assert!(linux
            .service_path()
            .ends_with(".config/systemd/user/ravenbot.service"));
        let mac = options(ServicePlatform::MacosLaunchd, &home);
        assert!(mac
            .service_path()
            .ends_with("Library/LaunchAgents/dev.ravenbot.background.plist"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn unsupported_platforms_report_their_limit_instead_of_pretending() {
        let home = temp_home("unsupported");
        let options = options(ServicePlatform::Unsupported, &home);
        let status = status_with(&options);
        assert!(!status.supported);
        assert!(status.detail.contains("not implemented"));
        assert!(render(&options).is_err());
        let _ = std::fs::remove_dir_all(home);
    }
}
