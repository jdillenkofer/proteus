//! Video frame types and pixel format conversions.

use bytemuck::{Pod, Zeroable};

/// Supported pixel formats for video frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// RGB with 8 bits per channel (24 bits per pixel)
    Rgb,
    /// RGBA with 8 bits per channel (32 bits per pixel)
    Rgba,
    /// YUV 4:2:2 packed format (Y0 U0 Y1 V0)
    Yuyv,
    /// YUV 4:2:2 packed format (U0 Y0 V0 Y1) - used by macOS
    Uyvy,
    /// NV12 semi-planar format (Y plane + interleaved UV)
    Nv12,
}

impl PixelFormat {
    /// Returns the number of bytes per pixel for packed formats.
    /// For planar formats like NV12, this returns the bytes for the Y component only.
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Rgb => 3,
            PixelFormat::Rgba => 4,
            PixelFormat::Yuyv => 2,
            PixelFormat::Uyvy => 2,
            PixelFormat::Nv12 => 1, // Y plane only
        }
    }
}

/// A video frame containing image data.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Pixel format of the frame data
    pub format: PixelFormat,
    /// Timestamp in microseconds (if available)
    pub timestamp_us: Option<u64>,
    /// Raw pixel data
    pub data: Vec<u8>,
}

impl VideoFrame {
    /// Creates a new video frame with the given dimensions and format.
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        let size = (width as usize) * (height as usize) * format.bytes_per_pixel();
        Self {
            width,
            height,
            format,
            timestamp_us: None,
            data: vec![0; size],
        }
    }

    /// Creates a video frame from existing data.
    pub fn from_data(width: u32, height: u32, format: PixelFormat, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format,
            timestamp_us: None,
            data,
        }
    }

    /// Scale this frame down if either dimension exceeds `max_dimension`.
    /// Preserves aspect ratio. Returns self unchanged if within limits.
    /// Always converts to RGBA format.
    pub fn scale_to_fit(&self, max_dimension: u32) -> VideoFrame {
        let max_dim = self.width.max(self.height);
        if max_dim <= max_dimension {
            let conv_start = std::time::Instant::now();
            let result = self.to_rgba();
            tracing::debug!("    [Perf] scale_to_fit (no resize) to_rgba: {:?}", conv_start.elapsed());
            return result;
        }

        // Calculate new dimensions preserving aspect ratio
        let scale = max_dimension as f32 / max_dim as f32;
        let new_width = ((self.width as f32 * scale) as u32).max(1);
        let new_height = ((self.height as f32 * scale) as u32).max(1);

        // Convert to RGBA first
        let conv_start = std::time::Instant::now();
        let rgba = self.to_rgba();
        let conv_elapsed = conv_start.elapsed();

        // Use image crate to resize
        let resize_start = std::time::Instant::now();
        let img = image::RgbaImage::from_raw(rgba.width, rgba.height, rgba.data)
            .expect("Failed to create image from frame data");
        let resized = image::imageops::resize(
            &img,
            new_width,
            new_height,
            image::imageops::FilterType::Nearest,
        );
        let resize_elapsed = resize_start.elapsed();

        tracing::debug!("    [Perf] scale_to_fit (with resize) to_rgba: {:?}, resize: {:?}", conv_elapsed, resize_elapsed);

        VideoFrame {
            width: new_width,
            height: new_height,
            format: PixelFormat::Rgba,
            timestamp_us: self.timestamp_us,
            data: resized.into_raw(),
        }
    }

    /// Converts this frame to RGBA format.
    pub fn to_rgba(&self) -> VideoFrame {
        if self.format == PixelFormat::Rgba {
            return self.clone();
        }

        let width = self.width as usize;
        let height = self.height as usize;
        let pixel_count = width * height;
        let mut rgba_data = vec![0u8; pixel_count * 4];

        // Fast path for RGB -> RGBA: just add alpha=255, no color conversion needed
        if self.format == PixelFormat::Rgb {
            for i in 0..pixel_count {
                rgba_data[i * 4] = self.data[i * 3];
                rgba_data[i * 4 + 1] = self.data[i * 3 + 1];
                rgba_data[i * 4 + 2] = self.data[i * 3 + 2];
                rgba_data[i * 4 + 3] = 255;
            }
            return VideoFrame {
                width: self.width,
                height: self.height,
                format: PixelFormat::Rgba,
                timestamp_us: self.timestamp_us,
                data: rgba_data,
            };
        }

        // Use ezk_image for YUV format conversions
        {
            let dst_color = ezk_image::ColorInfo::RGB(ezk_image::RgbColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
            });
            let mut dst_image = ezk_image::Image::from_buffer(
                ezk_image::PixelFormat::RGBA,
                &mut rgba_data[..],
                None,
                width,
                height,
                dst_color,
            ).expect("Failed to wrap RGBA dst buffer");

            let src_color_yuv = ezk_image::ColorInfo::YUV(ezk_image::YuvColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
                space: ezk_image::ColorSpace::BT709,
                full_range: false,
            });

            match self.format {
                PixelFormat::Yuyv => {
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::YUYV,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_yuv,
                    ).expect("Failed to wrap YUYV buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Nv12 => {
                     let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::NV12,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_yuv,
                    ).expect("Failed to wrap Nv12 buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Uyvy => {
                    // UYVY fallback
                    let count = self.data.len();
                    let mut yuyv_temp = vec![0u8; count];
                    for i in (0..count).step_by(2) {
                        if i + 1 < count {
                            yuyv_temp[i] = self.data[i+1];
                            yuyv_temp[i+1] = self.data[i];
                        }
                    }
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::YUYV,
                        &yuyv_temp[..],
                        None,
                        width,
                        height,
                        src_color_yuv,
                    ).expect("Failed to wrap UYVY buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Rgb | PixelFormat::Rgba => unreachable!(),
            }
        }

        VideoFrame {
            width: self.width,
            height: self.height,
            format: PixelFormat::Rgba,
            timestamp_us: self.timestamp_us,
            data: rgba_data,
        }
    }

    /// Converts this frame to NV12 format using ezk-image.
    pub fn to_nv12(&self) -> VideoFrame {
        if self.format == PixelFormat::Nv12 {
            return self.clone();
        }

        let width = self.width as usize;
        let height = self.height as usize;

        // NV12 size: Y plane + UV plane
        let y_size = width * height;
        let uv_stride = width + (width % 2);
        let uv_height = (height + 1) / 2;
        let uv_size = uv_stride * uv_height;
        let mut nv12_data = vec![0u8; y_size + uv_size];

        {
            // Destination Image (NV12)
            // Color can be standard Rec.709
            let dst_color = ezk_image::ColorInfo::YUV(ezk_image::YuvColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
                space: ezk_image::ColorSpace::BT709,
                full_range: false,
            });

            // Create destination image wrapper around mutable buffer
            let mut dst_image = ezk_image::Image::from_buffer(
                ezk_image::PixelFormat::NV12,
                &mut nv12_data[..],
                None,
                width,
                height,
                dst_color,
            ).expect("Failed to wrap NV12 buffer");

            let src_color_rgb = ezk_image::ColorInfo::RGB(ezk_image::RgbColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
            });
            let src_color_yuv = ezk_image::ColorInfo::YUV(ezk_image::YuvColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
                space: ezk_image::ColorSpace::BT709,
                full_range: false,
            });

            match self.format {
                PixelFormat::Rgba => {
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::RGBA,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_rgb,
                    ).expect("Failed to wrap RGBA buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Rgb => {
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::RGB,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_rgb,
                    ).expect("Failed to wrap RGB buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Yuyv => {
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::YUYV,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_yuv,
                    ).expect("Failed to wrap YUYV buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Uyvy => {
                    // UYVY not supported directly by ezk_image. Convert UYVY -> YUYV first.
                    // U0 Y0 V0 Y1 -> Y0 U0 Y1 V0
                    let count = self.data.len();
                    let mut yuyv_temp = vec![0u8; count];
                    for i in (0..count).step_by(2) {
                        if i + 1 < count {
                            yuyv_temp[i] = self.data[i+1];
                            yuyv_temp[i+1] = self.data[i];
                        }
                    }
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::YUYV,
                        &yuyv_temp[..],
                        None,
                        width,
                        height,
                        src_color_yuv,
                    ).expect("Failed to wrap UYVY(as YUYV) buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Nv12 => unreachable!(),
            }
        }

        VideoFrame {
            width: self.width,
            height: self.height,
            format: PixelFormat::Nv12,
            timestamp_us: self.timestamp_us,
            data: nv12_data,
        }
    }

    /// Converts this frame to YUYV format (YUV 4:2:2 packed).
    /// YUYV is Y0 U0 Y1 V0 (2 pixels packed in 4 bytes).
    pub fn to_yuyv(&self) -> VideoFrame {
        if self.format == PixelFormat::Yuyv {
            return self.clone();
        }

        let width = self.width as usize;
        let height = self.height as usize;
        let pixel_count = width * height;
        let mut yuyv_data = vec![0u8; pixel_count * 2];

        if self.format == PixelFormat::Uyvy {
            // Fast path: UYVY -> YUYV is just byte swap
            // U0 Y0 V0 Y1 -> Y0 U0 Y1 V0
            for i in (0..yuyv_data.len()).step_by(2) {
                if i + 1 < self.data.len() {
                    yuyv_data[i] = self.data[i+1];
                    yuyv_data[i+1] = self.data[i];
                }
            }
        } else {
            // Use ezk-image for other conversions
             let dst_color = ezk_image::ColorInfo::YUV(ezk_image::YuvColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
                space: ezk_image::ColorSpace::BT709,
                full_range: false,
            });
            let mut dst_image = ezk_image::Image::from_buffer(
                ezk_image::PixelFormat::YUYV,
                &mut yuyv_data[..],
                None,
                width,
                height,
                dst_color,
            ).expect("Failed to wrap YUYV dst buffer");

             let src_color_rgb = ezk_image::ColorInfo::RGB(ezk_image::RgbColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
            });
             let src_color_yuv = ezk_image::ColorInfo::YUV(ezk_image::YuvColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
                space: ezk_image::ColorSpace::BT709,
                full_range: false,
            });

            match self.format {
                PixelFormat::Rgba => {
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::RGBA,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_rgb,
                    ).expect("Failed to wrap RGBA buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Rgb => {
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::RGB,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_rgb,
                    ).expect("Failed to wrap RGB buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Nv12 => {
                     let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::NV12,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_yuv,
                    ).expect("Failed to wrap Nv12 buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Yuyv => unreachable!(),
                PixelFormat::Uyvy => unreachable!(), // Handled above
            }
        }

        VideoFrame {
            width: self.width,
            height: self.height,
            format: PixelFormat::Yuyv,
            timestamp_us: self.timestamp_us,
            data: yuyv_data,
        }
    }

    /// Converts this frame to UYVY format (YUV 4:2:2 packed).
    /// UYVY is U0 Y0 V0 Y1 (2 pixels packed in 4 bytes).
    /// This format is used by macOS virtual camera (kCVPixelFormatType_422YpCbCr8).
    pub fn to_uyvy(&self) -> VideoFrame {
        if self.format == PixelFormat::Uyvy {
            return self.clone();
        }

        let width = self.width as usize;
        let height = self.height as usize;
        let pixel_count = width * height;
        let mut uyvy_data = vec![0u8; pixel_count * 2];
        
        // Strategy: Convert to YUYV into uyvy_data, then in-place swap
        {
             let dst_color = ezk_image::ColorInfo::YUV(ezk_image::YuvColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
                space: ezk_image::ColorSpace::BT709,
                full_range: false,
            });
            // Treat destination as YUYV for conversion
            let mut dst_image = ezk_image::Image::from_buffer(
                ezk_image::PixelFormat::YUYV,
                &mut uyvy_data[..],
                None,
                width,
                height,
                dst_color,
            ).expect("Failed to wrap dst buffer for UYVY conversion");

             let src_color_rgb = ezk_image::ColorInfo::RGB(ezk_image::RgbColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
            });
             let src_color_yuv = ezk_image::ColorInfo::YUV(ezk_image::YuvColorInfo {
                transfer: ezk_image::ColorTransfer::Linear,
                primaries: ezk_image::ColorPrimaries::BT709,
                space: ezk_image::ColorSpace::BT709,
                full_range: false,
            });

            match self.format {
                PixelFormat::Rgba => {
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::RGBA,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_rgb,
                    ).expect("Failed to wrap RGBA buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Rgb => {
                    let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::RGB,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_rgb,
                    ).expect("Failed to wrap RGB buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Nv12 => {
                     let src_image = ezk_image::Image::from_buffer(
                        ezk_image::PixelFormat::NV12,
                        &self.data[..],
                        None,
                        width,
                        height,
                        src_color_yuv,
                    ).expect("Failed to wrap Nv12 buffer");
                    ezk_image::convert(&src_image, &mut dst_image).expect("Conversion failed");
                },
                PixelFormat::Yuyv => {
                    // Copy YUYV directly
                     uyvy_data.copy_from_slice(&self.data);
                },
                PixelFormat::Uyvy => unreachable!(),
            }
        }
        
        // In-place swap: YUYV -> UYVY
        // Y0 U0 Y1 V0 -> U0 Y0 V0 Y1
        for i in (0..uyvy_data.len()).step_by(2) {
             let b1 = uyvy_data[i];
             let b2 = uyvy_data[i+1];
             uyvy_data[i] = b2;
             uyvy_data[i+1] = b1;
        }

        VideoFrame {
            width: self.width,
            height: self.height,
            format: PixelFormat::Uyvy,
            timestamp_us: self.timestamp_us,
            data: uyvy_data,
        }
    }
}

/// Vertex for rendering a full-screen quad.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct QuadVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
}

impl QuadVertex {
    /// Vertices for a full-screen quad.
    pub const VERTICES: &'static [QuadVertex] = &[
        QuadVertex { position: [-1.0, -1.0], tex_coords: [0.0, 1.0] },
        QuadVertex { position: [1.0, -1.0], tex_coords: [1.0, 1.0] },
        QuadVertex { position: [1.0, 1.0], tex_coords: [1.0, 0.0] },
        QuadVertex { position: [-1.0, 1.0], tex_coords: [0.0, 0.0] },
    ];

    /// Indices for the quad (two triangles).
    pub const INDICES: &'static [u16] = &[0, 1, 2, 2, 3, 0];

    /// Returns the vertex buffer layout.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_rgba_conversion() {
        let rgb_data = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let frame = VideoFrame::from_data(2, 2, PixelFormat::Rgb, rgb_data);
        let rgba_frame = frame.to_rgba();

        assert_eq!(rgba_frame.format, PixelFormat::Rgba);
        assert_eq!(rgba_frame.data.len(), 16);
        // Check first pixel (red)
        assert_eq!(&rgba_frame.data[0..4], &[255, 0, 0, 255]);
        // Check second pixel (green)
        assert_eq!(&rgba_frame.data[4..8], &[0, 255, 0, 255]);
    }
}
