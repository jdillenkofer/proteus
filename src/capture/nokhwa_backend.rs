//! Nokhwa-based webcam capture backend.

use super::{CameraInfo, CaptureBackend, CaptureConfig};
use crate::frame::{PixelFormat, VideoFrame};
use anyhow::Result;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};
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
        // 1. Initialize logic: Try multiple "seed" formats to establish connection
        // Some cameras are very picky and will reject "Closest" if the hint doesn't match roughly what they support.
        // 
        // NOTE: macOS FaceTime cameras typically DON'T support MJPEG, they use NV12/YUYV.
        // USB webcams on all platforms typically support MJPEG for high resolutions.
        // We try uncompressed high-res first (for macOS built-in), then MJPEG (for USB cameras).
        let seed_formats = vec![
            // === High-res uncompressed (macOS FaceTime cameras, some USB cameras) ===
            // NV12 is the native format for many macOS cameras
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::NV12, 30),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::NV12, 30),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::YUYV, 30),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::YUYV, 30),
            
            // === MJPEG (USB webcams - hardware compressed, lower bandwidth) ===
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::MJPEG, 30),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::MJPEG, 25),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::MJPEG, 30),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::MJPEG, 25),
            
            // === Lower FPS variants (for bandwidth-constrained scenarios) ===
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::NV12, 15),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::NV12, 15),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::MJPEG, 15),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::MJPEG, 15),
            
            // === VGA fallbacks (last resort) ===
            CameraFormat::new(Resolution::new(640, 480), FrameFormat::NV12, 30),
            CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 30),
            CameraFormat::new(Resolution::new(640, 480), FrameFormat::MJPEG, 30),
        ];

        let mut camera = None;
        let mut active_format = None;
        
        // Try to brute-force open the camera with known standard formats
        for seed in seed_formats {
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(seed));
            let idx = CameraIndex::Index(config.device_index);
            
            // Try to create the camera instance
            if let Ok(mut cam) = Camera::new(idx, requested) {
                // CRITICAL: Verify if it actually opens the stream!
                // Just creating the object isn't enough for some drivers.
                if cam.open_stream().is_ok() {
                    tracing::info!("Verified connection with seed format: {:?}", seed);
                    active_format = Some(seed);
                    camera = Some(cam);
                    break;
                }
            }
        }
        
        let mut camera = camera.ok_or_else(|| anyhow::anyhow!("Could not connect to and open stream on camera index {} with any standard format.", config.device_index))?;

        // 2. Query supported formats to see if we can upgrade to the user's actual request
        // Sometimes this returns empty, in which case we just stick with the working seed.
        if let Ok(supported_formats) = camera.compatible_camera_formats() {
            if !supported_formats.is_empty() {
                // Find best match for USER CONFIG
                let target_res = Resolution::new(config.width, config.height);
                let mut best_format = None;
                let mut best_score = -10000;

                for fmt in &supported_formats {
                    let mut score = 0;
                    if fmt.resolution() == target_res { score += 1000; }
                    else {
                         let w_diff = (fmt.width() as i32 - config.width as i32).abs();
                         let h_diff = (fmt.height() as i32 - config.height as i32).abs();
                         score -= w_diff + h_diff;
                    }
                    if fmt.frame_rate() == config.fps { score += 500; }
                    else if fmt.frame_rate() > config.fps { score += 100; }
                    // Strongly prefer MJPEG - it has hardware decode and less USB bandwidth
                    if fmt.format() == FrameFormat::MJPEG { score += 200; }
                    // Avoid NV12/YUV on Windows - software conversion is slow
                    if fmt.format() == FrameFormat::NV12 { score -= 100; }
                    if fmt.format() == FrameFormat::YUYV { score -= 50; }

                    if score > best_score {
                        best_score = score;
                        best_format = Some(*fmt);
                    }
                }

                if let Some(better) = best_format {
                    // Only switch if it's different/better than what we have active
                    tracing::info!("Attempting to upgrade to better format: {:?}", better);
                    
                    let _ = camera.stop_stream(); 
                    if let Ok(_) = camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(better))) {
                         if let Err(e) = camera.open_stream() {
                             tracing::warn!("Failed to open stream with better format ({}), trying fallback...", e);
                             // If upgrade failed, try to reopen with the original seed
                             // This is a "best effort" recovery
                             if let Some(seed) = active_format {
                                 let _ = camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(seed)));
                                 let _ = camera.open_stream();
                             }
                         }
                    } else {
                         // Failed to set request, try to re-open existing
                         let _ = camera.open_stream(); 
                    }
                }
            } else {
                tracing::warn!("Device reported empty supported formats list. Using fallback format.");
            }
        }

        let resolution = camera.resolution();
        tracing::info!("Camera opened with resolution: {}", resolution);

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
