use anyhow::{anyhow, Result};
use image::{imageops::FilterType, DynamicImage, ImageBuffer, Luma, Rgba};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use tracing::{info, warn, debug};

use crate::frame::VideoFrame;

const MODEL_WIDTH: u32 = 256;
const MODEL_HEIGHT: u32 = 256;

pub struct SegmentationEngine {
    session: Session,
}

impl SegmentationEngine {
    /// Initialize the ONNX Runtime environment.
    /// This should be called once at startup.
    pub fn init() -> Result<()> {
        ort::init()
            .with_name("proteus")
            .commit();
        Ok(())
    }

    /// Try to load the segmentation model.
    /// Returns None if the model file is not found.
    pub fn new() -> Result<Option<Self>> {
        let model_path = Path::new("models/modnet.onnx");
        if !model_path.exists() {
            warn!("Segmentation model not found at {:?}. Background blur will blur EVERYTHING.", model_path);
            return Ok(None);
        }

        info!("Loading segmentation model from {:?}", model_path);
        
        let mut session_builder = Session::builder()?;
        session_builder = session_builder.with_optimization_level(GraphOptimizationLevel::Level3)?;
        session_builder = session_builder.with_intra_threads(4)?;
        
        // --- Mac Optimization: CoreML ---
        #[cfg(target_os = "macos")]
        {
            use ort::execution_providers::CoreMLExecutionProvider;
            session_builder = session_builder.with_execution_providers([
                CoreMLExecutionProvider::default().build()
            ])?;
            info!("CoreML Execution Provider enabled");
        }
    
        let session = session_builder.commit_from_file(model_path)?;

        Ok(Some(Self { session }))
    }

    /// Run inference on a video frame and return the alpha mask (grayscale 8-bit)
    pub fn predict(&mut self, frame: &VideoFrame) -> Result<(Vec<u8>, u32, u32)> {
        // 1. Resize via image library (copy is necessary for DynamicImage)
        let img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(frame.width, frame.height, frame.data.clone())
            .ok_or_else(|| anyhow!("Failed to create image buffer"))?;
        let dynamic_img = DynamicImage::ImageRgba8(img);
        
        let ratio = (MODEL_WIDTH as f32 / frame.width as f32).min(MODEL_HEIGHT as f32 / frame.height as f32);
        let new_width = (frame.width as f32 * ratio).round() as u32;
        let new_height = (frame.height as f32 * ratio).round() as u32;
        
        let resized = dynamic_img.resize_exact(new_width, new_height, FilterType::Triangle);
        let mut padded = ImageBuffer::from_pixel(MODEL_WIDTH, MODEL_HEIGHT, Rgba([0, 0, 0, 0]));
        
        let x_offset = (MODEL_WIDTH - new_width) / 2;
        let y_offset = (MODEL_HEIGHT - new_height) / 2;
        image::imageops::overlay(&mut padded, &resized, x_offset as i64, y_offset as i64);
        
        // 3. Fast Vectorized Preprocessing
        // Convert Rgba8 to f32 Plane-major [1, 3, 512, 512] with normalization (val - 0.5) / 0.5
        let mut input_data = vec![0.0f32; 1 * 3 * MODEL_HEIGHT as usize * MODEL_WIDTH as usize];
        let plane_size = (MODEL_HEIGHT * MODEL_WIDTH) as usize;
        
        // Direct pointer access/iterator is faster than enumerate_pixels in debug AND release
        let raw_pixels = padded.as_flat_samples();
        let samples = raw_pixels.as_slice();
        
        for i in 0..plane_size {
            let r = samples[i * 4] as f32;
            let g = samples[i * 4 + 1] as f32;
            let b = samples[i * 4 + 2] as f32;
            
            // (v / 255.0 - 0.5) / 0.5  => (v / 127.5) - 1.0
            input_data[i] = (r / 127.5) - 1.0;
            input_data[i + plane_size] = (g / 127.5) - 1.0;
            input_data[i + 2 * plane_size] = (b / 127.5) - 1.0;
        }

        // 4. Run inference
        let input_value = Value::from_array(([1, 3, MODEL_HEIGHT as i64, MODEL_WIDTH as i64], input_data))?;
        let inputs = ort::inputs!["input" => &input_value];
        let outputs = self.session.run(inputs)?;
        
        let (_, data) = outputs["output"].try_extract_tensor::<f32>()?;
        
        // Post-process: Extract matte [1, 1, 512, 512] -> grayscale bytes
        let mut mask_bytes = vec![0u8; plane_size];
        for i in 0..plane_size {
            mask_bytes[i] = (data[i].clamp(0.0, 1.0) * 255.0) as u8;
        }
        
        let mask_img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(MODEL_WIDTH, MODEL_HEIGHT, mask_bytes)
            .ok_or_else(|| anyhow!("Failed to reconstruct mask"))?;
        
        // Crop back to valid region synchronously
        let mask_cropped = DynamicImage::ImageLuma8(mask_img).crop_imm(x_offset, y_offset, new_width, new_height);
        
        Ok((mask_cropped.into_luma8().into_raw(), new_width, new_height))
    }
}

/// A background-threaded wrapper for the segmentation engine.
pub struct AsyncSegmentationEngine {
    frame_tx: mpsc::SyncSender<VideoFrame>,
    mask_rx: Receiver<(Vec<u8>, u32, u32)>,
}

impl AsyncSegmentationEngine {
    pub fn new() -> Result<Option<Self>> {
        let mut engine_opt = SegmentationEngine::new()?;
        let Some(mut engine) = engine_opt.take() else {
            return Ok(None);
        };

        // Use a bounded channel of size 1 to implement "drop-if-busy"
        let (frame_tx, frame_rx) = mpsc::sync_channel::<VideoFrame>(1);
        let (mask_tx, mask_rx) = mpsc::channel::<(Vec<u8>, u32, u32)>();

        thread::spawn(move || {
            info!("ML Worker Thread started (Zero-Backpressure mode)");
            while let Ok(frame) = frame_rx.recv() {
                let start = std::time::Instant::now();
                match engine.predict(&frame) {
                    Ok(result) => {
                        debug!("ML Worker Inference: {:?}", start.elapsed());
                        if mask_tx.send(result).is_err() {
                            break;
                        }
                    }
                    Err(e) => warn!("ML Worker error: {}", e),
                }
                // Clear any "stale" frames that might have queued up during processing
                // (Though with size 1, there's at most one stale frame).
                // Actually, the sync_channel(1) + try_send already handles this.
            }
            info!("ML Worker Thread exiting");
        });

        Ok(Some(Self { frame_tx, mask_rx }))
    }

    /// Try to send a frame for processing. Returns true if sent, false if busy.
    pub fn try_predict(&self, frame: VideoFrame) -> bool {
        match self.frame_tx.try_send(frame) {
            Ok(_) => true,
            Err(e) => {
                if let mpsc::TrySendError::Full(_) = e {
                    debug!("ML Worker busy, dropping frame to maintain real-time sync");
                }
                false
            }
        }
    }

    /// Get the latest available result from the background thread.
    pub fn poll_result(&self) -> Option<(Vec<u8>, u32, u32)> {
        let mut latest = None;
        // Drain the channel to get the MOST RECENT result
        while let Ok(result) = self.mask_rx.try_recv() {
            latest = Some(result);
        }
        latest
    }
}
