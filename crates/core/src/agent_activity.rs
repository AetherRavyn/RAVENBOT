//! Cross-surface agent activity and computer-control pause.
//!
//! Browser DOM actions and desktop input live in different crates, but the
//! person watching needs one answer: what is the agent doing right now, and can
//! I take the machine back? The shell installs one listener; skills emit small,
//! already-redacted events through it. The pause flag is process-wide and
//! checked before any mutating desktop or browser action.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

/// One observable action. It deliberately carries no typed text, secrets, or
/// full page contents; `detail` is a short, human-readable summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentActivityEvent {
    /// `browser`, `computer`, or `window`.
    pub surface: String,
    pub bot_id: String,
    pub action: String,
    pub detail: String,
    pub ok: bool,
    pub at: String,
    /// Screen-space pointer location for actions that have one. The computer
    /// panel uses it to draw a click marker over the screenshot.
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
}

type Listener = Box<dyn Fn(AgentActivityEvent) + Send + Sync>;

fn listener() -> &'static Mutex<Option<Listener>> {
    static LISTENER: OnceLock<Mutex<Option<Listener>>> = OnceLock::new();
    LISTENER.get_or_init(|| Mutex::new(None))
}

/// Install the shell's activity listener. Replacing one is intentional: a
/// restarted shell and each test both need to own the slot.
pub fn set_agent_activity_listener<F>(listener_fn: F)
where
    F: Fn(AgentActivityEvent) + Send + Sync + 'static,
{
    let mut slot = listener().lock().unwrap_or_else(|error| error.into_inner());
    *slot = Some(Box::new(listener_fn));
}

/// Remove the current listener, if any.
pub fn clear_agent_activity_listener() {
    let mut slot = listener().lock().unwrap_or_else(|error| error.into_inner());
    *slot = None;
}

/// Publish one action. A missing listener is not an error: headless runs and
/// CLI/MCP callers simply have nobody watching.
pub fn emit_agent_activity(
    surface: &str,
    bot_id: Uuid,
    action: &str,
    detail: impl Into<String>,
    ok: bool,
) {
    emit_agent_activity_at(surface, bot_id, action, detail, ok, None, None);
}

/// Publish an action with a screen-space pointer location.
pub fn emit_agent_activity_at(
    surface: &str,
    bot_id: Uuid,
    action: &str,
    detail: impl Into<String>,
    ok: bool,
    x: Option<f32>,
    y: Option<f32>,
) {
    let detail: String = detail.into();
    let event = AgentActivityEvent {
        surface: surface.to_string(),
        bot_id: bot_id.to_string(),
        action: action.to_string(),
        detail: detail.chars().take(300).collect(),
        ok,
        at: Utc::now().to_rfc3339(),
        x,
        y,
    };
    if let Some(notify) = listener()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
    {
        notify(event);
    }
}

static COMPUTER_PAUSED: AtomicBool = AtomicBool::new(false);
static WAYLAND_CONTROL_ALLOWED: AtomicBool = AtomicBool::new(false);

/// Stop or resume agent control of the real desktop. Read-only observation
/// (screenshots, window listing) stays available while paused.
pub fn set_computer_control_paused(paused: bool) {
    COMPUTER_PAUSED.store(paused, Ordering::SeqCst);
}

pub fn computer_control_paused() -> bool {
    COMPUTER_PAUSED.load(Ordering::SeqCst)
}

/// The user has explicitly allowed host control on a Wayland session. This is
/// persisted by the shell and re-applied before the runtime is built. It is
/// separate from the per-agent `host_control` switch: both must allow input.
pub fn set_wayland_control_allowed(allowed: bool) {
    WAYLAND_CONTROL_ALLOWED.store(allowed, Ordering::SeqCst);
}

pub fn wayland_control_allowed() -> bool {
    WAYLAND_CONTROL_ALLOWED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn activity_reaches_the_listener_and_details_stay_bounded() {
        // The listener is process-wide, so one test owns it for the duration.
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        set_agent_activity_listener(move |event| {
            sink.lock().unwrap().push(event);
        });
        let bot = Uuid::new_v4();
        emit_agent_activity("browser", bot, "click", "#submit", true);
        emit_agent_activity_at(
            "computer",
            bot,
            "click",
            "click (640, 360)",
            true,
            Some(640.0),
            Some(360.0),
        );
        emit_agent_activity("computer", bot, "type", "x".repeat(900), false);
        let events = seen.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].surface, "browser");
        assert_eq!(events[0].bot_id, bot.to_string());
        assert_eq!(events[0].action, "click");
        assert!(events[0].ok);
        assert_eq!((events[1].x, events[1].y), (Some(640.0), Some(360.0)));
        assert_eq!(events[2].detail.chars().count(), 300);
        assert!(!events[2].ok);
        drop(events);
        clear_agent_activity_listener();
    }

    #[test]
    fn pause_round_trips() {
        set_computer_control_paused(true);
        assert!(computer_control_paused());
        set_computer_control_paused(false);
        assert!(!computer_control_paused());
    }

    #[test]
    fn wayland_opt_in_round_trips() {
        set_wayland_control_allowed(true);
        assert!(wayland_control_allowed());
        set_wayland_control_allowed(false);
        assert!(!wayland_control_allowed());
    }
}
