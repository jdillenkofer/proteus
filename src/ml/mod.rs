use anyhow::{anyhow, Result};
use image::{imageops::FilterType, DynamicImage, ImageBuffer, Luma, Rgba};
use ndarray::{Array4, Axis};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;
use std::path::Path;
use tracing::{info, warn};

use crate::frame::VideoFrame;

/// MODNet input size (usually 512x512 for balance).
const MODEL_WIDTH: u32 = 512;
const MODEL_HEIGHT: u32 = 512;

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
        
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path)?;

        Ok(Some(Self { session }))
    }

    /// Run inference on a video frame and return the alpha mask (grayscale 8-bit).
    /// Resizes the output mask to `target_width` x `target_height`.
    pub fn predict(&mut self, frame: &VideoFrame, target_width: u32, target_height: u32) -> Result<Vec<u8>> {
        // 1. Convert VideoFrame to DynamicImage
        let img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(frame.width, frame.height, frame.data.clone())
            .ok_or_else(|| anyhow!("Failed to create image buffer"))?;
        let img = DynamicImage::ImageRgba8(img_buffer);

        // 2. Resize to model input size (Letterbox)
        // We want to scale the image so it fits within 512x512 while maintaining aspect ratio,
        // then pad the rest with black.
        
        let ratio = (MODEL_WIDTH as f32 / frame.width as f32).min(MODEL_HEIGHT as f32 / frame.height as f32);
        let new_width = (frame.width as f32 * ratio).round() as u32;
        let new_height = (frame.height as f32 * ratio).round() as u32;
        
        let resized = img.resize_exact(new_width, new_height, FilterType::Triangle);
        let mut padded = ImageBuffer::from_pixel(MODEL_WIDTH, MODEL_HEIGHT, Rgba([0, 0, 0, 0]));
        
        // Center the resized image
        let x_offset = (MODEL_WIDTH - new_width) / 2;
        let y_offset = (MODEL_HEIGHT - new_height) / 2;
        
        image::imageops::overlay(&mut padded, &resized, x_offset as i64, y_offset as i64);
        
        // 3. Normalize and prepare tensor
        let mut input_tensor = Array4::<f32>::zeros((1, 3, MODEL_HEIGHT as usize, MODEL_WIDTH as usize));
        
        for (x, y, pixel) in padded.enumerate_pixels() {
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;
            
            input_tensor[[0, 0, y as usize, x as usize]] = (r - 0.5) / 0.5;
            input_tensor[[0, 1, y as usize, x as usize]] = (g - 0.5) / 0.5;
            input_tensor[[0, 2, y as usize, x as usize]] = (b - 0.5) / 0.5;
        }

        // 4. Run inference
        let shape = input_tensor.shape().iter().map(|&x| x as i64).collect::<Vec<_>>();
        let data = input_tensor.into_raw_vec();
        let input_value = Value::from_array((shape, data))?;
        let inputs = ort::inputs!["input" => &input_value];
        let outputs = self.session.run(inputs)?;
        
        // Output is usually [1, 1, 512, 512] (matte)
        let (shape, data) = outputs["output"].try_extract_tensor::<f32>()?;
        let output_tensor = Array4::from_shape_vec(
            (shape[0] as usize, shape[1] as usize, shape[2] as usize, shape[3] as usize),
            data.to_vec()
        )?;
        
        // 5. Post-process
        // Extract 2D mask, resize back to original size
        // We'll construct a grayscale image from the output
        
        let binding = output_tensor.index_axis(Axis(0), 0);
        let output_data = binding.index_axis(Axis(0), 0);
        let mut mask_img = ImageBuffer::<Luma<u8>, Vec<u8>>::new(MODEL_WIDTH, MODEL_HEIGHT); // Luma8
        
        // Manual mapping from f32 to u8
        // output likely 0.0 to 1.0
        for (y, row) in output_data.outer_iter().enumerate() {
            for (x, val) in row.iter().enumerate() {
                let val: f32 = *val;
                let p = (val.clamp(0.0, 1.0) * 255.0) as u8;
                // mask_img.put_pixel(x as u32, y as u32, image::Luma([p]));
                // image::Luma is wrapping a single value
                 let pixel = image::Luma([p]);
                 mask_img.put_pixel(x as u32, y as u32, pixel);
            }
        }
        
        let mask_dynamic = DynamicImage::ImageLuma8(mask_img);
        
        // Crop back to valid region
        let mask_cropped = mask_dynamic.crop_imm(x_offset, y_offset, new_width, new_height);
        
        let mask_resized = mask_cropped.resize_exact(target_width, target_height, FilterType::Triangle);
        
        Ok(mask_resized.to_luma8().into_raw())
    }
}
