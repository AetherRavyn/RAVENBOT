//! Launching an application, document, or URL without a shell.
//!
//! The backend is chosen at runtime and the argv plan is separated from the
//! spawn so the choice can be tested without starting real applications:
//!
//! * **Linux** — `gtk-launch` for desktop ids/app names, `xdg-open` for files
//!   and URLs, and a direct `exec` of the target when neither helper exists.
//! * **macOS** — `open -a` for an application name, `open` for a file or URL.
//! * **Windows** — the executable is started directly; paths and URLs go
//!   through `explorer.exe`.
//!
//! Honesty contract: an unsupported platform, an empty/oversized target, or a
//! URL with no handler returns an error. The process is spawned with arguments
//! (never `sh -c`/`cmd /C`), and the call returns after spawn; it does not
//! claim the application reached a usable state.

use crate::platform::{find_program, Platform};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// Longest target accepted, in bytes. Longer values are not a path or URL a
/// desktop launcher can meaningfully open.
pub const MAX_LAUNCH_TARGET_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStrategy {
    GtkLaunch,
    XdgOpen,
    Direct,
    MacOpenApp,
    MacOpen,
    WindowsDirect,
    WindowsExplorer,
}

impl LaunchStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            LaunchStrategy::GtkLaunch => "gtk-launch",
            LaunchStrategy::XdgOpen => "xdg-open",
            LaunchStrategy::Direct => "direct",
            LaunchStrategy::MacOpenApp => "open -a",
            LaunchStrategy::MacOpen => "open",
            LaunchStrategy::WindowsDirect => "direct",
            LaunchStrategy::WindowsExplorer => "explorer.exe",
        }
    }
}

/// A validated command to run. No shell is involved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub program: String,
    pub args: Vec<String>,
    pub strategy: LaunchStrategy,
}

impl LaunchPlan {
    pub fn describe(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

/// What actually happened: the process was created, not that the app is ready.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchOutcome {
    pub pid: u32,
    pub strategy: String,
    pub program: String,
    pub args: Vec<String>,
}

/// Detected launch helpers. A missing helper is recorded as `None`, never
/// assumed because the platform typically has it.
#[derive(Debug, Clone)]
pub struct AppLauncher {
    platform: Platform,
    gtk_launch: Option<PathBuf>,
    xdg_open: Option<PathBuf>,
    mac_open: Option<PathBuf>,
    windows_explorer: Option<PathBuf>,
}

impl Default for AppLauncher {
    fn default() -> Self {
        Self::detect()
    }
}

impl AppLauncher {
    pub fn detect() -> Self {
        Self::detect_for(Platform::current(), find_program)
    }

    /// Injectable detection for tests and for callers that already know the
    /// platform. The finder receives a bare program name.
    pub fn detect_for<F>(platform: Platform, find: F) -> Self
    where
        F: Fn(&str) -> Option<PathBuf>,
    {
        let mac_open = if platform == Platform::MacOs {
            find("open").or_else(|| Some(PathBuf::from("/usr/bin/open")))
        } else {
            None
        };
        Self {
            platform,
            gtk_launch: if platform == Platform::Linux {
                find("gtk-launch")
            } else {
                None
            },
            xdg_open: if platform == Platform::Linux {
                find("xdg-open")
            } else {
                None
            },
            mac_open,
            windows_explorer: if platform == Platform::Windows {
                find("explorer.exe").or(Some(PathBuf::from("explorer.exe")))
            } else {
                None
            },
        }
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn is_supported(&self) -> bool {
        self.platform != Platform::Other
    }

    /// The helper that would be preferred on this machine. `direct` means the
    /// target itself is started; `unsupported` means no launch path exists.
    pub fn backend_summary(&self) -> String {
        match self.platform {
            Platform::Linux => {
                if self.gtk_launch.is_some() {
                    "gtk-launch".to_string()
                } else if self.xdg_open.is_some() {
                    "xdg-open".to_string()
                } else {
                    "direct".to_string()
                }
            }
            Platform::MacOs => "open".to_string(),
            Platform::Windows => "explorer.exe".to_string(),
            Platform::Other => "unsupported".to_string(),
        }
    }

    /// Pick a launch strategy. This is pure apart from the detected helper
    /// paths; it does not check that an application exists.
    pub fn plan(&self, target: &str) -> Result<LaunchPlan, String> {
        let target = validate_target(target)?;
        match self.platform {
            Platform::Linux => self.plan_linux(target),
            Platform::MacOs => self.plan_macos(target),
            Platform::Windows => self.plan_windows(target),
            Platform::Other => Err(format!(
                "Application launch is not implemented on this platform ({})",
                self.platform.as_str()
            )),
        }
    }

    fn plan_linux(&self, target: &str) -> Result<LaunchPlan, String> {
        if is_url(target) {
            return match &self.xdg_open {
                Some(path) => Ok(LaunchPlan {
                    program: path.display().to_string(),
                    args: vec![target.to_string()],
                    strategy: LaunchStrategy::XdgOpen,
                }),
                None => Err(
                    "No URL handler is available. Install `xdg-open` to launch a URL.".to_string(),
                ),
            };
        }
        if looks_like_desktop_id(target) {
            if let Some(path) = &self.gtk_launch {
                return Ok(LaunchPlan {
                    program: path.display().to_string(),
                    args: vec![target.to_string()],
                    strategy: LaunchStrategy::GtkLaunch,
                });
            }
        }
        if let Some(path) = &self.xdg_open {
            return Ok(LaunchPlan {
                program: path.display().to_string(),
                args: vec![target.to_string()],
                strategy: LaunchStrategy::XdgOpen,
            });
        }
        Ok(LaunchPlan {
            program: target.to_string(),
            args: Vec::new(),
            strategy: LaunchStrategy::Direct,
        })
    }

    fn plan_macos(&self, target: &str) -> Result<LaunchPlan, String> {
        let open = self
            .mac_open
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "open".to_string());
        if is_url(target) || looks_like_path(target) {
            Ok(LaunchPlan {
                program: open,
                args: vec![target.to_string()],
                strategy: LaunchStrategy::MacOpen,
            })
        } else {
            Ok(LaunchPlan {
                program: open,
                args: vec!["-a".to_string(), target.to_string()],
                strategy: LaunchStrategy::MacOpenApp,
            })
        }
    }

    fn plan_windows(&self, target: &str) -> Result<LaunchPlan, String> {
        if !is_url(target) && is_windows_executable(target) {
            return Ok(LaunchPlan {
                program: target.to_string(),
                args: Vec::new(),
                strategy: LaunchStrategy::WindowsDirect,
            });
        }
        if is_url(target) || looks_like_path(target) {
            let explorer = self
                .windows_explorer
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "explorer.exe".to_string());
            Ok(LaunchPlan {
                program: explorer,
                args: vec![target.to_string()],
                strategy: LaunchStrategy::WindowsExplorer,
            })
        } else {
            Ok(LaunchPlan {
                program: target.to_string(),
                args: Vec::new(),
                strategy: LaunchStrategy::WindowsDirect,
            })
        }
    }

    /// Spawn the planned command and return immediately. A detached thread
    /// reaps the child so a long-lived app does not leave a zombie.
    pub fn launch(&self, target: &str) -> Result<LaunchOutcome, String> {
        let plan = self.plan(target)?;
        let mut command = Command::new(&plan.program);
        command
            .args(&plan.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command
            .spawn()
            .map_err(|error| format!("failed to launch {}: {error}", plan.program))?;
        let pid = child.id();
        reap_in_background(child);
        Ok(LaunchOutcome {
            pid,
            strategy: plan.strategy.as_str().to_string(),
            program: plan.program,
            args: plan.args,
        })
    }
}

fn reap_in_background(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

fn validate_target(target: &str) -> Result<&str, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("app_launch target must not be empty".to_string());
    }
    if target.len() > MAX_LAUNCH_TARGET_BYTES {
        return Err(format!(
            "app_launch target is too long (max {MAX_LAUNCH_TARGET_BYTES} bytes)"
        ));
    }
    if target.chars().any(char::is_control) {
        return Err("app_launch target must not contain control characters".to_string());
    }
    // Without a `--` guard in every helper, a leading dash could be parsed as
    // a launcher option instead of a target.
    if target.starts_with('-') {
        return Err("app_launch target must not start with '-'".to_string());
    }
    Ok(target)
}

fn is_url(target: &str) -> bool {
    let Some(colon) = target.find(':') else {
        return false;
    };
    let scheme = &target[..colon];
    scheme.len() > 1
        && scheme
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

fn is_windows_executable(target: &str) -> bool {
    let lowercase = target.to_ascii_lowercase();
    [".exe", ".com", ".bat", ".cmd"]
        .iter()
        .any(|extension| lowercase.ends_with(extension))
}

fn looks_like_path(target: &str) -> bool {
    target.contains('/')
        || target.contains('\\')
        || target.starts_with('.')
        || target.starts_with('~')
}

fn looks_like_desktop_id(target: &str) -> bool {
    target.ends_with(".desktop") || (!is_url(target) && !looks_like_path(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher(platform: Platform, gtk: Option<&str>, xdg: Option<&str>) -> AppLauncher {
        AppLauncher::detect_for(platform, |name| match (name, gtk, xdg) {
            ("gtk-launch", Some(path), _) => Some(PathBuf::from(path)),
            ("xdg-open", _, Some(path)) => Some(PathBuf::from(path)),
            _ => None,
        })
    }

    #[test]
    fn linux_prefers_gtk_launch_for_a_desktop_id() {
        let launcher = launcher(
            Platform::Linux,
            Some("/usr/bin/gtk-launch"),
            Some("/usr/bin/xdg-open"),
        );
        let plan = launcher.plan("org.gnome.Calculator.desktop").unwrap();
        assert_eq!(plan.strategy, LaunchStrategy::GtkLaunch);
        assert_eq!(plan.program, "/usr/bin/gtk-launch");
        assert_eq!(plan.args, vec!["org.gnome.Calculator.desktop"]);
    }

    #[test]
    fn linux_uses_xdg_open_for_urls_and_paths_when_gtk_launch_exists() {
        let launcher = launcher(
            Platform::Linux,
            Some("/usr/bin/gtk-launch"),
            Some("/usr/bin/xdg-open"),
        );
        for target in ["https://example.com/report", "/tmp/report.pdf"] {
            let plan = launcher.plan(target).unwrap();
            assert_eq!(plan.strategy, LaunchStrategy::XdgOpen, "{target}");
            assert_eq!(plan.args, vec![target]);
        }
    }

    #[test]
    fn linux_falls_back_to_xdg_open_then_direct() {
        let xdg = launcher(Platform::Linux, None, Some("/usr/bin/xdg-open"));
        let plan = xdg.plan("firefox").unwrap();
        assert_eq!(plan.strategy, LaunchStrategy::XdgOpen);
        assert_eq!(plan.args, vec!["firefox"]);

        let direct = launcher(Platform::Linux, None, None);
        let plan = direct.plan("/opt/app/bin/app").unwrap();
        assert_eq!(plan.strategy, LaunchStrategy::Direct);
        assert_eq!(plan.program, "/opt/app/bin/app");
        assert!(plan.args.is_empty());
    }

    #[test]
    fn linux_url_without_a_handler_is_an_error() {
        let launcher = launcher(Platform::Linux, None, None);
        let error = launcher.plan("https://example.com").unwrap_err();
        assert!(error.contains("xdg-open"));
    }

    #[test]
    fn macos_distinguishes_app_names_from_paths_and_urls() {
        let launcher = AppLauncher::detect_for(Platform::MacOs, |_| None);
        let app = launcher.plan("Safari").unwrap();
        assert_eq!(app.program, "/usr/bin/open");
        assert_eq!(app.args, vec!["-a", "Safari"]);
        assert_eq!(app.strategy, LaunchStrategy::MacOpenApp);

        let file = launcher.plan("/Users/me/report.pdf").unwrap();
        assert_eq!(file.args, vec!["/Users/me/report.pdf"]);
        assert_eq!(file.strategy, LaunchStrategy::MacOpen);

        let url = launcher.plan("https://example.com").unwrap();
        assert_eq!(url.args, vec!["https://example.com"]);
        assert_eq!(url.strategy, LaunchStrategy::MacOpen);
    }

    #[test]
    fn windows_runs_executables_and_delegates_paths_and_urls_to_explorer() {
        let launcher = AppLauncher::detect_for(Platform::Windows, |_| None);
        let exe = launcher.plan("C:\\Tools\\app.exe").unwrap();
        assert_eq!(exe.strategy, LaunchStrategy::WindowsDirect);
        assert_eq!(exe.program, "C:\\Tools\\app.exe");

        let bare = launcher.plan("notepad").unwrap();
        assert_eq!(bare.strategy, LaunchStrategy::WindowsDirect);
        assert_eq!(bare.program, "notepad");

        let document = launcher.plan("C:\\Users\\me\\report.pdf").unwrap();
        assert_eq!(document.strategy, LaunchStrategy::WindowsExplorer);
        assert_eq!(document.program, "explorer.exe");

        let url = launcher.plan("https://example.com").unwrap();
        assert_eq!(url.strategy, LaunchStrategy::WindowsExplorer);
        assert_eq!(url.args, vec!["https://example.com"]);
    }

    #[test]
    fn target_validation_rejects_empty_control_and_oversized_values() {
        let launcher = launcher(Platform::Linux, Some("/usr/bin/gtk-launch"), None);
        assert!(launcher.plan("   ").is_err());
        assert!(launcher.plan("bad\0target").is_err());
        assert!(launcher.plan("line\nbreak").is_err());
        assert!(launcher.plan("-a").is_err());
        let long = "x".repeat(MAX_LAUNCH_TARGET_BYTES + 1);
        assert!(launcher.plan(&long).is_err());
    }

    #[test]
    fn unsupported_platform_is_an_explicit_error() {
        let launcher = AppLauncher::detect_for(Platform::Other, |_| None);
        assert!(!launcher.is_supported());
        let error = launcher.plan("anything").unwrap_err();
        assert!(error.contains("not implemented"));
    }

    #[test]
    fn backend_summary_reports_the_preferred_helper() {
        let gtk = launcher(
            Platform::Linux,
            Some("/usr/bin/gtk-launch"),
            Some("/usr/bin/xdg-open"),
        );
        assert_eq!(gtk.backend_summary(), "gtk-launch");
        let xdg = launcher(Platform::Linux, None, Some("/usr/bin/xdg-open"));
        assert_eq!(xdg.backend_summary(), "xdg-open");
        let direct = launcher(Platform::Linux, None, None);
        assert_eq!(direct.backend_summary(), "direct");
    }

    #[test]
    #[ignore = "spawns a real application; set RAVENBOT_LIVE_LAUNCH_TARGET and run explicitly"]
    fn live_app_launch_is_opt_in() {
        let Some(target) = std::env::var("RAVENBOT_LIVE_LAUNCH_TARGET").ok() else {
            return;
        };
        let launcher = AppLauncher::detect();
        let outcome = launcher.launch(&target).expect("launch");
        assert!(outcome.pid > 0);
    }

    #[test]
    fn detection_reports_a_well_formed_backend_summary() {
        let launcher = AppLauncher::detect();
        assert!(!launcher.backend_summary().is_empty());
        eprintln!(
            "detected app launch backend: {} ({})",
            launcher.backend_summary(),
            launcher.platform().as_str()
        );
    }
}
