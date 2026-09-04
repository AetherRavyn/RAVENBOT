//! Screenshot capture capabilities

use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScreenshotError {
    #[error("Capture failed: {0}")]
    CaptureFailed(String),
    #[error("Format error: {0}")]
    FormatError(String),
}

/// A captured screenshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screenshot {
    /// Base64-encoded image data
    pub data: String,
    /// Image format (png, jpeg)
    pub format: String,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Screenshot {
    /// Get the image as a data URL
    pub fn to_data_url(&self) -> String {
        format!("data:image/{};base64,{}", self.format, self.data)
    }

    /// Get raw bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, ScreenshotError> {
        general_purpose::STANDARD
            .decode(&self.data)
            .map_err(|e| ScreenshotError::FormatError(e.to_string()))
    }
}

/// Screenshot capture device
pub struct ScreenshotCapture {
    /// Current capture region (None = full screen)
    region: Option<Region>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenshotCapture {
    pub fn new() -> Self {
        Self { region: None }
    }

    /// Set capture region
    pub fn with_region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    /// Capture a screenshot
    /// In production, this would use platform-specific APIs
    pub async fn capture(&self) -> Result<Screenshot, ScreenshotError> {
        // For now, create a placeholder screenshot
        // In production:
        // - macOS: use screencapturekit
        // - Windows: use BitBlt
        // - Linux: use X11/Wayland APIs
        
        tracing::info!("Capturing screenshot");
        
        // Create a simple 1x1 pixel PNG placeholder
        let placeholder_png = create_placeholder_image();
        
        Ok(Screenshot {
            data: general_purpose::STANDARD.encode(&placeholder_png),
            format: "png".to_string(),
            width: 1920,
            height: 1080,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Capture a specific window
    pub async fn capture_window(&self, _window_id: u64) -> Result<Screenshot, ScreenshotError> {
        self.capture().await
    }
}

impl Default for ScreenshotCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a minimal valid PNG image (1x1 white pixel)
fn create_placeholder_image() -> Vec<u8> {
    // Minimal 1x1 white PNG
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, // IHDR length
        0x49, 0x48, 0x44, 0x52, // IHDR
        0x00, 0x00, 0x00, 0x01, // Width: 1
        0x00, 0x00, 0x00, 0x01, // Height: 1
        0x08, 0x02,             // Bit depth: 8, Color type: 2 (RGB)
        0x00, 0x00, 0x00,       // Compression, filter, interlace
        0x90, 0x77, 0x53, 0xDE, // CRC
        0x00, 0x00, 0x00, 0x0C, // IDAT length
        0x49, 0x44, 0x41, 0x54, // IDAT
        0x08, 0xD7,             // Zlib header
        0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, // Compressed data
        0xE2, 0x21, 0xBC, 0x33, // CRC
        0x00, 0x00, 0x00, 0x00, // IEND length
        0x49, 0x45, 0x4E, 0x44, // IEND
        0xAE, 0x42, 0x60, 0x82, // CRC
    ]
}
