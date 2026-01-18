//! Output backends for displaying processed video.

pub mod window_output;

#[cfg(target_os = "macos")]
#[path = "virtual_camera_macos.rs"]
pub mod virtual_camera;

#[cfg(target_os = "windows")]
#[path = "virtual_camera_windows.rs"]
pub mod virtual_camera;

#[cfg(target_os = "linux")]
#[path = "virtual_camera_linux.rs"]
pub mod virtual_camera;

pub use window_output::WindowOutput;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub use virtual_camera::{VirtualCameraConfig, VirtualCameraOutput};

use crate::frame::VideoFrame;
use anyhow::Result;

/// Trait for video output backends.
pub trait OutputBackend {
    /// Write a frame to the output.
    fn write_frame(&mut self, frame: &VideoFrame) -> Result<()>;
}
