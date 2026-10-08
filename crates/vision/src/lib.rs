//! RAVENBOT vision and multimodal capabilities
//!
//! This crate provides screenshot capture, image analysis,
//! and computer control through vision models.

pub mod screenshot;
pub mod image_analysis;
pub mod computer_control;
pub mod input;
pub mod audio;

pub use screenshot::ScreenshotCapture;
pub use image_analysis::ImageAnalyzer;
pub use computer_control::{ComputerController, ComputerAction, MouseButton, ActionResult};
pub use input::InputInjector;
pub use audio::{AudioTranscriber, TextToSpeech};

/// Resize image bytes so the longest edge ≤ max_edge. Returns PNG bytes.
/// Used to keep vision prompts reasonable (most providers downsample anyway,
/// but this avoids multi-MB base64 in the conversation).
pub fn resize_for_vision(image_bytes: &[u8], max_edge: u32) -> Result<Vec<u8>, image::ImageError> {
    let img = image::load_from_memory(image_bytes)?;
    let (w, h) = (img.width(), img.height());
    let longest = w.max(h);
    let resized = if longest > max_edge {
        let scale = max_edge as f32 / longest as f32;
        let nw = (w as f32 * scale).max(1.0) as u32;
        let nh = (h as f32 * scale).max(1.0) as u32;
        img.resize(nw, nh, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut out: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut out);
    resized.write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(out)
}
