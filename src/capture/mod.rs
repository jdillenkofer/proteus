//! Webcam capture backends.

mod nokhwa_backend;

pub use nokhwa_backend::NokhwaCapture;

use crate::frame::VideoFrame;
use anyhow::Result;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, info};

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
    /// Camera device ID (index or name)
    pub device_id: String,
    /// Desired frame width
    pub width: u32,
    /// Desired frame height
    pub height: u32,
    /// Maximum frame width (for camera format selection)
    pub max_input_width: u32,
    /// Maximum frame height (for camera format selection)
    pub max_input_height: u32,
    /// Desired frame rate
    pub fps: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            device_id: "0".to_string(),
            width: 1920,
            height: 1080,
            max_input_width: 1920,
            max_input_height: 1080,
            fps: 30,
        }
    }
}

/// Async capture wrapper that runs camera capture in a background thread.
/// This decouples frame acquisition from the render loop to improve FPS.
/// 
/// Since nokhwa's Camera is not Send, we spawn the thread first and create
/// the camera inside the thread, then communicate via channels.
pub struct AsyncCapture {
    frame_rx: mpsc::Receiver<VideoFrame>,
    latest_frame: Option<VideoFrame>,
    width: u32,
    height: u32,
    running: Arc<AtomicBool>,
}

impl AsyncCapture {
    /// Creates a new async capture wrapper.
    /// The camera is created inside the background thread since it's not Send.
    pub fn new(config: CaptureConfig) -> Result<Self> {
        // Channel for frames from the capture thread
        let (frame_tx, frame_rx) = mpsc::sync_channel::<VideoFrame>(2);
        
        // Channel for initial setup result (size or error)
        let (setup_tx, setup_rx) = mpsc::channel::<Result<(u32, u32)>>();
        
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        
        std::thread::spawn(move || {
            // Create camera inside the thread
            let mut capture = match NokhwaCapture::open(config) {
                Ok(c) => c,
                Err(e) => {
                    let _ = setup_tx.send(Err(e));
                    return;
                }
            };
            
            let size = capture.frame_size();
            if setup_tx.send(Ok(size)).is_err() {
                return;
            }
            
            info!("Camera capture thread started");
            while running_clone.load(Ordering::Relaxed) {
                let capture_start = std::time::Instant::now();
                match capture.capture_frame() {
                    Ok(frame) => {
                        let capture_elapsed = capture_start.elapsed();
                        debug!("[Perf] Camera capture_frame: {:?}", capture_elapsed);
                        // Use try_send to drop frames if the receiver is slow
                        match frame_tx.try_send(frame) {
                            Ok(_) => {},
                            Err(mpsc::TrySendError::Full(_)) => {
                                debug!("Render loop slow, dropping camera frame to maintain real-time");
                            },
                            Err(mpsc::TrySendError::Disconnected(_)) => {
                                info!("Camera capture thread: receiver disconnected, exiting");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Camera capture error: {}", e);
                    }
                }
            }
            info!("Camera capture thread exiting");
        });
        
        // Wait for setup result
        let (width, height) = setup_rx.recv()
            .map_err(|_| anyhow::anyhow!("Camera thread failed to start"))??;
        
        Ok(Self {
            frame_rx,
            latest_frame: None,
            width,
            height,
            running,
        })
    }
    
    /// Gets the latest available frame, or returns the previous frame if none available.
    /// This never blocks - it returns immediately with whatever is available.
    pub fn get_latest_frame(&mut self) -> Option<&VideoFrame> {
        // Drain all available frames and keep the latest
        while let Ok(frame) = self.frame_rx.try_recv() {
            self.latest_frame = Some(frame);
        }
        self.latest_frame.as_ref()
    }
    
    /// Returns the frame dimensions.
    pub fn frame_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl Drop for AsyncCapture {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
