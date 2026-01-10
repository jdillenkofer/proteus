//! Nokhwa-based webcam capture backend.

use super::{CameraInfo, CaptureBackend, CaptureConfig};
use crate::frame::{PixelFormat, VideoFrame};
use anyhow::Result;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

/// Webcam capture using the nokhwa library.
pub struct NokhwaCapture {
    camera: Camera,
    width: u32,
    height: u32,
}

impl CaptureBackend for NokhwaCapture {
    fn list_devices() -> Result<Vec<CameraInfo>> {
        let devices = nokhwa::query(nokhwa::utils::ApiBackend::Auto)?;
        Ok(devices
            .into_iter()
            .map(|d| CameraInfo {
                index: d.index().as_index().unwrap_or(0),
                name: d.human_name().to_string(),
            })
            .collect())
    }

    fn open(config: CaptureConfig) -> Result<Self> {
        let index = CameraIndex::Index(config.device_index);
        // Use highest resolution available, then we'll get actual resolution after opening
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);

        let mut camera = Camera::new(index, requested)?;
        camera.open_stream()?;

        let resolution = camera.resolution();

        Ok(Self {
            camera,
            width: resolution.width(),
            height: resolution.height(),
        })
    }

    fn capture_frame(&mut self) -> Result<VideoFrame> {
        let frame = self.camera.frame()?;
        let decoded = frame.decode_image::<RgbFormat>()?;
        let rgb_data = decoded.into_raw();

        Ok(VideoFrame::from_data(
            self.width,
            self.height,
            PixelFormat::Rgb,
            rgb_data,
        ))
    }

    fn frame_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
