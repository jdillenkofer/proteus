//! Output backends for displaying processed video.

pub mod window_output;

#[cfg(target_os = "windows")]
pub mod virtual_camera;

pub use window_output::WindowOutput;

#[cfg(target_os = "windows")]
pub use virtual_camera::{VirtualCameraConfig, VirtualCameraOutput};

use crate::frame::VideoFrame;
use anyhow::Result;

/// Trait for video output backends.
pub trait OutputBackend {
    /// Write a frame to the output.
    fn write_frame(&mut self, frame: &VideoFrame) -> Result<()>;
}
