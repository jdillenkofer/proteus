//! Video frame types and pixel format conversions.

use bytemuck::{Pod, Zeroable};

/// Supported pixel formats for video frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// RGB with 8 bits per channel (24 bits per pixel)
    Rgb,
    /// RGBA with 8 bits per channel (32 bits per pixel)
    Rgba,
    /// YUV 4:2:2 packed format
    Yuyv,
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

    /// Converts this frame to RGBA format.
    pub fn to_rgba(&self) -> VideoFrame {
        if self.format == PixelFormat::Rgba {
            return self.clone();
        }

        let pixel_count = (self.width as usize) * (self.height as usize);
        let mut rgba_data = vec![0u8; pixel_count * 4];

        match self.format {
            PixelFormat::Rgb => {
                for i in 0..pixel_count {
                    rgba_data[i * 4] = self.data[i * 3];
                    rgba_data[i * 4 + 1] = self.data[i * 3 + 1];
                    rgba_data[i * 4 + 2] = self.data[i * 3 + 2];
                    rgba_data[i * 4 + 3] = 255;
                }
            }
            PixelFormat::Yuyv => {
                // YUYV: Y0 U0 Y1 V0 (2 pixels packed in 4 bytes)
                for i in 0..(pixel_count / 2) {
                    let y0 = self.data[i * 4] as f32;
                    let u = self.data[i * 4 + 1] as f32 - 128.0;
                    let y1 = self.data[i * 4 + 2] as f32;
                    let v = self.data[i * 4 + 3] as f32 - 128.0;

                    // First pixel
                    let idx = i * 2 * 4;
                    rgba_data[idx] = (y0 + 1.402 * v).clamp(0.0, 255.0) as u8;
                    rgba_data[idx + 1] = (y0 - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
                    rgba_data[idx + 2] = (y0 + 1.772 * u).clamp(0.0, 255.0) as u8;
                    rgba_data[idx + 3] = 255;

                    // Second pixel
                    let idx = (i * 2 + 1) * 4;
                    rgba_data[idx] = (y1 + 1.402 * v).clamp(0.0, 255.0) as u8;
                    rgba_data[idx + 1] = (y1 - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
                    rgba_data[idx + 2] = (y1 + 1.772 * u).clamp(0.0, 255.0) as u8;
                    rgba_data[idx + 3] = 255;
                }
            }
            PixelFormat::Nv12 => {
                // NV12: Y plane followed by interleaved UV plane
                let y_plane = &self.data[..pixel_count];
                let uv_plane = &self.data[pixel_count..];

                for y in 0..self.height as usize {
                    for x in 0..self.width as usize {
                        let y_val = y_plane[y * self.width as usize + x] as f32;
                        let uv_idx = (y / 2) * self.width as usize + (x / 2) * 2;
                        let u = uv_plane[uv_idx] as f32 - 128.0;
                        let v = uv_plane[uv_idx + 1] as f32 - 128.0;

                        let idx = (y * self.width as usize + x) * 4;
                        rgba_data[idx] = (y_val + 1.402 * v).clamp(0.0, 255.0) as u8;
                        rgba_data[idx + 1] = (y_val - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
                        rgba_data[idx + 2] = (y_val + 1.772 * u).clamp(0.0, 255.0) as u8;
                        rgba_data[idx + 3] = 255;
                    }
                }
            }
            PixelFormat::Rgba => unreachable!(),
        }

        VideoFrame {
            width: self.width,
            height: self.height,
            format: PixelFormat::Rgba,
            timestamp_us: self.timestamp_us,
            data: rgba_data,
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
