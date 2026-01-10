//! wgpu-based GPU shader pipeline.

use super::{ShaderPipeline, ShaderSource};
use crate::frame::{PixelFormat, QuadVertex, VideoFrame};
use anyhow::{anyhow, Result};
use naga::front::glsl::{Frontend, Options};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::ShaderStage;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// Default vertex shader in WGSL.
const VERTEX_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    return out;
}
"#;

/// Default passthrough fragment shader in WGSL.
const DEFAULT_FRAGMENT_SHADER: &str = r#"
@group(0) @binding(0) var t_texture: texture_2d<f32>;
@group(0) @binding(1) var s_sampler: sampler;

@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(t_texture, s_sampler, tex_coords);
}
"#;

/// Uniforms passed to the shader.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub time: f32,
    pub width: f32,
    pub height: f32,
    pub _padding: u32, // Padding for 16-byte alignment
}


/// GPU shader pipeline using wgpu.
pub struct WgpuPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    render_pipelines: Vec<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    output_width: u32,
    output_height: u32,
}

impl WgpuPipeline {
    /// Creates a new wgpu pipeline with the given shaders.
    pub fn new(width: u32, height: u32, shaders: Vec<ShaderSource>) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| anyhow!("Failed to find GPU adapter"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Proteus Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))?;

        // Prepare shader sources
        // If no shaders provided, use default passthrough
        let shader_sources = if shaders.is_empty() {
            vec![(DEFAULT_FRAGMENT_SHADER.to_string(), "fs_main")]
        } else {
            let mut sources = Vec::new();
            for shader in shaders {
                let (fragment_wgsl, fragment_entry_point) = match shader {
                    ShaderSource::Glsl(glsl) => (Self::glsl_to_wgsl(&glsl)?, "main"),
                    ShaderSource::Wgsl(wgsl) => (wgsl, "fs_main"),
                };
                sources.push((fragment_wgsl, fragment_entry_point));
            }
            sources
        };

        // Create shader modules
        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(VERTEX_SHADER)),
        });

        // Create bind group layout (shared for all pipelines if they use the same layout)
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let mut render_pipelines = Vec::new();

        for (i, (fragment_wgsl, fragment_entry_point)) in shader_sources.into_iter().enumerate() {
            let fragment_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("Fragment Shader {}", i)),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(fragment_wgsl)),
            });

            let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("Render Pipeline {}", i)),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vertex_module,
                    entry_point: Some("vs_main"),
                    buffers: &[QuadVertex::layout()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fragment_module,
                    entry_point: Some(fragment_entry_point),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
            render_pipelines.push(render_pipeline);
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(QuadVertex::VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(QuadVertex::INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Texture Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create uniform buffer
        let uniforms = Uniforms {
            time: 0.0,
            width: width as f32,
            height: height as f32,
            _padding: 0,
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Ok(Self {
            device,
            queue,
            render_pipelines,
            vertex_buffer,
            index_buffer,
            bind_group_layout,
            uniform_buffer,
            sampler,
            output_width: width,
            output_height: height,
        })
    }


    /// Converts GLSL fragment shader to WGSL.
    fn glsl_to_wgsl(glsl: &str) -> Result<String> {
        let mut frontend = Frontend::default();
        let options = Options::from(ShaderStage::Fragment);
        
        let module = frontend
            .parse(&options, glsl)
            .map_err(|e| anyhow!("GLSL parse error: {:?}", e))?;

        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        let info = validator
            .validate(&module)
            .map_err(|e| anyhow!("Shader validation error: {:?}", e))?;

        let wgsl = naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .map_err(|e| anyhow!("WGSL generation error: {:?}", e))?;

        Ok(wgsl)
    }

    /// Returns the device and queue for external use (e.g., window output).
    pub fn device_and_queue(&self) -> (&wgpu::Device, &wgpu::Queue) {
        (&self.device, &self.queue)
    }

    /// Returns the render pipelines for external use.
    pub fn render_pipelines(&self) -> &[wgpu::RenderPipeline] {
        &self.render_pipelines
    }

    /// Returns the bind group layout.
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Returns the vertex and index buffers.
    pub fn buffers(&self) -> (&wgpu::Buffer, &wgpu::Buffer) {
        (&self.vertex_buffer, &self.index_buffer)
    }

    /// Returns the sampler.
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }
}

impl ShaderPipeline for WgpuPipeline {
    fn process_frame(&mut self, input: &VideoFrame, time: f32) -> Result<VideoFrame> {
        // Update uniform buffer
        let uniforms = Uniforms {
            time,
            width: self.output_width as f32,
            height: self.output_height as f32,
            _padding: 0,
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Convert to RGBA if needed
        let rgba_input = input.to_rgba();

        // Create input texture from the video frame
        let mut frames = Vec::new();
        
        let initial_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Initial Input Texture"),
            size: wgpu::Extent3d {
                width: rgba_input.width,
                height: rgba_input.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &initial_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba_input.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(rgba_input.width * 4),
                rows_per_image: Some(rgba_input.height),
            },
            wgpu::Extent3d {
                width: rgba_input.width,
                height: rgba_input.height,
                depth_or_array_layers: 1,
            },
        );
        
        frames.push(initial_texture);

        // Process through all pipelines
        // we need N render passes
        // Pass 0: Initial Texture -> Texture 1
        // Pass 1: Texture 1 -> Texture 2
        // ...
        
        for i in 0..self.render_pipelines.len() {
             let output_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("Output Texture {}", i)),
                size: wgpu::Extent3d {
                    width: self.output_width,
                    height: self.output_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            frames.push(output_texture);
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        for (i, pipeline) in self.render_pipelines.iter().enumerate() {
            let input_view = frames[i].create_view(&wgpu::TextureViewDescriptor::default());
            let output_view = frames[i+1].create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Bind Group {}", i)),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                ],
            });

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&format!("Render Pass {}", i)),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_pipeline(pipeline);
                render_pass.set_bind_group(0, &bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, 0..1);
            }
        }

        // Copy final output to buffer
        let final_texture = frames.last().unwrap();
        let output_buffer_size =
            (self.output_width * self.output_height * 4) as wgpu::BufferAddress;
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: final_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.output_width * 4),
                    rows_per_image: Some(self.output_height),
                },
            },
            wgpu::Extent3d {
                width: self.output_width,
                height: self.output_height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        // Read back the output
        let buffer_slice = output_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        receiver.recv()??;

        let data = buffer_slice.get_mapped_range();
        let output_data = data.to_vec();
        drop(data);
        output_buffer.unmap();

        Ok(VideoFrame::from_data(
            self.output_width,
            self.output_height,
            PixelFormat::Rgba,
            output_data,
        ))
    }
}
