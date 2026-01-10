//! Webcam capture backends.

mod nokhwa_backend;

pub use nokhwa_backend::NokhwaCapture;

use crate::frame::VideoFrame;
use anyhow::Result;

/// Trait for webcam capture backends.
pub trait CaptureBackend {
    /// Returns a list of available camera devices.
    fn list_devices() -> Result<Vec<CameraInfo>>
    where
        Self: Sized;

    /// Opens the camera with the specified configuration.
    fn open(config: CaptureConfig) -> Result<Self>
    where
        Self: Sized;

    /// Captures a single frame from the camera.
    fn capture_frame(&mut self) -> Result<VideoFrame>;

    /// Returns the current frame dimensions.
    fn frame_size(&self) -> (u32, u32);
}

/// Information about a camera device.
#[derive(Debug, Clone)]
pub struct CameraInfo {
    /// Device index
    pub index: u32,
    /// Human-readable name
    pub name: String,
}

/// Configuration for camera capture.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Camera device index
    pub device_index: u32,
    /// Desired frame width
    pub width: u32,
    /// Desired frame height
    pub height: u32,
    /// Desired frame rate
    pub fps: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            device_index: 0,
            width: 1280,
            height: 720,
            fps: 30,
        }
    }
}
