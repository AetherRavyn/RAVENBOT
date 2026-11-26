//! Clipboard read/write with detected, platform-appropriate backends.
//!
//! * **Linux Wayland** — `wl-paste` / `wl-copy`
//! * **Linux X11** — `xclip -selection clipboard`, then `xsel` if `xclip` is
//!   absent. The X11 fallback is also used under XWayland when the native
//!   tools are missing.
//! * **macOS** — `pbpaste` / `pbcopy`
//! * **Windows** — PowerShell `Get-Clipboard -Raw` / `Set-Clipboard`, with
//!   explicit UTF-8 console encoding.
//!
//! Planning is pure and testable; execution never pretends a missing backend
//! worked. Clipboard contents are capped and are never written to activity or
//! log messages by this module.

use crate::platform::{find_program, Platform};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Maximum bytes read from or accepted into the clipboard. The clipboard can
/// hold secrets and huge documents; 64 KiB keeps a read bounded without
/// truncating ordinary text.
pub const MAX_CLIPBOARD_BYTES: usize = 64 * 1024;

/// A clipboard helper that never exits (a broken selection owner) must not
/// wedge the agent loop. Ten seconds is generous for a text clipboard.
const CLIPBOARD_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    WlPaste,
    WlCopy,
    Xclip,
    Xsel,
    Pbpaste,
    Pbcopy,
    PowerShell,
}

impl Tool {
    fn binary(self) -> &'static str {
        match self {
            Tool::WlPaste => "wl-paste",
            Tool::WlCopy => "wl-copy",
            Tool::Xclip => "xclip",
            Tool::Xsel => "xsel",
            Tool::Pbpaste => "pbpaste",
            Tool::Pbcopy => "pbcopy",
            Tool::PowerShell => "powershell",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Program {
    tool: Tool,
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardEnvironment {
    pub wayland: bool,
    pub x11: bool,
}

impl ClipboardEnvironment {
    pub fn detect() -> Self {
        let wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|value| value.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
            || std::env::var("WAYLAND_DISPLAY")
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
        let x11 = std::env::var("DISPLAY")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        Self { wayland, x11 }
    }
}

#[derive(Debug, Clone)]
pub struct Clipboard {
    read: Option<Program>,
    write: Option<Program>,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::detect()
    }
}

impl Clipboard {
    pub fn detect() -> Self {
        Self::detect_for(
            Platform::current(),
            ClipboardEnvironment::detect(),
            find_program,
        )
    }

    /// Injectable detection for tests. The finder receives a bare program
    /// name; a backend is present only when the finder returns a path.
    pub fn detect_for<F>(platform: Platform, environment: ClipboardEnvironment, find: F) -> Self
    where
        F: Fn(&str) -> Option<PathBuf>,
    {
        let mut clipboard = Self {
            read: None,
            write: None,
        };
        match platform {
            Platform::Linux => {
                if environment.wayland {
                    clipboard.read = find("wl-paste").map(|path| Program {
                        tool: Tool::WlPaste,
                        path,
                    });
                    clipboard.write = find("wl-copy").map(|path| Program {
                        tool: Tool::WlCopy,
                        path,
                    });
                }
                if environment.x11 {
                    if clipboard.read.is_none() {
                        clipboard.read = find("xclip")
                            .map(|path| Program {
                                tool: Tool::Xclip,
                                path,
                            })
                            .or_else(|| {
                                find("xsel").map(|path| Program {
                                    tool: Tool::Xsel,
                                    path,
                                })
                            });
                    }
                    if clipboard.write.is_none() {
                        clipboard.write = find("xclip")
                            .map(|path| Program {
                                tool: Tool::Xclip,
                                path,
                            })
                            .or_else(|| {
                                find("xsel").map(|path| Program {
                                    tool: Tool::Xsel,
                                    path,
                                })
                            });
                    }
                }
            }
            Platform::MacOs => {
                clipboard.read = find("pbpaste").map(|path| Program {
                    tool: Tool::Pbpaste,
                    path,
                });
                clipboard.write = find("pbcopy").map(|path| Program {
                    tool: Tool::Pbcopy,
                    path,
                });
            }
            Platform::Windows => {
                let shell = find("powershell").or_else(|| find("pwsh"));
                if let Some(path) = shell {
                    clipboard.read = Some(Program {
                        tool: Tool::PowerShell,
                        path: path.clone(),
                    });
                    clipboard.write = Some(Program {
                        tool: Tool::PowerShell,
                        path,
                    });
                }
            }
            Platform::Other => {}
        }
        clipboard
    }

    pub fn read_available(&self) -> bool {
        self.read.is_some()
    }

    pub fn write_available(&self) -> bool {
        self.write.is_some()
    }

    pub fn is_available(&self) -> bool {
        self.read_available() && self.write_available()
    }

    pub fn backend_summary(&self) -> String {
        let name = |program: &Option<Program>| {
            program
                .as_ref()
                .map(|program| program.tool.binary())
                .unwrap_or("none")
        };
        format!("read={} write={}", name(&self.read), name(&self.write))
    }

    pub fn read_backend(&self) -> String {
        self.read
            .as_ref()
            .map(|program| program.tool.binary().to_string())
            .unwrap_or_else(|| "none".to_string())
    }

    pub fn write_backend(&self) -> String {
        self.write
            .as_ref()
            .map(|program| program.tool.binary().to_string())
            .unwrap_or_else(|| "none".to_string())
    }

    pub(crate) fn read_plan(&self) -> Result<ClipboardCommand, String> {
        let program = self
            .read
            .as_ref()
            .ok_or("No clipboard reader is available on this machine")?;
        let args: Vec<String> = match program.tool {
            Tool::WlPaste => vec!["--no-newline".to_string()],
            Tool::Xclip => vec![
                "-selection".to_string(),
                "clipboard".to_string(),
                "-o".to_string(),
            ],
            Tool::Xsel => vec!["--clipboard".to_string(), "--output".to_string()],
            Tool::Pbpaste => Vec::new(),
            Tool::PowerShell => vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; Get-Clipboard -Raw"
                    .to_string(),
            ],
            Tool::WlCopy | Tool::Pbcopy => {
                return Err(format!(
                    "{} cannot read the clipboard",
                    program.tool.binary()
                ))
            }
        };
        Ok(ClipboardCommand {
            program: program.path.display().to_string(),
            args,
            stdin: None,
        })
    }

    pub(crate) fn write_plan(&self, text: &str) -> Result<ClipboardCommand, String> {
        validate_clipboard_text(text)?;
        let program = self
            .write
            .as_ref()
            .ok_or("No clipboard writer is available on this machine")?;
        let args: Vec<String> = match program.tool {
            Tool::WlCopy => Vec::new(),
            Tool::Xclip => vec![
                "-selection".to_string(),
                "clipboard".to_string(),
                "-i".to_string(),
            ],
            Tool::Xsel => vec!["--clipboard".to_string(), "--input".to_string()],
            Tool::Pbcopy => Vec::new(),
            Tool::PowerShell => vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                "[Console]::InputEncoding = [System.Text.Encoding]::UTF8; $text = [Console]::In.ReadToEnd(); Set-Clipboard -Value $text".to_string(),
            ],
            Tool::WlPaste | Tool::Pbpaste => {
                return Err(format!(
                    "{} cannot write the clipboard",
                    program.tool.binary()
                ))
            }
        };
        Ok(ClipboardCommand {
            program: program.path.display().to_string(),
            args,
            stdin: Some(text.to_string()),
        })
    }

    /// Read the clipboard as UTF-8 text. Output is capped at
    /// [`MAX_CLIPBOARD_BYTES`] on a character boundary.
    pub async fn read_text(&self) -> Result<ClipboardRead, String> {
        let plan = self.read_plan()?;
        let mut command = Command::new(&plan.program);
        command
            .args(&plan.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let output = tokio::time::timeout(CLIPBOARD_COMMAND_TIMEOUT, command.output())
            .await
            .map_err(|_| format!("{} timed out reading the clipboard", plan.program))?
            .map_err(|error| format!("failed to run {}: {error}", plan.program))?;
        if !output.status.success() {
            return Err(command_failure(&plan.program, &output.stderr));
        }
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let bytes = raw.len();
        let (text, truncated) = cap_text(raw);
        Ok(ClipboardRead {
            text,
            bytes,
            truncated,
            backend: self.read_backend(),
        })
    }

    /// Replace the clipboard with text. Oversized input is rejected rather
    /// than silently truncated.
    pub async fn write_text(&self, text: &str) -> Result<ClipboardWrite, String> {
        let plan = self.write_plan(text)?;
        let mut child = Command::new(&plan.program)
            .args(&plan.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("failed to run {}: {error}", plan.program))?;
        if let Some(stdin) = plan.stdin {
            let mut pipe = child
                .stdin
                .take()
                .ok_or_else(|| format!("{} did not accept input", plan.program))?;
            pipe.write_all(stdin.as_bytes())
                .await
                .map_err(|error| format!("failed to write to {}: {error}", plan.program))?;
            pipe.shutdown()
                .await
                .map_err(|error| format!("failed to close {} input: {error}", plan.program))?;
        }
        let status = tokio::time::timeout(CLIPBOARD_COMMAND_TIMEOUT, child.wait())
            .await
            .map_err(|_| format!("{} timed out writing the clipboard", plan.program))?
            .map_err(|error| format!("failed to wait for {}: {error}", plan.program))?;
        if !status.success() {
            return Err(format!("{} failed: exited with {status}", plan.program));
        }
        Ok(ClipboardWrite {
            bytes: text.len(),
            backend: self.write_backend(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClipboardCommand {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Option<String>,
}

impl ClipboardCommand {
    pub fn describe(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardRead {
    pub text: String,
    /// Size before capping, in bytes.
    pub bytes: usize,
    pub truncated: bool,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardWrite {
    pub bytes: usize,
    pub backend: String,
}

fn validate_clipboard_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err(format!(
            "clipboard text is too long ({} bytes; max {MAX_CLIPBOARD_BYTES})",
            text.len()
        ));
    }
    if text.contains('\0') {
        return Err("clipboard text must not contain NUL characters".to_string());
    }
    Ok(())
}

fn cap_text(mut text: String) -> (String, bool) {
    if text.len() <= MAX_CLIPBOARD_BYTES {
        return (text, false);
    }
    let mut end = MAX_CLIPBOARD_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, true)
}

fn command_failure(program: &str, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    format!(
        "{} failed: {}",
        program,
        stderr.trim().lines().next().unwrap_or("unknown error")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(tool: Tool, path: &str) -> Program {
        Program {
            tool,
            path: PathBuf::from(path),
        }
    }

    fn clipboard(read: Option<Program>, write: Option<Program>) -> Clipboard {
        Clipboard { read, write }
    }

    #[test]
    fn wayland_prefers_wl_paste_and_wl_copy() {
        let clipboard = Clipboard::detect_for(
            Platform::Linux,
            ClipboardEnvironment {
                wayland: true,
                x11: true,
            },
            |name| match name {
                "wl-paste" => Some(PathBuf::from("/usr/bin/wl-paste")),
                "wl-copy" => Some(PathBuf::from("/usr/bin/wl-copy")),
                "xclip" => Some(PathBuf::from("/usr/bin/xclip")),
                _ => None,
            },
        );
        assert_eq!(clipboard.backend_summary(), "read=wl-paste write=wl-copy");
        assert_eq!(clipboard.read_plan().unwrap().args, vec!["--no-newline"]);
        assert!(clipboard.write_plan("hello").unwrap().args.is_empty());
    }

    #[test]
    fn x11_uses_xclip_with_the_clipboard_selection() {
        let clipboard = Clipboard::detect_for(
            Platform::Linux,
            ClipboardEnvironment {
                wayland: false,
                x11: true,
            },
            |name| (name == "xclip").then(|| PathBuf::from("/usr/bin/xclip")),
        );
        let read = clipboard.read_plan().unwrap();
        assert!(read.program.ends_with("xclip"));
        assert_eq!(read.args, vec!["-selection", "clipboard", "-o"]);
        let write = clipboard.write_plan("hello").unwrap();
        assert_eq!(write.args, vec!["-selection", "clipboard", "-i"]);
        assert_eq!(write.stdin.as_deref(), Some("hello"));
    }

    #[test]
    fn x11_falls_back_to_xsel_when_xclip_is_absent() {
        let clipboard = Clipboard::detect_for(
            Platform::Linux,
            ClipboardEnvironment {
                wayland: false,
                x11: true,
            },
            |name| (name == "xsel").then(|| PathBuf::from("/usr/bin/xsel")),
        );
        assert_eq!(clipboard.backend_summary(), "read=xsel write=xsel");
        assert_eq!(
            clipboard.read_plan().unwrap().args,
            vec!["--clipboard", "--output"]
        );
        assert_eq!(
            clipboard.write_plan("hello").unwrap().args,
            vec!["--clipboard", "--input"]
        );
    }

    #[test]
    fn macos_uses_pbpaste_and_pbcopy() {
        let clipboard = Clipboard::detect_for(
            Platform::MacOs,
            ClipboardEnvironment {
                wayland: false,
                x11: false,
            },
            |name| match name {
                "pbpaste" => Some(PathBuf::from("/usr/bin/pbpaste")),
                "pbcopy" => Some(PathBuf::from("/usr/bin/pbcopy")),
                _ => None,
            },
        );
        assert_eq!(clipboard.backend_summary(), "read=pbpaste write=pbcopy");
        assert!(clipboard.read_plan().unwrap().args.is_empty());
        assert!(clipboard.write_plan("hello").unwrap().args.is_empty());
    }

    #[test]
    fn windows_uses_powershell_with_utf8_and_raw_text() {
        let clipboard = Clipboard::detect_for(
            Platform::Windows,
            ClipboardEnvironment {
                wayland: false,
                x11: false,
            },
            |name| {
                (name == "powershell").then(|| {
                    PathBuf::from("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
                })
            },
        );
        assert_eq!(
            clipboard.backend_summary(),
            "read=powershell write=powershell"
        );
        let read = clipboard.read_plan().unwrap();
        assert!(read.program.ends_with("powershell.exe"));
        assert!(read
            .args
            .iter()
            .any(|arg| arg.contains("Get-Clipboard -Raw")));
        assert!(read.args.iter().any(|arg| arg.contains("OutputEncoding")));
        let write = clipboard.write_plan("hello").unwrap();
        assert!(write.args.iter().any(|arg| arg.contains("Set-Clipboard")));
        assert!(write.args.iter().any(|arg| arg.contains("InputEncoding")));
        assert_eq!(write.stdin.as_deref(), Some("hello"));
    }

    #[test]
    fn no_backend_is_an_explicit_error() {
        let clipboard = clipboard(None, None);
        assert!(!clipboard.is_available());
        assert_eq!(clipboard.backend_summary(), "read=none write=none");
        assert!(clipboard.read_plan().is_err());
        assert!(clipboard.write_plan("hello").is_err());
    }

    #[test]
    fn write_rejects_oversized_and_nul_text() {
        let clipboard = clipboard(None, Some(program(Tool::WlCopy, "/usr/bin/wl-copy")));
        let long = "x".repeat(MAX_CLIPBOARD_BYTES + 1);
        assert!(clipboard.write_plan(&long).is_err());
        assert!(clipboard.write_plan("a\0b").is_err());
    }

    #[test]
    fn read_caps_on_a_character_boundary() {
        let value = "é".repeat(MAX_CLIPBOARD_BYTES);
        let (text, truncated) = cap_text(value);
        assert!(truncated);
        assert!(text.len() <= MAX_CLIPBOARD_BYTES);
        assert!(text.chars().all(|character| character == 'é'));
        let (unchanged, truncated) = cap_text("hello".to_string());
        assert!(!truncated);
        assert_eq!(unchanged, "hello");
    }

    #[tokio::test]
    #[ignore = "requires a live desktop clipboard owner; opt in with RAVENBOT_LIVE_CLIPBOARD=1"]
    async fn live_clipboard_read_is_opt_in() {
        if std::env::var("RAVENBOT_LIVE_CLIPBOARD").as_deref() != Ok("1") {
            return;
        }
        let clipboard = Clipboard::detect();
        assert!(!clipboard.backend_summary().is_empty());
        // A read may legitimately fail when nothing owns the selection; the
        // point of this opt-in check is to exercise the real backend.
        let _ = clipboard.read_text().await;
    }

    #[test]
    fn detection_reports_a_well_formed_backend_summary() {
        let clipboard = Clipboard::detect();
        assert!(clipboard.backend_summary().starts_with("read="));
        eprintln!(
            "detected clipboard backend: {}",
            clipboard.backend_summary()
        );
    }
}
