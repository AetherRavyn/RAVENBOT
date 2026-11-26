//! Small platform helpers shared by the desktop-control backends.
//!
//! Detection is explicit: callers can ask for the current platform or inject a
//! platform and program finder in tests. Nothing here assumes a tool exists
//! because the platform normally ships it.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
    Other,
}

impl Platform {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "linux" => Platform::Linux,
            "macos" => Platform::MacOs,
            "windows" => Platform::Windows,
            _ => Platform::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::MacOs => "macos",
            Platform::Windows => "windows",
            Platform::Other => "unsupported",
        }
    }
}

/// Find a program on `PATH`.
///
/// On Windows the usual executable extensions are tried because `PATH`
/// entries contain `powershell.exe`, not `powershell`.
pub(crate) fn find_program(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        #[cfg(windows)]
        {
            let has_extension = std::path::Path::new(name).extension().is_some();
            let candidates = if has_extension {
                vec![dir.join(name)]
            } else {
                ["exe", "cmd", "bat", "com"]
                    .iter()
                    .map(|extension| dir.join(format!("{name}.{extension}")))
                    .collect()
            };
            for candidate in candidates {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
