//! Desktop window discovery and focus.
//!
//! Linux has two paths:
//! - **Hyprland** exposes native Wayland windows through `hyprctl clients -j`;
//!   xcap's X11 enumeration sees none of them.
//! - X11 (and XWayland-only sessions) uses xcap for listing and `xdotool` for
//!   activation.
//!
//! Failures are reported honestly. A strict compositor can refuse focus; the
//! caller gets the compositor's reason instead of a fabricated success.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DesktopWindow {
    pub id: u64,
    pub title: String,
    pub app_name: String,
    pub width: u32,
    pub height: u32,
    pub minimized: bool,
    pub focused: bool,
}

fn find_binary(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn hyprland_available() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() && find_binary("hyprctl").is_some()
}

fn hyprland_clients() -> Result<serde_json::Value, String> {
    let hyprctl = find_binary("hyprctl")
        .ok_or_else(|| "hyprctl is not installed on this machine.".to_string())?;
    let output = std::process::Command::new(hyprctl)
        .args(["clients", "-j"])
        .output()
        .map_err(|error| format!("Could not run hyprctl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "hyprctl could not list windows: {}",
            String::from_utf8_lossy(&output.stderr)
                .trim()
                .lines()
                .next()
                .unwrap_or("unknown error")
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("hyprctl returned invalid JSON: {error}"))
}

/// Parse the `hyprctl clients -j` array. Kept pure for tests.
fn parse_hyprland_clients(value: &serde_json::Value) -> Vec<DesktopWindow> {
    let Some(clients) = value.as_array() else {
        return Vec::new();
    };
    let mut windows = Vec::new();
    for client in clients {
        let Some(address) = client.get("address").and_then(|value| value.as_str()) else {
            continue;
        };
        let Ok(id) = u64::from_str_radix(address.trim_start_matches("0x"), 16) else {
            continue;
        };
        let title = client
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let app_name = client
            .get("class")
            .and_then(|value| value.as_str())
            .or_else(|| client.get("initialClass").and_then(|value| value.as_str()))
            .unwrap_or("")
            .to_string();
        if title.trim().is_empty() && app_name.trim().is_empty() {
            continue;
        }
        let width = client
            .get("size")
            .and_then(|value| value.as_array())
            .and_then(|sizes| sizes.first())
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32;
        let height = client
            .get("size")
            .and_then(|value| value.as_array())
            .and_then(|sizes| sizes.get(1))
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32;
        let hidden = client
            .get("hidden")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let mapped = client
            .get("mapped")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let focused = client
            .get("focusHistoryID")
            .and_then(|value| value.as_i64())
            .map(|rank| rank == 0)
            .unwrap_or(false);
        windows.push(DesktopWindow {
            id,
            title,
            app_name,
            width,
            height,
            minimized: hidden || !mapped,
            focused,
        });
    }
    windows.sort_by(|left, right| {
        right
            .focused
            .cmp(&left.focused)
            .then_with(|| {
                left.app_name
                    .to_lowercase()
                    .cmp(&right.app_name.to_lowercase())
            })
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
    });
    windows
}

fn hyprland_windows() -> Result<Vec<DesktopWindow>, String> {
    Ok(parse_hyprland_clients(&hyprland_clients()?))
}

/// Parse the top-left of the compositor's layout bounding box. The grim
/// screenshot canvas starts here, so screenshot pixel (0, 0) maps to this
/// global coordinate.
fn parse_hyprland_origin(value: &serde_json::Value) -> Option<(i32, i32)> {
    let monitors = value.as_array()?;
    let mut min_x: Option<i32> = None;
    let mut min_y: Option<i32> = None;
    for monitor in monitors {
        if let Some(x) = monitor.get("x").and_then(|value| value.as_i64()) {
            let x = x as i32;
            min_x = Some(min_x.map_or(x, |current| current.min(x)));
        }
        if let Some(y) = monitor.get("y").and_then(|value| value.as_i64()) {
            let y = y as i32;
            min_y = Some(min_y.map_or(y, |current| current.min(y)));
        }
    }
    Some((min_x?, min_y?))
}

/// The global coordinate of the top-left of the desktop layout. `None` means
/// the session has no Hyprland monitor data, so callers should assume (0, 0).
pub fn desktop_origin() -> Option<(i32, i32)> {
    if !hyprland_available() {
        return None;
    }
    parse_hyprland_origin(&hyprctl_json("monitors")?)
}

fn hyprctl_json(command: &str) -> Option<serde_json::Value> {
    let hyprctl = find_binary("hyprctl")?;
    let output = std::process::Command::new(hyprctl)
        .args([command, "-j"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

fn xcap_windows() -> Result<Vec<DesktopWindow>, String> {
    let windows = xcap::Window::all()
        .map_err(|error| format!("The window manager did not return its windows: {error}"))?;
    let mut result = Vec::new();
    for window in windows {
        let Ok(id) = window.id() else {
            continue;
        };
        let title = window.title().unwrap_or_default();
        let app_name = window.app_name().unwrap_or_default();
        if title.trim().is_empty() && app_name.trim().is_empty() {
            continue;
        }
        result.push(DesktopWindow {
            id: id as u64,
            title,
            app_name,
            width: window.width().unwrap_or(0),
            height: window.height().unwrap_or(0),
            minimized: window.is_minimized().unwrap_or(false),
            focused: window.is_focused().unwrap_or(false),
        });
    }
    result.sort_by(|left, right| {
        right
            .focused
            .cmp(&left.focused)
            .then_with(|| {
                left.app_name
                    .to_lowercase()
                    .cmp(&right.app_name.to_lowercase())
            })
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
    });
    Ok(result)
}

/// Every window the compositor exposes. Hyprland is checked first because a
/// native Wayland window never appears in xcap's X11 list.
pub fn list_windows() -> Result<Vec<DesktopWindow>, String> {
    if hyprland_available() {
        if let Ok(windows) = hyprland_windows() {
            if !windows.is_empty() {
                return Ok(windows);
            }
        }
    }
    xcap_windows()
}

/// Focus a window by its id. Returns a human-readable confirmation.
pub async fn focus_window(id: u64) -> Result<String, String> {
    let window = list_windows()?
        .into_iter()
        .find(|window| window.id == id)
        .ok_or_else(|| format!("No window with id {id} is open."))?;
    if hyprland_available() {
        let hyprctl = find_binary("hyprctl")
            .ok_or_else(|| "hyprctl is not installed on this machine.".to_string())?;
        let output = tokio::process::Command::new(hyprctl)
            .args(["dispatch", "focuswindow", &format!("address:0x{id:x}")])
            .output()
            .await
            .map_err(|error| format!("Could not run hyprctl: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Hyprland refused to focus {}: {}",
                window.title,
                String::from_utf8_lossy(&output.stderr)
                    .trim()
                    .lines()
                    .next()
                    .unwrap_or("unknown error")
            ));
        }
        return Ok(format!(
            "Focused {} ({})",
            if window.title.trim().is_empty() {
                window.app_name.as_str()
            } else {
                window.title.as_str()
            },
            window.id
        ));
    }

    let xdotool = find_binary("xdotool").ok_or_else(|| {
        "Focusing a window needs `xdotool` on this machine. Install it, or control the app through its own CLI."
            .to_string()
    })?;
    let output = tokio::process::Command::new(xdotool)
        .args(["windowactivate", "--sync", &id.to_string()])
        .output()
        .await
        .map_err(|error| format!("Could not run xdotool: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "The window manager refused to focus {}: {}",
            window.title,
            stderr.trim().lines().next().unwrap_or("unknown error")
        ));
    }
    Ok(format!(
        "Focused {} ({})",
        if window.title.trim().is_empty() {
            window.app_name.as_str()
        } else {
            window.title.as_str()
        },
        window.id
    ))
}

/// The compositor geometry for a window, used by the `grim` screenshot path.
pub fn window_geometry(id: u64) -> Option<(i32, i32, u32, u32)> {
    if !hyprland_available() {
        return None;
    }
    let clients = hyprland_clients().ok()?;
    let client = clients.as_array()?.iter().find(|client| {
        client
            .get("address")
            .and_then(|value| value.as_str())
            .and_then(|address| u64::from_str_radix(address.trim_start_matches("0x"), 16).ok())
            == Some(id)
    })?;
    let at = client.get("at")?.as_array()?;
    let size = client.get("size")?.as_array()?;
    Some((
        at.first()?.as_i64()? as i32,
        at.get(1)?.as_i64()? as i32,
        size.first()?.as_u64()? as u32,
        size.get(1)?.as_u64()? as u32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hyprland_windows_and_marks_focus_and_hidden() {
        let clients = serde_json::json!([
            {
                "address": "0xabc123",
                "title": "Billing Ledger",
                "class": "code",
                "size": [1200, 800],
                "hidden": false,
                "mapped": true,
                "focusHistoryID": 0
            },
            {
                "address": "0xdef456",
                "title": "Background",
                "initialClass": "slack",
                "size": [800, 600],
                "hidden": true,
                "mapped": true,
                "focusHistoryID": 4
            }
        ]);
        let windows = parse_hyprland_clients(&clients);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].title, "Billing Ledger");
        assert_eq!(windows[0].id, 0xabc123);
        assert!(windows[0].focused);
        assert_eq!(windows[1].app_name, "slack");
        assert!(windows[1].minimized);
    }

    #[test]
    fn layout_origin_accounts_for_negative_monitor_positions() {
        let monitors = serde_json::json!([
            { "name": "DP-3", "x": 0, "y": 0, "width": 1920, "height": 1080 },
            { "name": "DP-4", "x": 3840, "y": -420, "width": 1920, "height": 1080 }
        ]);
        assert_eq!(parse_hyprland_origin(&monitors), Some((0, -420)));
        assert_eq!(parse_hyprland_origin(&serde_json::json!([])), None);
    }

    #[test]
    fn window_listing_is_honest() {
        // Headless CI has no compositor. Either a real list or a descriptive
        // error is acceptable; a fabricated window is not.
        match list_windows() {
            Ok(windows) => {
                for window in windows {
                    assert!(!window.title.is_empty() || !window.app_name.is_empty());
                }
            }
            Err(error) => assert!(!error.is_empty()),
        }
    }

    #[tokio::test]
    async fn unknown_window_is_refused() {
        let error = focus_window(0).await.expect_err("window 0 does not exist");
        assert!(error.contains("No window") || error.contains("window manager"));
    }
}
