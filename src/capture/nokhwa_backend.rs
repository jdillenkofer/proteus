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
            // Priority: Highest resolution -> Highest framerate -> NV12 > YUYV > MJPEG
            
            // === 1080p @ 30fps ===
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::NV12, 30),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::YUYV, 30),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::MJPEG, 30),
            
            // === 1080p @ 25fps ===
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::NV12, 25),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::YUYV, 25),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::MJPEG, 25),
            
            // === 1080p @ 15fps ===
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::NV12, 15),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::YUYV, 15),
            CameraFormat::new(Resolution::new(1920, 1080), FrameFormat::MJPEG, 15),
            
            // === 720p @ 30fps ===
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::NV12, 30),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::YUYV, 30),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::MJPEG, 30),
            
            // === 720p @ 25fps ===
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::NV12, 25),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::YUYV, 25),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::MJPEG, 25),
            
            // === 720p @ 15fps ===
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::NV12, 15),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::YUYV, 15),
            CameraFormat::new(Resolution::new(1280, 720), FrameFormat::MJPEG, 15),
            
            // === VGA fallbacks (last resort) ===
            CameraFormat::new(Resolution::new(640, 480), FrameFormat::NV12, 30),
            CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 30),
            CameraFormat::new(Resolution::new(640, 480), FrameFormat::MJPEG, 30),
        ];

        let mut camera = None;
        let mut active_format = None;
        
        // Resolve device index from config.device_id (index or name)
        let target_index = if let Ok(idx) = config.device_id.parse::<u32>() {
            CameraIndex::Index(idx)
        } else {
            let devices = nokhwa::query(nokhwa::utils::ApiBackend::Auto)?;
            devices.into_iter()
                .find(|d| d.human_name() == config.device_id)
                .ok_or_else(|| anyhow::anyhow!("Camera not found: {}", config.device_id))?
                .index()
                .clone()
        };

        // Filter seed formats by max dimensions
        let filtered_seeds: Vec<_> = seed_formats.into_iter()
            .filter(|fmt| fmt.width() <= config.max_input_width && fmt.height() <= config.max_input_height)
            .collect();
        
        tracing::info!("Filtered seed formats to {} options (max {}x{})", 
            filtered_seeds.len(), config.max_input_width, config.max_input_height);

        // Try to brute-force open the camera with known standard formats
        for seed in filtered_seeds {
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(seed));
            let idx = target_index.clone();
            
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
        
        let mut camera = camera.ok_or_else(|| anyhow::anyhow!("Could not connect to and open stream on camera '{}' with any standard format.", config.device_id))?;

        // 2. Query supported formats to see if we can upgrade to the user's actual request
        // Sometimes this returns empty, in which case we just stick with the working seed.
        if let Ok(supported_formats) = camera.compatible_camera_formats() {
            if !supported_formats.is_empty() {
                // Find best format: prioritize highest resolution, then framerate, then format type
                let mut best_format = None;
                let mut best_score: i64 = -1;

                for fmt in &supported_formats {
                    // Only consider 16:9 aspect ratio formats
                    // 16:9 means width/height = 16/9 ≈ 1.777...
                    // Check: width * 9 == height * 16
                    if fmt.width() * 9 != fmt.height() * 16 {
                        continue;
                    }
                    
                    // Filter by max dimensions if specified
                    if fmt.width() > config.max_input_width || fmt.height() > config.max_input_height {
                        continue;
                    }
                    
                    let mut score: i64 = 0;
                    
                    // 1. Highest resolution first (primary criterion)
                    // Use total pixels as score multiplier for resolution priority
                    let resolution_score = (fmt.width() as i64) * (fmt.height() as i64);
                    score += resolution_score;
                    
                    // 2. Highest framerate (secondary criterion)
                    // Scale by 1000 to make it significant but less than resolution differences
                    score += (fmt.frame_rate() as i64) * 1000;
                    
                    // 3. Format priority: NV12 > YUYV > MJPEG (tertiary criterion)
                    // Small values so they only break ties between otherwise equal formats
                    match fmt.format() {
                        FrameFormat::NV12 => score += 30,
                        FrameFormat::YUYV => score += 20,
                        FrameFormat::MJPEG => score += 10,
                        _ => {}
                    }

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
