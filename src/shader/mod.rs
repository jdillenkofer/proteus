//! GPU shader pipeline.

mod wgpu_pipeline;

pub use wgpu_pipeline::{TextureSlot, WgpuPipeline};

use crate::frame::VideoFrame;
use anyhow::Result;

/// Trait for shader processing pipelines.
pub trait ShaderPipeline {
    /// Process a video frame through the shader.
    /// `time` is the elapsed time in seconds since the application started.
    fn process_frame(&mut self, input: &VideoFrame, time: f32) -> Result<VideoFrame>;
}

/// Shader source with language specification.
#[derive(Debug, Clone)]
pub enum ShaderSource {
    /// GLSL fragment shader source code
    Glsl(String),
    /// WGSL shader source code  
    Wgsl(String),
}
