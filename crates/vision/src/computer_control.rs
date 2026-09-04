//! Computer control through vision-guided actions

use crate::screenshot::{Screenshot, ScreenshotCapture};
use crate::image_analysis::{ImageAnalyzer, AnalysisResult};
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
    Scroll { x: f32, y: f32, delta_x: i32, delta_y: i32 },
    /// Move mouse to coordinates
    MoveMouse { x: f32, y: f32 },
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
}

/// Vision-guided computer controller
pub struct ComputerController {
    screenshot_capture: ScreenshotCapture,
    image_analyzer: ImageAnalyzer,
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
            capture_after_action: true,
            max_actions: 50,
        }
    }

    /// Execute a computer action
    pub async fn execute(&self, action: &ComputerAction) -> Result<ActionResult, ControlError> {
        tracing::info!(action = ?action, "Executing computer action");

        match action {
            ComputerAction::Click { x, y, button } => {
                self.perform_click(*x, *y, button).await
            }
            ComputerAction::DoubleClick { x, y } => {
                self.perform_click(*x, *y, &MouseButton::Left).await?;
                self.perform_click(*x, *y, &MouseButton::Left).await
            }
            ComputerAction::RightClick { x, y } => {
                self.perform_click(*x, *y, &MouseButton::Right).await
            }
            ComputerAction::TypeText { text } => {
                self.perform_type_text(text).await
            }
            ComputerAction::PressKey { key } => {
                self.perform_key_press(key).await
            }
            ComputerAction::Scroll { x, y, delta_x, delta_y } => {
                self.perform_scroll(*x, *y, *delta_x, *delta_y).await
            }
            ComputerAction::MoveMouse { x, y } => {
                self.perform_move_mouse(*x, *y).await
            }
            ComputerAction::Wait { milliseconds } => {
                tokio::time::sleep(tokio::time::Duration::from_millis(*milliseconds)).await;
                Ok(ActionResult {
                    success: true,
                    message: format!("Waited {}ms", milliseconds),
                    screenshot_after: None,
                })
            }
        }
    }

    /// Perform a click action
    async fn perform_click(&self, x: f32, y: f32, button: &MouseButton) -> Result<ActionResult, ControlError> {
        // In production, use platform-specific APIs
        // - macOS: CGEvent
        // - Windows: SendInput
        // - Linux: xdotool
        
        tracing::info!(x = x, y = y, button = ?button, "Click performed");
        
        let screenshot_after = if self.capture_after_action {
            Some(self.screenshot_capture.capture().await
                .map_err(|e| ControlError::ScreenshotFailed(e.to_string()))?)
        } else {
            None
        };

        Ok(ActionResult {
            success: true,
            message: format!("Clicked at ({}, {}) with {:?}", x, y, button),
            screenshot_after,
        })
    }

    /// Perform text typing
    async fn perform_type_text(&self, text: &str) -> Result<ActionResult, ControlError> {
        tracing::info!(text = %text, "Typing text");
        
        Ok(ActionResult {
            success: true,
            message: format!("Typed: {}", text),
            screenshot_after: None,
        })
    }

    /// Perform key press
    async fn perform_key_press(&self, key: &str) -> Result<ActionResult, ControlError> {
        tracing::info!(key = %key, "Pressing key");
        
        Ok(ActionResult {
            success: true,
            message: format!("Pressed key: {}", key),
            screenshot_after: None,
        })
    }

    /// Perform scroll
    async fn perform_scroll(&self, x: f32, y: f32, delta_x: i32, delta_y: i32) -> Result<ActionResult, ControlError> {
        tracing::info!(x = x, y = y, delta_x = delta_x, delta_y = delta_y, "Scrolling");
        
        Ok(ActionResult {
            success: true,
            message: format!("Scrolled at ({}, {}) by ({}, {})", x, y, delta_x, delta_y),
            screenshot_after: None,
        })
    }

    /// Perform mouse move
    async fn perform_move_mouse(&self, x: f32, y: f32) -> Result<ActionResult, ControlError> {
        tracing::info!(x = x, y = y, "Moving mouse");
        
        Ok(ActionResult {
            success: true,
            message: format!("Moved mouse to ({}, {})", x, y),
            screenshot_after: None,
        })
    }

    /// Take a screenshot and analyze it
    pub async fn observe(&self) -> Result<(Screenshot, AnalysisResult), ControlError> {
        let screenshot = self.screenshot_capture.capture().await
            .map_err(|e| ControlError::ScreenshotFailed(e.to_string()))?;
        
        let analysis = self.image_analyzer.analyze_screenshot(&screenshot).await
            .map_err(|e| ControlError::AnalysisFailed(e.to_string()))?;
        
        Ok((screenshot, analysis))
    }

    /// Execute a sequence of actions
    pub async fn execute_sequence(&self, actions: &[ComputerAction]) -> Result<Vec<ActionResult>, ControlError> {
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

impl Default for ComputerController {
    fn default() -> Self {
        Self::new()
    }
}
