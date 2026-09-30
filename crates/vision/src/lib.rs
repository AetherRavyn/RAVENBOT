//! RAVENBOT vision and multimodal capabilities
//!
//! This crate provides screenshot capture, image analysis,
//! and computer control through vision models.

pub mod screenshot;
pub mod image_analysis;
pub mod computer_control;
pub mod audio;

pub use screenshot::ScreenshotCapture;
pub use image_analysis::ImageAnalyzer;
pub use computer_control::ComputerController;
pub use audio::{AudioTranscriber, TextToSpeech};
