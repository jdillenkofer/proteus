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
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Default v4l2loopback device path.
const DEFAULT_DEVICE: &str = "/dev/video10";

// V4L2 Constants
const VIDIOC_S_FMT: u64 = 0xC0D05605; // _IOWR('V', 5, struct v4l2_format)
const V4L2_BUF_TYPE_VIDEO_OUTPUT: u32 = 2;
const V4L2_PIX_FMT_YUYV: u32 = 0x56595559; // 'Y' 'U' 'Y' 'V'

#[repr(C)]
struct v4l2_format {
    type_: u32,
    fmt: v4l2_format_union,
}

#[repr(C)]
union v4l2_format_union {
    pix: v4l2_pix_format,
    raw_data: [u8; 200], // Adjusted for 64-bit alignment (4+4+200 = 208 bytes)
    _align: u64, // Force 8-byte alignment for the union
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct v4l2_pix_format {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    priv_: u32,
    flags: u32,
    ycbcr_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

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
        // Try to open the device and configure it
        let device = Self::open_and_configure_device(&config)?;

        info!(
            "Virtual camera output created on {} ({}x{} @ {} fps, YUYV)",
            config.device.display(),
            config.width,
            config.height,
            config.fps
        );
        info!("Select the v4l2loopback camera in your video application");

        Ok(Self { config, device })
    }

    /// Open the v4l2loopback device and configure format.
    fn open_and_configure_device(config: &VirtualCameraConfig) -> Result<File> {
        let path = &config.device;

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
            .read(true) // Read permission required for ioctl? Usually yes for getting/setting format
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

        // Configure format using ioctl
        let fd = file.as_raw_fd();
        
        let pix = v4l2_pix_format {
            width: config.width,
            height: config.height,
            pixelformat: V4L2_PIX_FMT_YUYV,
            field: 0, // V4L2_FIELD_ANY / V4L2_FIELD_NONE
            bytesperline: config.width * 2, // YUYV is 2 bytes per pixel
            sizeimage: config.width * config.height * 2,
            colorspace: 8, // V4L2_COLORSPACE_SRGB
            priv_: 0,
            flags: 0,
            ycbcr_enc: 0,
            quantization: 0,
            xfer_func: 0,
        };

        let mut fmt = v4l2_format {
            type_: V4L2_BUF_TYPE_VIDEO_OUTPUT,
            fmt: v4l2_format_union { pix },
        };

        unsafe {
            if libc::ioctl(fd, VIDIOC_S_FMT, &mut fmt) < 0 {
                let err = std::io::Error::last_os_error();
                warn!("Failed to set v4l2 format: {}. Output might be incorrect.", err);
                // We don't fail here because some devices might not support S_FMT but still work?
                // But for v4l2loopback it is crucial.
            } else {
                debug!("Successfully set v4l2 format to YUYV {}x{}", config.width, config.height);
            }
        }

        debug!("Opened v4l2loopback device: {}", path.display());
        Ok(file)
    }

    fn write_frame_internal(&mut self, frame: &VideoFrame) -> Result<()> {
        // v4l2loopback typically accepts raw pixel data
        // Convert to YUYV (most standard webcam format)
        let yuyv = frame.to_yuyv();

        // Write the raw YUYV data to the device
        self.device.write_all(&yuyv.data).map_err(|e| {
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
