//! Computer control skill — drive the real desktop from a running bot.
//!
//! Each call performs one input action (click, type, key, scroll, move, wait),
//! captures the screen or a window, lists open windows, or focuses a window.
//! Actions run through `ravenbot-vision`'s input layer, which detects
//! xdotool/ydotool/wtype and reports honestly when a capability is unavailable.
//!
//! Every mutating action is gated by the approval broker unless the bot is in
//! Full mode, and by the process-wide computer takeover flag. The UI receives a
//! small activity event per action; no typed text is included.

use async_trait::async_trait;
use ravenbot_core::Permission;
use ravenbot_vision::computer_control::{ComputerAction, MouseButton};
use ravenbot_vision::ComputerController;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

use crate::traits::{Skill, SkillContext, SkillError, SkillResult, SkillRisk};

pub struct ComputerControlSkill;

impl ComputerControlSkill {
    pub fn new() -> Self {
        Self
    }
}

/// The frame the agent last looked at. Pointer actions use this to translate
/// image pixels into desktop layout coordinates; without it, a click on a
/// monitor above the origin would land hundreds of pixels away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenshotFrame {
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
}

fn screenshot_frames() -> &'static Mutex<HashMap<Uuid, ScreenshotFrame>> {
    static FRAMES: OnceLock<Mutex<HashMap<Uuid, ScreenshotFrame>>> = OnceLock::new();
    FRAMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_frame(bot_id: Uuid, frame: ScreenshotFrame) {
    screenshot_frames()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(bot_id, frame);
}

fn last_frame(bot_id: Uuid) -> Option<ScreenshotFrame> {
    screenshot_frames()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&bot_id)
        .copied()
}

/// Translate an image pixel to the desktop coordinate the input backend
/// expects. When no screenshot has been taken yet, the two spaces are the
/// same and this is a no-op.
fn translate_point(bot_id: Uuid, x: f32, y: f32) -> (f32, f32) {
    match last_frame(bot_id) {
        Some(frame) => (x + frame.origin_x as f32, y + frame.origin_y as f32),
        None => (x, y),
    }
}

fn frame_center(bot_id: Uuid) -> (f32, f32) {
    match last_frame(bot_id) {
        Some(frame) => (
            frame.origin_x as f32 + frame.width as f32 / 2.0,
            frame.origin_y as f32 + frame.height as f32 / 2.0,
        ),
        None => (0.0, 0.0),
    }
}

fn activity(context: &SkillContext, action: &str, detail: impl Into<String>, ok: bool) {
    ravenbot_core::emit_agent_activity("computer", context.bot_id, action, detail, ok);
}

fn activity_at(
    context: &SkillContext,
    action: &str,
    detail: impl Into<String>,
    ok: bool,
    point: Option<(f32, f32)>,
) {
    let (x, y) = match point {
        Some((x, y)) => (Some(x), Some(y)),
        None => (None, None),
    };
    ravenbot_core::emit_agent_activity_at("computer", context.bot_id, action, detail, ok, x, y);
}

fn action_point(action: &ComputerAction) -> Option<(f32, f32)> {
    match action {
        ComputerAction::Click { x, y, .. }
        | ComputerAction::DoubleClick { x, y }
        | ComputerAction::RightClick { x, y }
        | ComputerAction::MoveMouse { x, y }
        | ComputerAction::Scroll { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

fn required_f32(
    arguments: &serde_json::Value,
    key: &str,
    action: &str,
) -> Result<f32, SkillError> {
    arguments
        .get(key)
        .and_then(|value| value.as_f64())
        .map(|value| value as f32)
        .ok_or_else(|| {
            SkillError::InvalidArguments(format!("action={action} requires numeric '{key}'"))
        })
}

fn required_window_id(
    arguments: &serde_json::Value,
    action: &str,
) -> Result<u64, SkillError> {
    arguments
        .get("window_id")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| {
            SkillError::InvalidArguments(format!("action={action} requires 'window_id'"))
        })
}

#[async_trait]
impl Skill for ComputerControlSkill {
    fn id(&self) -> &str {
        "computer_control"
    }

    fn name(&self) -> &str {
        "Computer Control"
    }

    fn description(&self) -> &str {
        "Control the real desktop: list and focus windows, move the mouse, click, \
         type text, press keys, scroll, wait, or capture the screen or one window. \
         Use for GUI tasks that no CLI tool covers. Pointer coordinates are pixels \
         in the most recent screenshot; the screenshot reports the desktop origin \
         when its top-left pixel is not (0, 0)."
    }

    fn version(&self) -> &str {
        "1.1.0"
    }

    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::InputControl, Permission::Screenshot]
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "click", "double_click", "right_click", "move", "type", "key",
                        "scroll", "wait", "screenshot", "screenshot_window", "windows",
                        "focus_window"
                    ],
                    "description": "The action to perform"
                },
                "x": { "type": "number", "description": "X pixel in the most recent screenshot (pointer actions)" },
                "y": { "type": "number", "description": "Y pixel in the most recent screenshot (pointer actions)" },
                "text": { "type": "string", "description": "Text to type (action=type)" },
                "key": { "type": "string", "description": "Key name, e.g. Return, ctrl+c, Tab (action=key)" },
                "delta_x": { "type": "integer", "description": "Horizontal scroll amount (action=scroll)" },
                "delta_y": { "type": "integer", "description": "Vertical scroll amount; positive scrolls down (action=scroll)" },
                "milliseconds": { "type": "integer", "description": "Wait duration in milliseconds, max 30000 (action=wait)" },
                "window_id": { "type": "integer", "description": "Window id from action=windows (focus_window / screenshot_window)" }
            },
            "required": ["action"]
        })
    }

    fn risk(&self) -> SkillRisk {
        SkillRisk::High
    }

    async fn execute(
        &self,
        context: &SkillContext,
        arguments: serde_json::Value,
    ) -> Result<SkillResult, SkillError> {
        let action = arguments
            .get("action")
            .and_then(|value| value.as_str())
            .ok_or_else(|| SkillError::InvalidArguments("Missing 'action' field".to_string()))?;

        // Read-only observation is always allowed.
        match action {
            "screenshot" => {
                let capture = ravenbot_vision::ScreenshotCapture::new();
                let shot = match capture.capture().await {
                    Ok(shot) => shot,
                    Err(error) => {
                        activity(context, action, error.to_string(), false);
                        return Err(SkillError::Execution(error.to_string()));
                    }
                };
                remember_frame(
                    context.bot_id,
                    ScreenshotFrame {
                        origin_x: shot.origin_x,
                        origin_y: shot.origin_y,
                        width: shot.width,
                        height: shot.height,
                    },
                );
                activity(
                    context,
                    action,
                    format!("captured {}x{} screen", shot.width, shot.height),
                    true,
                );
                return Ok(SkillResult::success(serde_json::json!({
                    "action": "screenshot",
                    "width": shot.width,
                    "height": shot.height,
                    "origin_x": shot.origin_x,
                    "origin_y": shot.origin_y,
                    "data_url": shot.to_data_url(),
                })));
            }
            "screenshot_window" => {
                let window_id = required_window_id(&arguments, action)?;
                let capture = ravenbot_vision::ScreenshotCapture::new();
                let shot = match capture.capture_window(window_id).await {
                    Ok(shot) => shot,
                    Err(error) => {
                        activity(context, action, error.to_string(), false);
                        return Err(SkillError::Execution(error.to_string()));
                    }
                };
                remember_frame(
                    context.bot_id,
                    ScreenshotFrame {
                        origin_x: shot.origin_x,
                        origin_y: shot.origin_y,
                        width: shot.width,
                        height: shot.height,
                    },
                );
                activity(
                    context,
                    action,
                    format!("captured window {} ({}x{})", window_id, shot.width, shot.height),
                    true,
                );
                return Ok(SkillResult::success(serde_json::json!({
                    "action": "screenshot_window",
                    "window_id": window_id,
                    "width": shot.width,
                    "height": shot.height,
                    "origin_x": shot.origin_x,
                    "origin_y": shot.origin_y,
                    "data_url": shot.to_data_url(),
                })));
            }
            "windows" => {
                let windows = match ravenbot_vision::list_windows() {
                    Ok(windows) => windows,
                    Err(error) => {
                        activity(context, action, error.clone(), false);
                        return Ok(SkillResult::failure(error));
                    }
                };
                activity(
                    context,
                    action,
                    format!("listed {} windows", windows.len()),
                    true,
                );
                return Ok(SkillResult::success(serde_json::json!({
                    "action": "windows",
                    "windows": windows,
                })));
            }
            _ => {}
        }

        // Everything below changes the desktop or its focused window.
        if action == "focus_window" {
            let window_id = required_window_id(&arguments, action)?;
            if ravenbot_core::computer_control_paused() {
                activity(context, action, "blocked by user takeover", false);
                return Ok(SkillResult::failure(
                    "The user has taken over the computer. Agent input is paused until they release it.",
                ));
            }
            return match ravenbot_vision::focus_window(window_id).await {
                Ok(message) => {
                    activity(context, action, message.clone(), true);
                    Ok(SkillResult::success(serde_json::json!({
                        "action": "focus_window",
                        "window_id": window_id,
                        "message": message,
                    })))
                }
                Err(error) => {
                    activity(context, action, error.clone(), false);
                    Ok(SkillResult::failure(error))
                }
            };
        }

        let controller = ComputerController::new();
        let int = |key: &str| -> i32 {
            arguments
                .get(key)
                .and_then(|value| value.as_i64())
                .unwrap_or(0) as i32
        };
        let point = |key: &str| -> Result<(f32, f32), SkillError> {
            Ok(translate_point(
                context.bot_id,
                required_f32(&arguments, "x", key)?,
                required_f32(&arguments, "y", key)?,
            ))
        };

        let computer_action = match action {
            "click" => {
                let (x, y) = point(action)?;
                ComputerAction::Click {
                    x,
                    y,
                    button: MouseButton::Left,
                }
            }
            "double_click" => {
                let (x, y) = point(action)?;
                ComputerAction::DoubleClick { x, y }
            }
            "right_click" => {
                let (x, y) = point(action)?;
                ComputerAction::RightClick { x, y }
            }
            "move" => {
                let (x, y) = point(action)?;
                ComputerAction::MoveMouse { x, y }
            }
            "type" => ComputerAction::TypeText {
                text: arguments
                    .get("text")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidArguments("action=type requires 'text'".to_string())
                    })?
                    .to_string(),
            },
            "key" => ComputerAction::PressKey {
                key: arguments
                    .get("key")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidArguments("action=key requires 'key'".to_string())
                    })?
                    .to_string(),
            },
            "scroll" => {
                let point = match (
                    arguments.get("x").and_then(|value| value.as_f64()),
                    arguments.get("y").and_then(|value| value.as_f64()),
                ) {
                    (Some(x), Some(y)) => translate_point(context.bot_id, x as f32, y as f32),
                    _ => frame_center(context.bot_id),
                };
                ComputerAction::Scroll {
                    x: point.0,
                    y: point.1,
                    delta_x: int("delta_x"),
                    delta_y: int("delta_y"),
                }
            }
            "wait" => ComputerAction::Wait {
                milliseconds: arguments
                    .get("milliseconds")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(1000)
                    .min(30_000),
            },
            other => {
                return Ok(SkillResult::failure(format!(
                    "Unknown action '{other}'. Valid: click, double_click, right_click, move, type, key, scroll, wait, screenshot, screenshot_window, windows, focus_window."
                )))
            }
        };

        if ravenbot_core::computer_control_paused() {
            activity(context, action, "blocked by user takeover", false);
            return Ok(SkillResult::failure(
                "The user has taken over the computer. Agent input is paused until they release it.",
            ));
        }

        if !controller.input_available() && action != "wait" {
            activity(context, action, "no desktop input backend", false);
            return Ok(SkillResult::failure(
                "No desktop input backend is available on this machine. Install `xdotool` (X11) or `ydotool`/`wtype` (Wayland).",
            ));
        }

        let plan = controller
            .input_plan(&computer_action)
            .unwrap_or_else(|| "unavailable".to_string());
        let detail = match &computer_action {
            ComputerAction::Click { x, y, .. }
            | ComputerAction::DoubleClick { x, y }
            | ComputerAction::RightClick { x, y }
            | ComputerAction::MoveMouse { x, y } => format!("{action} ({x}, {y})"),
            ComputerAction::TypeText { text } => {
                format!("typed {} characters", text.chars().count())
            }
            ComputerAction::PressKey { key } => {
                // A single-character key can be a password character; keep the
                // activity feed free of keystrokes.
                if key.chars().count() <= 1 {
                    "pressed a key".to_string()
                } else {
                    format!("key {key}")
                }
            }
            ComputerAction::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => format!("scroll ({x}, {y}) by ({delta_x}, {delta_y})"),
            ComputerAction::Wait { milliseconds } => format!("wait {milliseconds}ms"),
        };

        let point = action_point(&computer_action);
        let result = match controller.execute(&computer_action).await {
            Ok(result) => result,
            Err(error) => {
                activity_at(
                    context,
                    action,
                    format!("{detail}: {error}"),
                    false,
                    point,
                );
                return Err(SkillError::Execution(error.to_string()));
            }
        };

        activity_at(context, action, detail.clone(), result.success, point);
        let mut output = serde_json::json!({
            "action": action,
            "success": result.success,
            "message": result.message,
            "backend": controller.input_backend(),
            "plan": plan,
        });
        if let Some(shot) = result.screenshot_after {
            output["screenshot_after"] = serde_json::json!(shot.to_data_url());
        }
        Ok(SkillResult::success(output))
    }
}

impl Default for ComputerControlSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ctx() -> SkillContext {
        SkillContext::with_default_tier(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
    }

    #[tokio::test]
    async fn unknown_action_reports_the_valid_set() {
        let skill = ComputerControlSkill::new();
        let result = skill
            .execute(&ctx(), serde_json::json!({"action": "fly"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("Unknown action"));
    }

    #[tokio::test]
    async fn type_without_text_is_invalid() {
        let skill = ComputerControlSkill::new();
        let err = skill
            .execute(&ctx(), serde_json::json!({"action": "type"}))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn click_without_coordinates_is_invalid() {
        let skill = ComputerControlSkill::new();
        let err = skill
            .execute(&ctx(), serde_json::json!({"action": "click"}))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn window_screenshot_requires_a_window_id() {
        let skill = ComputerControlSkill::new();
        let err = skill
            .execute(&ctx(), serde_json::json!({"action": "screenshot_window"}))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn windows_action_is_read_only_and_honest() {
        let skill = ComputerControlSkill::new();
        let result = skill
            .execute(&ctx(), serde_json::json!({"action": "windows"}))
            .await
            .expect("execute");
        // Headless CI may have no compositor; either a real list or an honest
        // failure is acceptable.
        if result.success {
            assert!(result.output.get("windows").and_then(|value| value.as_array()).is_some());
        } else {
            assert!(result.error.is_some());
        }
    }

    #[tokio::test]
    async fn takeover_blocks_input_but_not_observation() {
        ravenbot_core::set_computer_control_paused(true);
        let skill = ComputerControlSkill::new();
        let blocked = skill
            .execute(
                &ctx(),
                serde_json::json!({"action": "click", "x": 10, "y": 20}),
            )
            .await
            .expect("execute");
        assert!(!blocked.success);
        assert!(blocked
            .error
            .unwrap_or_default()
            .contains("taken over the computer"));

        let screenshot = skill
            .execute(&ctx(), serde_json::json!({"action": "screenshot"}))
            .await;
        // Screenshot may fail on headless CI, but it must not be the takeover
        // refusal, because observation stays available while paused.
        if let Ok(result) = screenshot {
            if !result.success {
                assert!(!result
                    .error
                    .unwrap_or_default()
                    .contains("taken over the computer"));
            }
        }
        ravenbot_core::set_computer_control_paused(false);
    }

    #[tokio::test]
    async fn screenshot_returns_a_real_image_frame() {
        let skill = ComputerControlSkill::new();
        let result = skill
            .execute(&ctx(), serde_json::json!({"action": "screenshot"}))
            .await;
        match result {
            Ok(r) => {
                assert!(r.output.get("data_url").is_some());
                assert!(r.output.get("width").is_some());
            }
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }
}
