//! Output backends for displaying processed video.

pub mod window_output;

pub use window_output::WindowOutput;

use crate::frame::VideoFrame;
use anyhow::Result;

/// Trait for video output backends.
pub trait OutputBackend {
    /// Write a frame to the output.
    fn write_frame(&mut self, frame: &VideoFrame) -> Result<()>;
}
