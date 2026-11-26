//! Computer control through vision-guided actions

use crate::app_launch::AppLauncher;
use crate::clipboard::Clipboard;
use crate::image_analysis::{AnalysisResult, ImageAnalyzer};
use crate::input::{Button, InputInjector};
use crate::screenshot::{Screenshot, ScreenshotCapture};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ControlError {
    #[error("Action failed: {0}")]
    ActionFailed(String),
    #[error("Screenshot failed: {0}")]
    ScreenshotFailed(String),
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

/// An action to perform on the computer
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ComputerAction {
    /// Click at coordinates
    Click { x: f32, y: f32, button: MouseButton },
    /// Double-click at coordinates
    DoubleClick { x: f32, y: f32 },
    /// Right-click at coordinates
    RightClick { x: f32, y: f32 },
    /// Type text
    TypeText { text: String },
    /// Press a key
    PressKey { key: String },
    /// Scroll at coordinates
    Scroll {
        x: f32,
        y: f32,
        delta_x: i32,
        delta_y: i32,
    },
    /// Move mouse to coordinates
    MoveMouse { x: f32, y: f32 },
    /// Drag with the left button held from one point to another
    Drag {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        /// Optional hold duration; the input layer caps it at 10 seconds.
        #[serde(default)]
        duration_ms: Option<u64>,
    },
    /// Launch an application, document, or URL
    AppLaunch { target: String },
    /// Read the system clipboard as text (capped)
    ClipboardRead,
    /// Replace the system clipboard with text
    ClipboardWrite { text: String },
    /// Wait for a duration
    Wait { milliseconds: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Result of a computer action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub message: String,
    pub screenshot_after: Option<Screenshot>,
    /// Action-specific data (for example clipboard text or a PID). The
    /// activity log never carries clipboard contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// Vision-guided computer controller
pub struct ComputerController {
    screenshot_capture: ScreenshotCapture,
    image_analyzer: ImageAnalyzer,
    input: InputInjector,
    app_launcher: AppLauncher,
    clipboard: Clipboard,
    /// Whether to capture screenshot after each action
    capture_after_action: bool,
    /// Maximum actions per task
    max_actions: usize,
}

impl ComputerController {
    pub fn new() -> Self {
        Self {
            screenshot_capture: ScreenshotCapture::new(),
            image_analyzer: ImageAnalyzer::new(),
            input: InputInjector::detect(),
            app_launcher: AppLauncher::detect(),
            clipboard: Clipboard::detect(),
            capture_after_action: true,
            max_actions: 50,
        }
    }

    /// Whether real input injection is available on this machine.
    pub fn input_available(&self) -> bool {
        self.input.is_available()
    }

    /// Human-readable summary of the detected input backend.
    pub fn input_backend(&self) -> String {
        self.input.backend_summary()
    }

    /// Human-readable summary of the detected app-launch backend.
    pub fn app_launch_backend(&self) -> String {
        self.app_launcher.backend_summary()
    }

    /// Human-readable summary of the detected clipboard backends.
    pub fn clipboard_backend(&self) -> String {
        self.clipboard.backend_summary()
    }

    /// The argv plan this action would run, without running it.
    pub fn input_plan(&self, action: &ComputerAction) -> Option<String> {
        self.input.describe_plan(action)
    }

    /// The argv plan for any action, including launch and clipboard actions.
    pub fn action_plan(&self, action: &ComputerAction) -> Option<String> {
        match action {
            ComputerAction::AppLaunch { target } => self
                .app_launcher
                .plan(target)
                .ok()
                .map(|plan| plan.describe()),
            ComputerAction::ClipboardRead => {
                self.clipboard.read_plan().ok().map(|plan| plan.describe())
            }
            ComputerAction::ClipboardWrite { text } => self
                .clipboard
                .write_plan(text)
                .ok()
                .map(|plan| plan.describe()),
            _ => self.input.describe_plan(action),
        }
    }

    /// Capture a screenshot after a mutating action, if enabled and possible.
    /// A failed capture never fails the action itself.
    async fn after_shot(&self) -> Option<Screenshot> {
        if !self.capture_after_action {
            return None;
        }
        match self.screenshot_capture.capture().await {
            Ok(shot) => Some(shot),
            Err(e) => {
                tracing::debug!(error = %e, "post-action screenshot unavailable");
                None
            }
        }
    }

    /// Execute a computer action
    pub async fn execute(&self, action: &ComputerAction) -> Result<ActionResult, ControlError> {
        tracing::info!(action = action_kind(action), "Executing computer action");

        match action {
            ComputerAction::Click { x, y, button } => self.perform_click(*x, *y, button).await,
            ComputerAction::DoubleClick { x, y } => {
                self.perform_click(*x, *y, &MouseButton::Left).await?;
                self.perform_click(*x, *y, &MouseButton::Left).await
            }
            ComputerAction::RightClick { x, y } => {
                self.perform_click(*x, *y, &MouseButton::Right).await
            }
            ComputerAction::TypeText { text } => self.perform_type_text(text).await,
            ComputerAction::PressKey { key } => self.perform_key_press(key).await,
            ComputerAction::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => self.perform_scroll(*x, *y, *delta_x, *delta_y).await,
            ComputerAction::MoveMouse { x, y } => self.perform_move_mouse(*x, *y).await,
            ComputerAction::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                duration_ms,
            } => {
                self.perform_drag(*from_x, *from_y, *to_x, *to_y, *duration_ms)
                    .await
            }
            ComputerAction::AppLaunch { target } => self.perform_app_launch(target).await,
            ComputerAction::ClipboardRead => self.perform_clipboard_read().await,
            ComputerAction::ClipboardWrite { text } => self.perform_clipboard_write(text).await,
            ComputerAction::Wait { milliseconds } => {
                tokio::time::sleep(tokio::time::Duration::from_millis(*milliseconds)).await;
                Ok(ActionResult {
                    success: true,
                    message: format!("Waited {}ms", milliseconds),
                    screenshot_after: None,
                    payload: None,
                })
            }
        }
    }

    fn map_button(&self, button: &MouseButton) -> Button {
        match button {
            MouseButton::Left => Button::Left,
            MouseButton::Middle => Button::Middle,
            MouseButton::Right => Button::Right,
        }
    }

    /// Perform a click action
    async fn perform_click(
        &self,
        x: f32,
        y: f32,
        button: &MouseButton,
    ) -> Result<ActionResult, ControlError> {
        self.input
            .click(x, y, self.map_button(button))
            .await
            .map_err(ControlError::ActionFailed)?;
        tracing::info!(x, y, ?button, "Click performed");
        Ok(ActionResult {
            success: true,
            message: format!("Clicked at ({}, {}) with {:?}", x, y, button),
            screenshot_after: self.after_shot().await,
            payload: None,
        })
    }

    /// Perform text typing
    async fn perform_type_text(&self, text: &str) -> Result<ActionResult, ControlError> {
        self.input
            .type_text(text)
            .await
            .map_err(ControlError::ActionFailed)?;
        tracing::info!(len = text.len(), "Typed text");
        Ok(ActionResult {
            success: true,
            message: format!("Typed {} character(s)", text.chars().count()),
            screenshot_after: self.after_shot().await,
            payload: None,
        })
    }

    /// Perform key press
    async fn perform_key_press(&self, key: &str) -> Result<ActionResult, ControlError> {
        self.input
            .press_key(key)
            .await
            .map_err(ControlError::ActionFailed)?;
        tracing::info!(key, "Pressed key");
        Ok(ActionResult {
            success: true,
            message: format!("Pressed key: {}", key),
            screenshot_after: self.after_shot().await,
            payload: None,
        })
    }

    /// Perform scroll
    async fn perform_scroll(
        &self,
        x: f32,
        y: f32,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<ActionResult, ControlError> {
        self.input
            .scroll(x, y, delta_x, delta_y)
            .await
            .map_err(ControlError::ActionFailed)?;
        tracing::info!(x, y, delta_x, delta_y, "Scrolled");
        Ok(ActionResult {
            success: true,
            message: format!("Scrolled at ({}, {}) by ({}, {})", x, y, delta_x, delta_y),
            screenshot_after: self.after_shot().await,
            payload: None,
        })
    }

    /// Perform mouse move
    async fn perform_move_mouse(&self, x: f32, y: f32) -> Result<ActionResult, ControlError> {
        self.input
            .move_mouse(x, y)
            .await
            .map_err(ControlError::ActionFailed)?;
        tracing::info!(x, y, "Moved mouse");
        Ok(ActionResult {
            success: true,
            message: format!("Moved mouse to ({}, {})", x, y),
            screenshot_after: self.after_shot().await,
            payload: None,
        })
    }

    /// Perform a left-button drag between two screen points.
    async fn perform_drag(
        &self,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        duration_ms: Option<u64>,
    ) -> Result<ActionResult, ControlError> {
        self.input
            .drag(from_x, from_y, to_x, to_y, duration_ms)
            .await
            .map_err(ControlError::ActionFailed)?;
        tracing::info!(from_x, from_y, to_x, to_y, ?duration_ms, "Dragged");
        Ok(ActionResult {
            success: true,
            message: format!(
                "Dragged from ({}, {}) to ({}, {}) in {}ms",
                from_x,
                from_y,
                to_x,
                to_y,
                duration_ms.unwrap_or(500)
            ),
            screenshot_after: self.after_shot().await,
            payload: None,
        })
    }

    /// Launch an application, document, or URL. Success means the process was
    /// spawned, not that the application finished starting.
    async fn perform_app_launch(&self, target: &str) -> Result<ActionResult, ControlError> {
        let outcome = self
            .app_launcher
            .launch(target)
            .map_err(ControlError::ActionFailed)?;
        tracing::info!(
            pid = outcome.pid,
            strategy = %outcome.strategy,
            "Launched application"
        );
        Ok(ActionResult {
            success: true,
            message: format!(
                "Launched {} via {} (pid {})",
                target, outcome.strategy, outcome.pid
            ),
            screenshot_after: None,
            payload: Some(serde_json::json!({
                "pid": outcome.pid,
                "strategy": outcome.strategy,
                "program": outcome.program,
                "args": outcome.args,
            })),
        })
    }

    /// Read the clipboard without logging its contents.
    async fn perform_clipboard_read(&self) -> Result<ActionResult, ControlError> {
        let read = self
            .clipboard
            .read_text()
            .await
            .map_err(ControlError::ActionFailed)?;
        let characters = read.text.chars().count();
        tracing::info!(
            bytes = read.bytes,
            truncated = read.truncated,
            "Read clipboard"
        );
        Ok(ActionResult {
            success: true,
            message: format!(
                "Read {characters} character(s) from the clipboard{}",
                if read.truncated { " (truncated)" } else { "" }
            ),
            screenshot_after: None,
            payload: Some(serde_json::json!({
                "text": read.text,
                "bytes": read.bytes,
                "truncated": read.truncated,
                "backend": read.backend,
            })),
        })
    }

    /// Replace the clipboard without logging its contents.
    async fn perform_clipboard_write(&self, text: &str) -> Result<ActionResult, ControlError> {
        let write = self
            .clipboard
            .write_text(text)
            .await
            .map_err(ControlError::ActionFailed)?;
        tracing::info!(bytes = write.bytes, "Wrote clipboard");
        Ok(ActionResult {
            success: true,
            message: format!(
                "Wrote {} character(s) to the clipboard",
                text.chars().count()
            ),
            screenshot_after: None,
            payload: Some(serde_json::json!({
                "bytes": write.bytes,
                "backend": write.backend,
            })),
        })
    }

    /// Take a screenshot and analyze it
    pub async fn observe(&self) -> Result<(Screenshot, AnalysisResult), ControlError> {
        let screenshot = self
            .screenshot_capture
            .capture()
            .await
            .map_err(|e| ControlError::ScreenshotFailed(e.to_string()))?;

        let analysis = self
            .image_analyzer
            .analyze_screenshot(&screenshot)
            .await
            .map_err(|e| ControlError::AnalysisFailed(e.to_string()))?;

        Ok((screenshot, analysis))
    }

    /// Execute a sequence of actions
    pub async fn execute_sequence(
        &self,
        actions: &[ComputerAction],
    ) -> Result<Vec<ActionResult>, ControlError> {
        let mut results = Vec::new();
        let actions_to_run = actions.iter().take(self.max_actions);

        for action in actions_to_run {
            let result = self.execute(action).await?;
            results.push(result);

            // Small delay between actions
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        Ok(results)
    }
}

fn action_kind(action: &ComputerAction) -> &'static str {
    match action {
        ComputerAction::Click { .. } => "click",
        ComputerAction::DoubleClick { .. } => "double_click",
        ComputerAction::RightClick { .. } => "right_click",
        ComputerAction::TypeText { .. } => "type_text",
        ComputerAction::PressKey { .. } => "press_key",
        ComputerAction::Scroll { .. } => "scroll",
        ComputerAction::MoveMouse { .. } => "move_mouse",
        ComputerAction::Drag { .. } => "drag",
        ComputerAction::AppLaunch { .. } => "app_launch",
        ComputerAction::ClipboardRead => "clipboard_read",
        ComputerAction::ClipboardWrite { .. } => "clipboard_write",
        ComputerAction::Wait { .. } => "wait",
    }
}

impl Default for ComputerController {
    fn default() -> Self {
        Self::new()
    }
}
