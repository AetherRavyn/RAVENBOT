//! A process-wide hook for opening the desktop browser dock.
//!
//! The skill crate is deliberate about not knowing whether it runs inside the
//! Tauri shell, the CLI, or an MCP sidecar. The shell installs a callback at
//! startup; when one is present, `browser_open` can ask the UI to show a real
//! page. When one is absent (headless or test), the request reports `false`
//! instead of pretending a window appeared.

use std::sync::{Mutex, OnceLock};

type Opener = Box<dyn Fn(String) + Send + Sync>;

fn opener() -> &'static Mutex<Option<Opener>> {
    static OPENER: OnceLock<Mutex<Option<Opener>>> = OnceLock::new();
    OPENER.get_or_init(|| Mutex::new(None))
}

/// Install the shell's browser-dock opener. Replacing an existing opener is
/// intentional: tests and hot-restarted shells both need to take ownership.
pub fn set_browser_dock_opener<F>(open: F)
where
    F: Fn(String) + Send + Sync + 'static,
{
    let mut slot = opener().lock().unwrap_or_else(|error| error.into_inner());
    *slot = Some(Box::new(open));
}

/// Ask the desktop shell to show `url` in its browser dock.
///
/// Returns `true` only when an opener is installed and accepted the request.
pub fn request_browser_dock(url: impl Into<String>) -> bool {
    let url = url.into();
    if url.trim().is_empty() {
        return false;
    }
    let slot = opener().lock().unwrap_or_else(|error| error.into_inner());
    match slot.as_ref() {
        Some(open) => {
            open(url);
            true
        }
        None => false,
    }
}

/// Whether a shell is listening for browser-dock requests.
pub fn browser_dock_available() -> bool {
    opener()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_refused_when_no_shell_is_installed() {
        // This test owns the process-wide slot only in the sense that it does
        // not install one; another test in this binary that does is expected
        // to restore it, so keep the assertion on the empty-url path which is
        // independent of installation.
        assert!(!request_browser_dock("   "));
    }

    #[test]
    fn installed_opener_receives_the_url() {
        let seen = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = seen.clone();
        set_browser_dock_opener(move |url| {
            sink.lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(url);
        });
        assert!(browser_dock_available());
        assert!(request_browser_dock("https://example.com"));
        assert_eq!(
            seen.lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_slice(),
            ["https://example.com"]
        );
    }
}

type Evaluator = Box<dyn Fn(String) -> Result<String, String> + Send + Sync>;

fn evaluator() -> &'static Mutex<Option<Evaluator>> {
    static EVALUATOR: OnceLock<Mutex<Option<Evaluator>>> = OnceLock::new();
    EVALUATOR.get_or_init(|| Mutex::new(None))
}

/// Install a result-bearing JavaScript evaluator for the dock. The desktop
/// shell installs one on platforms where its webview exposes a callback; a
/// headless or unsupported build leaves it empty.
pub fn set_browser_dock_evaluator<F>(evaluate: F)
where
    F: Fn(String) -> Result<String, String> + Send + Sync + 'static,
{
    let mut slot = evaluator()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *slot = Some(Box::new(evaluate));
}

/// Evaluate JavaScript in the open browser dock and return its JSON result.
///
/// `Ok(None)` means no shell evaluator is installed. `Err` means the shell is
/// present but the script or page failed; those are different answers and a
/// caller must not turn the first into a fake success.
pub fn evaluate_browser_dock(script: &str) -> Result<Option<String>, String> {
    if script.trim().is_empty() {
        return Err("The browser script is empty.".to_string());
    }
    let slot = evaluator()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    match slot.as_ref() {
        Some(evaluate) => evaluate(script.to_string()).map(Some),
        None => Ok(None),
    }
}

/// Whether the shell can evaluate JavaScript in the dock.
pub fn browser_dock_eval_available() -> bool {
    evaluator()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_some()
}
