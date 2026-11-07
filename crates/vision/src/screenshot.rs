// Screenshot capture — real screen capture via `xcap`.
//
// Honesty contract: on success this returns a real frame with real
// dimensions. On failure (no display, headless, Wayland without the
// PipeWire portal, permission denied) it returns a descriptive error —
// NEVER a placeholder image. Callers can trust a `Ok(Screenshot)` is real.

use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScreenshotError {
    #[error("Capture failed: {0}")]
    CaptureFailed(String),
    #[error("No display available — are you running in a graphical environment?")]
    NoDisplay,
    #[error("Permission denied — grant Screen Recording / Desktop access in system settings.")]
    PermissionDenied,
    #[error("No monitors detected.")]
    NoMonitors,
    #[error("Format error: {0}")]
    FormatError(String),
}

/// A captured screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screenshot {
    /// Base64-encoded PNG data.
    pub data: String,
    /// Image format (always "png" for now).
    pub format: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Desktop-layout coordinate of the image's top-left pixel. A click on a
    /// screenshot at (x, y) targets `origin + (x, y)` on the machine.
    #[serde(default)]
    pub origin_x: i32,
    #[serde(default)]
    pub origin_y: i32,
    /// Timestamp.
    pub timestamp: chrono::DateTime<Utc>,
}

impl Screenshot {
    /// Get the image as a data URL.
    pub fn to_data_url(&self) -> String {
        format!("data:image/png;base64,{}", self.data)
    }

    /// Get raw PNG bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ScreenshotError> {
        general_purpose::STANDARD
            .decode(&self.data)
            .map_err(|e| ScreenshotError::FormatError(e.to_string()))
    }
}

/// Capture region (unused for full-monitor capture, kept for API compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct ScreenshotCapture {
    region: Option<Region>,
}

fn find_binary(name: &str) -> Option<std::path::PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

/// Wayland capture through grim. Hyprland and most wlroots compositors expose
/// the full output here even when xcap sees no monitor.
async fn capture_with_grim(region: Option<Region>) -> Result<Screenshot, ScreenshotError> {
    let grim = find_binary("grim").ok_or_else(|| {
        ScreenshotError::CaptureFailed(
            "grim is not installed. Install grim for Wayland screen capture.".to_string(),
        )
    })?;
    let (origin_x, origin_y) = match region.as_ref() {
        Some(region) => (region.x, region.y),
        None => crate::window_control::desktop_origin().unwrap_or((0, 0)),
    };
    let mut command = tokio::process::Command::new(grim);
    match region {
        Some(region) => {
            command.args([
                "-g",
                &format!(
                    "{},{} {}x{}",
                    region.x, region.y, region.width, region.height
                ),
                "-",
            ]);
        }
        None => {
            command.arg("-");
        }
    }
    let output = command
        .output()
        .await
        .map_err(|error| ScreenshotError::CaptureFailed(format!("Could not run grim: {error}")))?;
    if !output.status.success() {
        return Err(ScreenshotError::CaptureFailed(format!(
            "grim failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .trim()
                .lines()
                .next()
                .unwrap_or("unknown error")
        )));
    }
    if output.stdout.is_empty() {
        return Err(ScreenshotError::CaptureFailed(
            "grim returned an empty frame.".to_string(),
        ));
    }
    let image = image::load_from_memory(&output.stdout)
        .map_err(|error| ScreenshotError::FormatError(error.to_string()))?;
    let width = image.width();
    let height = image.height();
    if width < 2 || height < 2 {
        return Err(ScreenshotError::CaptureFailed(
            "grim returned an implausibly small frame.".to_string(),
        ));
    }
    Ok(Screenshot {
        data: general_purpose::STANDARD.encode(&output.stdout),
        format: "png".to_string(),
        width,
        height,
        origin_x,
        origin_y,
        timestamp: Utc::now(),
    })
}

impl ScreenshotCapture {
    pub fn new() -> Self {
        Self { region: None }
    }

    pub fn with_region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    /// Capture the primary monitor. Returns a REAL frame or a descriptive
    /// error — never a placeholder.
    pub async fn capture(&self) -> Result<Screenshot, ScreenshotError> {
        // Prefer the compositor's own capture on Wayland. xcap's X11 path can
        // return an empty or partial frame on Hyprland.
        if wayland_session() {
            if let Ok(shot) = capture_with_grim(None).await {
                return Ok(shot);
            }
        }
        use xcap::{image::ImageFormat, Monitor};

        let monitors = Monitor::all().map_err(|e| match e.to_string().as_str() {
            s if s.contains("permission") => ScreenshotError::PermissionDenied,
            _ => ScreenshotError::CaptureFailed(format!("Failed to enumerate monitors: {}", e)),
        })?;

        if monitors.is_empty() {
            return capture_with_grim(None).await;
        }

        let monitor = monitors
            .iter()
            // xcap 0.5+ made these getters fallible; a monitor we cannot read
            // metadata for is simply not the primary one.
            .find(|m| m.is_primary().unwrap_or(false))
            .unwrap_or(&monitors[0]);
        let origin_x = monitor.x().unwrap_or(0);
        let origin_y = monitor.y().unwrap_or(0);

        let img = monitor.capture_image().map_err(|e| {
            let msg = e.to_string().to_lowercase();
            if msg.contains("permission") || msg.contains("denied") {
                ScreenshotError::PermissionDenied
            } else {
                ScreenshotError::CaptureFailed(format!("Capture failed: {}", e))
            }
        })?;

        let width = img.width();
        let height = img.height();

        let mut png_bytes: Vec<u8> = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        img.write_to(&mut cursor, ImageFormat::Png)
            .map_err(|e| ScreenshotError::FormatError(e.to_string()))?;

        if width < 2 || height < 2 {
            return Err(ScreenshotError::CaptureFailed(
                "Captured frame is implausibly small — capture likely failed silently".to_string(),
            ));
        }

        tracing::info!(width = width, height = height, "Screenshot captured");

        Ok(Screenshot {
            data: general_purpose::STANDARD.encode(&png_bytes),
            format: "png".to_string(),
            width,
            height,
            origin_x,
            origin_y,
            timestamp: Utc::now(),
        })
    }

    /// List available monitors (for a future "pick monitor" UI).
    pub fn list_monitors() -> Result<Vec<(u32, u32, u32, bool)>, ScreenshotError> {
        fn meta<T>(
            label: &str,
            r: Result<T, impl std::fmt::Display>,
        ) -> Result<T, ScreenshotError> {
            r.map_err(|e| ScreenshotError::CaptureFailed(format!("{label}: {e}")))
        }
        let monitors =
            xcap::Monitor::all().map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;
        monitors
            .iter()
            .map(|m| {
                Ok((
                    meta("monitor id", m.id())?,
                    meta("monitor width", m.width())?,
                    meta("monitor height", m.height())?,
                    m.is_primary().unwrap_or(false),
                ))
            })
            .collect()
    }

    /// Capture a specific window by id.
    pub async fn capture_window(&self, window_id: u64) -> Result<Screenshot, ScreenshotError> {
        // Hyprland windows are native Wayland surfaces; xcap cannot see them.
        if wayland_session() {
            if let Some((x, y, width, height)) = crate::window_control::window_geometry(window_id) {
                if let Ok(shot) = capture_with_grim(Some(Region {
                    x,
                    y,
                    width,
                    height,
                }))
                .await
                {
                    return Ok(shot);
                }
            }
        }
        use xcap::{image::ImageFormat, Window};

        let windows = Window::all().map_err(|e| {
            ScreenshotError::CaptureFailed(format!("Failed to enumerate windows: {}", e))
        })?;

        let window = windows
            .iter()
            // `id()` is fallible as of xcap 0.5; a window we cannot identify
            // just does not match the one that was asked for.
            .find(|w| w.id().ok().map(|id| id as u64) == Some(window_id))
            .ok_or_else(|| {
                ScreenshotError::CaptureFailed(format!("Window {} not found", window_id))
            })?;

        let img = window.capture_image().map_err(|e| {
            let msg = e.to_string().to_lowercase();
            if msg.contains("permission") || msg.contains("denied") {
                ScreenshotError::PermissionDenied
            } else {
                ScreenshotError::CaptureFailed(format!("Window capture failed: {}", e))
            }
        })?;

        let width = img.width();
        let height = img.height();
        let origin_x = window.x().unwrap_or(0);
        let origin_y = window.y().unwrap_or(0);

        let mut png_bytes: Vec<u8> = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        img.write_to(&mut cursor, ImageFormat::Png)
            .map_err(|e| ScreenshotError::FormatError(e.to_string()))?;

        if width < 2 || height < 2 {
            return Err(ScreenshotError::CaptureFailed(
                "Captured window frame is implausibly small".to_string(),
            ));
        }

        Ok(Screenshot {
            data: general_purpose::STANDARD.encode(&png_bytes),
            format: "png".to_string(),
            width,
            height,
            origin_x,
            origin_y,
            timestamp: Utc::now(),
        })
    }
}

impl Default for ScreenshotCapture {
    fn default() -> Self {
        Self::new()
    }
}
