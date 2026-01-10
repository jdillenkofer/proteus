//! Linux virtual camera output using v4l2loopback.
//!
//! This module writes frames to a v4l2loopback virtual video device.
//! Requires v4l2loopback kernel module to be loaded.

use super::OutputBackend;
use crate::frame::VideoFrame;
use anyhow::{anyhow, Result};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Default v4l2loopback device path.
const DEFAULT_DEVICE: &str = "/dev/video10";

/// Configuration for virtual camera output.
#[derive(Debug, Clone)]
pub struct VirtualCameraConfig {
    /// Device path (e.g., /dev/video10)
    pub device: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for VirtualCameraConfig {
    fn default() -> Self {
        Self {
            device: PathBuf::from(DEFAULT_DEVICE),
            width: 1280,
            height: 720,
            fps: 30,
        }
    }
}

/// Virtual camera output using v4l2loopback.
pub struct VirtualCameraOutput {
    config: VirtualCameraConfig,
    device: File,
}

impl VirtualCameraOutput {
    /// Creates a new virtual camera output.
    ///
    /// This opens the v4l2loopback device for writing frames.
    pub fn new(config: VirtualCameraConfig) -> Result<Self> {
        // Try to open the device
        let device = Self::open_device(&config.device)?;

        info!(
            "Virtual camera output created on {} ({}x{} @ {} fps)",
            config.device.display(),
            config.width,
            config.height,
            config.fps
        );
        info!("Select the v4l2loopback camera in your video application");

        Ok(Self { config, device })
    }

    /// Open the v4l2loopback device.
    fn open_device(path: &PathBuf) -> Result<File> {
        // Check if device exists
        if !path.exists() {
            return Err(anyhow!(
                "v4l2loopback device '{}' not found. \n\
                Make sure v4l2loopback is loaded:\n  \
                sudo modprobe v4l2loopback devices=1 video_nr=10 card_label=\"Proteus Camera\" exclusive_caps=1",
                path.display()
            ));
        }

        // Open for writing with non-blocking mode
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| {
                anyhow!(
                    "Failed to open v4l2loopback device '{}': {}. \n\
                    Make sure you have write permissions or run with sudo.",
                    path.display(),
                    e
                )
            })?;

        debug!("Opened v4l2loopback device: {}", path.display());
        Ok(file)
    }

    /// Write a frame to the v4l2loopback device.
    fn write_frame_internal(&mut self, frame: &VideoFrame) -> Result<()> {
        // v4l2loopback typically accepts raw pixel data
        // Convert to RGBA first, then write
        let rgba = frame.to_rgba();

        // Write the raw RGBA data to the device
        self.device.write_all(&rgba.data).map_err(|e| {
            // Non-blocking write might fail if buffer is full, that's OK
            if e.kind() == std::io::ErrorKind::WouldBlock {
                warn!("v4l2loopback buffer full, frame dropped");
                return anyhow!("Buffer full");
            }
            anyhow!("Failed to write to v4l2loopback: {}", e)
        })?;

        Ok(())
    }
}

impl Drop for VirtualCameraOutput {
    fn drop(&mut self) {
        debug!("Virtual camera output closed");
    }
}

impl OutputBackend for VirtualCameraOutput {
    fn write_frame(&mut self, frame: &VideoFrame) -> Result<()> {
        self.write_frame_internal(frame)
    }
}
