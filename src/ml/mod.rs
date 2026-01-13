use anyhow::{anyhow, Result};
use image::{imageops::FilterType, GrayImage, ImageBuffer, Rgba, RgbImage, Rgb};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use tracing::{info, warn, debug};

use crate::frame::VideoFrame;

// Landscape input resolution (256x144) - optimized for 16:9 webcam feeds
// Note: Width x Height in image terms, model uses NCHW format [1, 3, 144, 256]
const MODEL_WIDTH: u32 = 256;
const MODEL_HEIGHT: u32 = 144;

// Embed the ONNX model directly into the binary
const SELFIE_MODEL_BYTES: &[u8] = include_bytes!("../../models/mediapipe_selfie.onnx");

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

    /// Load the embedded segmentation model.
    pub fn new() -> Result<Option<Self>> {
        info!("Loading embedded segmentation model");
        
        let mut session_builder = Session::builder()?;
        session_builder = session_builder.with_optimization_level(GraphOptimizationLevel::Level3)?;
        session_builder = session_builder.with_intra_threads(4)?;
        
        // --- Mac Optimization: CoreML ---
        #[cfg(target_os = "macos")]
        {
            use ort::ep::CoreMLExecutionProvider;
            session_builder = session_builder.with_execution_providers([
                CoreMLExecutionProvider::default().build()
            ])?;
            info!("CoreML Execution Provider enabled");
        }

        // --- Windows Optimization: DirectML (GPU) ---
        #[cfg(target_os = "windows")]
        {
            use ort::ep::DirectMLExecutionProvider;
            session_builder = session_builder.with_execution_providers([
                DirectMLExecutionProvider::default().build()
            ])?;
            info!("DirectML Execution Provider enabled (GPU acceleration)");
        }

        // --- Linux Optimization: CUDA / ROCm (GPU) ---
        #[cfg(target_os = "linux")]
        {
            #[allow(unused_mut)]
            let mut providers = Vec::new();

            #[cfg(feature = "cuda")]
            {
                use ort::ep::CUDAExecutionProvider;
                let p = CUDAExecutionProvider::default().build();
                providers.push(p);
                info!("CUDA Execution Provider registered");
            }

            #[cfg(feature = "rocm")]
            {
                use ort::ep::ROCmExecutionProvider;
                let p = ROCmExecutionProvider::default().build();
                providers.push(p);
                info!("ROCm Execution Provider registered");
            }
            
            if !providers.is_empty() {
                 session_builder = session_builder.with_execution_providers(providers)?;
            }
        }
    
        let session = session_builder.commit_from_memory(SELFIE_MODEL_BYTES)?;

        Ok(Some(Self { session }))
    }

    /// Run inference on a video frame and return the alpha mask at original resolution
    pub fn predict(&mut self, frame: &VideoFrame) -> Result<(Vec<u8>, u32, u32)> {
        let orig_w = frame.width;
        let orig_h = frame.height;
        
        // 1. Create RGB image from RGBA frame
        let rgba_img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(orig_w, orig_h, frame.data.clone())
            .ok_or_else(|| anyhow!("Failed to create image buffer"))?;
        
        // Convert RGBA to RGB
        let rgb_img: RgbImage = ImageBuffer::from_fn(orig_w, orig_h, |x, y| {
            let p = rgba_img.get_pixel(x, y);
            Rgb([p[0], p[1], p[2]])
        });
        
        // 2. Letterbox: resize maintaining aspect ratio, pad to MODEL_WIDTH x MODEL_HEIGHT
        let scale = (MODEL_WIDTH as f32 / orig_w as f32).min(MODEL_HEIGHT as f32 / orig_h as f32);
        let scaled_w = (orig_w as f32 * scale).round() as u32;
        let scaled_h = (orig_h as f32 * scale).round() as u32;
        
        // Resize preserving aspect ratio (use Triangle for smoother edges)
        let resized = image::imageops::resize(&rgb_img, scaled_w, scaled_h, FilterType::Triangle);
        
        // Create black canvas and paste resized image centered
        let offset_x = (MODEL_WIDTH - scaled_w) / 2;
        let offset_y = (MODEL_HEIGHT - scaled_h) / 2;
        
        // Create black RGB canvas
        let mut canvas: RgbImage = ImageBuffer::from_pixel(MODEL_WIDTH, MODEL_HEIGHT, Rgb([0, 0, 0]));
        
        // Copy resized image onto canvas
        for y in 0..scaled_h {
            for x in 0..scaled_w {
                let pixel = resized.get_pixel(x, y);
                canvas.put_pixel(x + offset_x, y + offset_y, *pixel);
            }
        }
        
        // 3. Preprocessing: Convert to NCHW float [0, 1]
        // Model expects [1, 3, 144, 256] - NCHW format (HuggingFace ONNX model)
        let plane_size = (MODEL_HEIGHT * MODEL_WIDTH) as usize;
        let mut input_data = vec![0.0f32; 1 * 3 * plane_size];
        
        let samples = canvas.as_raw();
        
        // NCHW: channels are planar [R plane, G plane, B plane]
        for i in 0..plane_size {
            let r = samples[i * 3] as f32;
            let g = samples[i * 3 + 1] as f32;
            let b = samples[i * 3 + 2] as f32;
            
            // Normalize to [0, 1] and store in NCHW layout
            input_data[i] = r / 255.0;                    // R plane
            input_data[plane_size + i] = g / 255.0;       // G plane
            input_data[2 * plane_size + i] = b / 255.0;   // B plane
        }

        // 4. Run inference - NCHW format [1, C, H, W]
        let input_value = Value::from_array(([1, 3, MODEL_HEIGHT as i64, MODEL_WIDTH as i64], input_data))?;
        let inputs = ort::inputs!["pixel_values" => &input_value];
        let outputs = self.session.run(inputs)?;
        
        // Output is "alphas" - already person mask (not background)
        let (_, data) = outputs["alphas"].try_extract_tensor::<f32>()?;
        
        // 5. Post-process: Extract mask, un-letterbox, resize to original resolution
        // Output is already person mask (no inversion needed)
        let mask_bytes: Vec<u8> = data.iter()
            .take(plane_size)
            .map(|&v| (v.clamp(0.0, 1.0) * 255.0) as u8)  // Already person mask
            .collect();
        
        let mask_img = GrayImage::from_raw(MODEL_WIDTH, MODEL_HEIGHT, mask_bytes)
            .ok_or_else(|| anyhow!("Failed to create mask image"))?;
        
        // Crop out the letterboxed region (the valid mask area)
        let cropped = image::imageops::crop_imm(
            &mask_img,
            offset_x,
            offset_y,
            scaled_w,
            scaled_h,
        ).to_image();
        
        // Resize back to original frame resolution (use Gaussian for smooth alpha mask)
        let final_mask = image::imageops::resize(&cropped, orig_w, orig_h, FilterType::Gaussian);
        
        Ok((final_mask.into_raw(), orig_w, orig_h))
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
