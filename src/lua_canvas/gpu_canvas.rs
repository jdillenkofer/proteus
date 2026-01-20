//! GPU-based 2D canvas using wgpu with batched rendering.
//!
//! Provides primitives for 2D rendering: rectangles, circles, lines.
//! Uses SDF-based fragment shaders for anti-aliased rendering.
//! All draw calls are batched and submitted in a single command buffer.

use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Maximum number of primitives that can be batched in a single frame.
pub const MAX_PRIMITIVES: usize = 16384;

/// A GPU-based 2D canvas that renders to an RGBA texture.
pub struct GpuCanvas {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pub width: u32,
    pub height: u32,
    // Render target texture
    texture: wgpu::Texture,
    // Cached views
    texture_view: wgpu::TextureView,
    srgb_view: wgpu::TextureView,
    // Stencil texture for clipping
    stencil_view: wgpu::TextureView,
    // Pipelines for different primitives
    rect_fill_pipeline: wgpu::RenderPipeline,
    rect_fill_clipped_pipeline: wgpu::RenderPipeline,
    circle_fill_pipeline: wgpu::RenderPipeline,
    circle_fill_clipped_pipeline: wgpu::RenderPipeline,
    circle_stroke_pipeline: wgpu::RenderPipeline,
    circle_stroke_clipped_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    line_pipeline_clipped: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    glyph_pipeline_clipped: wgpu::RenderPipeline,
    stencil_write_pipeline: wgpu::RenderPipeline,
    // Vertex buffer for full-screen quad
    quad_vertex_buffer: wgpu::Buffer,
    // Uniform bind group layout
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    // Glyph atlas resources
    glyph_atlas_texture: wgpu::Texture,
    glyph_bind_group: wgpu::BindGroup,
    // Staging buffer for CPU readback
    staging_buffer: wgpu::Buffer,
    // Current clip state
    clip_active: bool,
    // Batched draw commands
    pending_commands: Vec<DrawCommand>,
    // Pending clear color (if any)
    pending_clear: Option<wgpu::Color>,
    // Pre-allocated uniform buffer for batching
    uniform_buffer: wgpu::Buffer,
}

/// Types of draw commands
#[derive(Clone, Copy)]
pub enum DrawCommandType {
    FillRect,
    FillCircle,
    StrokeCircle,
    Line,
    Glyph,
    PushClip,
    PopClip,
}

/// A batched draw command
pub struct DrawCommand {
    pub cmd_type: DrawCommandType,
    pub uniforms: [f32; 16], // 4x vec4 = 16 floats
    pub clip_active: bool,
}

/// Uniform data passed to shaders (64 bytes = 16 floats)
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PrimitiveUniforms {
    // Rect/Circle/Glyph: target x, y, w, h
    bounds: [f32; 4],
    // RGBA color (0-1 range) OR Glyph: atlas u, v, w, h
    color: [f32; 4],
    // Extra params: stroke_width, canvas_width, canvas_height, 0 OR Glyph: color RGBA
    extra: [f32; 4],
    // Extended params for Glyph: atlas_w, atlas_h, 0, 0
    extra2: [f32; 4],
}

// Full-screen quad vertices (two triangles)
const QUAD_VERTICES: &[[f32; 2]; 6] = &[
    [-1.0, -1.0],
    [1.0, -1.0],
    [1.0, 1.0],
    [-1.0, -1.0],
    [1.0, 1.0],
    [-1.0, 1.0],
];

impl GpuCanvas {
    /// Create a new GPU canvas with the given dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        // Create wgpu instance and adapter
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("Failed to find GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("LuaCanvas Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::default(),
            },
        ))
        .expect("Failed to create device");

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        Self::with_device_queue(device, queue, width, height)
    }

    /// Create a GPU canvas using an existing device and queue.
    pub fn with_device_queue(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
    ) -> Self {
        // Create render target texture
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Canvas Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create stencil texture for clipping
        let stencil_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Stencil Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Stencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let stencil_view = stencil_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create staging buffer for CPU readback
        // Align to 256 bytes for COPY_BYTES_PER_ROW_ALIGNMENT
        let aligned_bytes_per_row = (width * 4 + 255) & !255;
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size: (aligned_bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Create vertex buffer for full-screen quad
        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Create bind group layout for uniforms
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<PrimitiveUniforms>() as u64),
                    },
                    count: None,
                }],
            });

        // Pre-allocate uniform buffer for all primitives
        let uniform_buffer_size = (MAX_PRIMITIVES * 256) as u64; // 256 byte alignment per uniform
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: uniform_buffer_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let srgb_view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });

        // Glyph Atlas setup (2048x2048 Alpha8 for now)
        let atlas_w = 2048;
        let atlas_h = 2048;
        let glyph_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Atlas"),
            size: wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let glyph_atlas_view = glyph_atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let glyph_atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let glyph_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glyph Bind Group Layout"),
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
            ],
        });

        let glyph_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glyph Bind Group"),
            layout: &glyph_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&glyph_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&glyph_atlas_sampler),
                },
            ],
        });

        // Create shaders and pipelines
        let rect_fill_pipeline = Self::create_rect_fill_pipeline(&device, &uniform_bind_group_layout, false);
        let rect_fill_clipped_pipeline = Self::create_rect_fill_pipeline(&device, &uniform_bind_group_layout, true);
        let circle_fill_pipeline = Self::create_circle_fill_pipeline(&device, &uniform_bind_group_layout, false);
        let circle_fill_clipped_pipeline = Self::create_circle_fill_pipeline(&device, &uniform_bind_group_layout, true);
        let circle_stroke_pipeline = Self::create_circle_stroke_pipeline(&device, &uniform_bind_group_layout, false);
        let circle_stroke_clipped_pipeline = Self::create_circle_stroke_pipeline(&device, &uniform_bind_group_layout, true);
        let line_pipeline = Self::create_line_pipeline(&device, &uniform_bind_group_layout, false);
        let line_pipeline_clipped = Self::create_line_pipeline(&device, &uniform_bind_group_layout, true);
        let glyph_pipeline = Self::create_glyph_pipeline(&device, &uniform_bind_group_layout, &glyph_bind_group_layout, false);
        let glyph_pipeline_clipped = Self::create_glyph_pipeline(&device, &uniform_bind_group_layout, &glyph_bind_group_layout, true);
        let stencil_write_pipeline = Self::create_stencil_write_pipeline(&device, &uniform_bind_group_layout);

        Self {
            device,
            queue,
            width,
            height,
            texture,
            texture_view,
            srgb_view,
            stencil_view,
            rect_fill_pipeline,
            rect_fill_clipped_pipeline,
            circle_fill_pipeline,
            circle_fill_clipped_pipeline,
            circle_stroke_pipeline,
            circle_stroke_clipped_pipeline,
            line_pipeline,
            line_pipeline_clipped,
            glyph_pipeline,
            glyph_pipeline_clipped,
            stencil_write_pipeline,
            quad_vertex_buffer,
            uniform_bind_group_layout,
            glyph_atlas_texture,
            glyph_bind_group,
            staging_buffer,
            clip_active: false,
            pending_commands: Vec::with_capacity(1024),
            pending_clear: None,
            uniform_buffer,
        }
    }

    fn create_rect_fill_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        stencil_test: bool,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rect Fill Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rect_fill.wgsl").into()),
        });

        Self::create_pipeline(device, bind_group_layout, &shader, "Rect Fill Pipeline", false, stencil_test)
    }

    fn create_circle_fill_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        stencil_test: bool,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Circle Fill Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/circle_fill.wgsl").into()),
        });

        Self::create_pipeline(device, bind_group_layout, &shader, "Circle Fill Pipeline", false, stencil_test)
    }

    fn create_circle_stroke_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        stencil_test: bool,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Circle Stroke Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/circle_stroke.wgsl").into()),
        });

        Self::create_pipeline(device, bind_group_layout, &shader, "Circle Stroke Pipeline", false, stencil_test)
    }

    fn create_line_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        stencil_test: bool,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Line Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/line.wgsl").into()),
        });

        Self::create_pipeline(device, bind_group_layout, &shader, "Line Pipeline", false, stencil_test)
    }

    fn create_glyph_pipeline(
        device: &wgpu::Device,
        uniform_layout: &wgpu::BindGroupLayout,
        glyph_layout: &wgpu::BindGroupLayout,
        stencil_test: bool,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glyph Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/glyph.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Glyph Pipeline Layout"),
            bind_group_layouts: &[uniform_layout, glyph_layout],
            immediate_size: 0,
        });

        let stencil_state = if stencil_test {
            wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            }
        } else {
            wgpu::StencilFaceState::IGNORE
        };

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Glyph Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Stencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState {
                    front: stencil_state,
                    back: stencil_state,
                    read_mask: 0xFF,
                    write_mask: 0xFF,
                },
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    fn create_stencil_write_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Stencil Write Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rect_fill.wgsl").into()),
        });

        Self::create_pipeline(device, bind_group_layout, &shader, "Stencil Write Pipeline", true, false)
    }

    fn create_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        shader: &wgpu::ShaderModule,
        label: &str,
        stencil_write: bool,
        stencil_test: bool,
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{} Layout", label)),
            bind_group_layouts: &[bind_group_layout],
            immediate_size: 0,
        });

        let stencil_state = if stencil_write {
            wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Always,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Replace,
            }
        } else if stencil_test {
            wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            }
        } else {
            wgpu::StencilFaceState::IGNORE
        };

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: if stencil_write {
                        wgpu::ColorWrites::empty()
                    } else {
                        wgpu::ColorWrites::ALL
                    },
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Stencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState {
                    front: stencil_state,
                    back: stencil_state,
                    read_mask: 0xFF,
                    write_mask: 0xFF,
                },
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Clear the canvas with the given RGBA color (batched).
    pub fn clear(&mut self, r: u8, g: u8, b: u8, a: u8) {
        // Clear pending commands since we're clearing the canvas
        self.pending_commands.clear();
        self.pending_clear = Some(wgpu::Color {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: a as f64 / 255.0,
        });
        self.clip_active = false;
    }


    /// Upload a single glyph to the atlas texture.
    pub fn upload_glyph_to_atlas(&self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.glyph_atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Queue a glyph draw command without an immediate flush.
    pub fn queue_glyph(&mut self, 
        target_x: f32, target_y: f32, target_w: f32, target_h: f32,
        atlas_x: f32, atlas_y: f32, atlas_w: f32, atlas_h: f32,
        r: u8, g: u8, b: u8, a: u8,
    ) {
        if self.pending_commands.len() >= MAX_PRIMITIVES {
            self.flush();
        }

        self.pending_commands.push(DrawCommand {
            cmd_type: DrawCommandType::Glyph,
            uniforms: [
                target_x, target_y, target_w, target_h,
                atlas_x, atlas_y, atlas_w, atlas_h,
                r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0,
                2048.0, 2048.0, self.width as f32, self.height as f32,
            ],
            clip_active: self.clip_active,
        });
    }




    /// Draw an image directly to the canvas texture (used for sprites).
    /// Note: Call flush() first if you want this to appear on top of previous draws.
    pub fn draw_image(&self, x: i32, y: i32, width: u32, height: u32, data: &[u8]) {
        if x < 0 || y < 0 || x + width as i32 > self.width as i32 || y + height as i32 > self.height as i32 {
            // Partial clipping or skip? For now, skip if out of bounds to keep it simple.
            if x >= self.width as i32 || y >= self.height as i32 || x + width as i32 <= 0 || y + height as i32 <= 0 {
                return;
            }
        }

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: x.max(0) as u32,
                    y: y.max(0) as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Flush all pending draw commands to the GPU.
    pub fn flush(&mut self) {
        if self.pending_commands.is_empty() && self.pending_clear.is_none() {
            return;
        }

        // Upload all uniforms to the buffer
        let uniform_stride = 256u32; // wgpu requires 256-byte alignment for dynamic offsets
        let mut uniform_data = vec![0u8; self.pending_commands.len() * uniform_stride as usize];
        
        for (i, cmd) in self.pending_commands.iter().enumerate() {
            let offset = i * uniform_stride as usize;
            let uniforms = PrimitiveUniforms {
                bounds: [cmd.uniforms[0], cmd.uniforms[1], cmd.uniforms[2], cmd.uniforms[3]],
                color: [cmd.uniforms[4], cmd.uniforms[5], cmd.uniforms[6], cmd.uniforms[7]],
                extra: [cmd.uniforms[8], cmd.uniforms[9], cmd.uniforms[10], cmd.uniforms[11]],
                extra2: [cmd.uniforms[12], cmd.uniforms[13], cmd.uniforms[14], cmd.uniforms[15]],
            };
            uniform_data[offset..offset + std::mem::size_of::<PrimitiveUniforms>()]
                .copy_from_slice(bytemuck::bytes_of(&uniforms));
        }

        if !uniform_data.is_empty() {
            self.queue.write_buffer(&self.uniform_buffer, 0, &uniform_data);
        }

        // Create bind group for the uniform buffer
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Batched Uniform Bind Group"),
            layout: &self.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &self.uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<PrimitiveUniforms>() as u64),
                }),
            }],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Batched Render Encoder"),
        });

        // Handle clear
        let has_clear = self.pending_clear.is_some();
        let clear_color = self.pending_clear.take().unwrap_or(wgpu::Color::TRANSPARENT);
        
        if has_clear {
            self.clip_active = false; // Clear resets clip state for a new frame
        }

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Batched Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if has_clear {
                            wgpu::LoadOp::Clear(clear_color)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.stencil_view,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: if has_clear {
                            wgpu::LoadOp::Clear(0)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                }),
                ..Default::default()
            });

            let mut last_pipeline: Option<*const wgpu::RenderPipeline> = None;
            let mut last_stencil_ref: Option<u32> = None;

            for (i, cmd) in self.pending_commands.iter().enumerate() {
                let dynamic_offset = (i as u32) * uniform_stride;

                let (pipeline, stencil_ref) = match cmd.cmd_type {
                    DrawCommandType::FillRect => (
                        if cmd.clip_active { &self.rect_fill_clipped_pipeline } else { &self.rect_fill_pipeline },
                        if cmd.clip_active { Some(1) } else { None }
                    ),
                    DrawCommandType::FillCircle => (
                        if cmd.clip_active { &self.circle_fill_clipped_pipeline } else { &self.circle_fill_pipeline },
                        if cmd.clip_active { Some(1) } else { None }
                    ),
                    DrawCommandType::StrokeCircle => (
                        if cmd.clip_active { &self.circle_stroke_clipped_pipeline } else { &self.circle_stroke_pipeline },
                        if cmd.clip_active { Some(1) } else { None }
                    ),
                    DrawCommandType::Line => (
                        if cmd.clip_active { &self.line_pipeline_clipped } else { &self.line_pipeline },
                        if cmd.clip_active { Some(1) } else { None }
                    ),
                    DrawCommandType::Glyph => (
                        if cmd.clip_active { &self.glyph_pipeline_clipped } else { &self.glyph_pipeline },
                        if cmd.clip_active { Some(1) } else { None }
                    ),
                    DrawCommandType::PushClip => (
                        &self.stencil_write_pipeline,
                        Some(1)
                    ),
                    DrawCommandType::PopClip => (
                        &self.stencil_write_pipeline,
                        Some(0)
                    ),
                };

                // Only switch pipeline if necessary
                if last_pipeline != Some(pipeline as *const _) {
                    render_pass.set_pipeline(pipeline);
                    last_pipeline = Some(pipeline as *const _);
                    
                    // Re-bind uniforms
                    render_pass.set_bind_group(0, &bind_group, &[dynamic_offset]);
                    
                    // For glyphs, also bind the atlas
                    if matches!(cmd.cmd_type, DrawCommandType::Glyph) {
                        render_pass.set_bind_group(1, &self.glyph_bind_group, &[]);
                    }
                } else {
                    render_pass.set_bind_group(0, &bind_group, &[dynamic_offset]);
                    // For glyphs, also bind the atlas
                    if matches!(cmd.cmd_type, DrawCommandType::Glyph) {
                        render_pass.set_bind_group(1, &self.glyph_bind_group, &[]);
                    }
                }

                // Only set stencil ref if necessary
                if let Some(r) = stencil_ref {
                    if last_stencil_ref != Some(r) {
                        render_pass.set_stencil_reference(r);
                        last_stencil_ref = Some(r);
                    }
                }

                render_pass.set_bind_group(0, &bind_group, &[dynamic_offset]);
                render_pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
                render_pass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.pending_commands.clear();
    }

    /// Add multiple pre-batched commands.
    pub fn add_commands(&mut self, commands: Vec<DrawCommand>) {
        if commands.is_empty() {
            return;
        }
        for cmd in commands {
            if self.pending_commands.len() >= MAX_PRIMITIVES {
                self.flush();
            }
            self.pending_commands.push(cmd);
        }
    }

    /// Read the canvas pixels back to CPU memory.
    pub fn read_pixels(&mut self) -> Vec<u8> {
        // Flush all pending draws first
        self.flush();

        let aligned_bytes_per_row = (self.width * 4 + 255) & !255;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Copy Encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        let submission_index = self.queue.submit(std::iter::once(encoder.finish()));

        // Map the staging buffer and read the data
        let buffer_slice = self.staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::PollType::Wait { timeout: None, submission_index: Some(submission_index) }).ok();
        rx.recv().unwrap().unwrap();

        let data = buffer_slice.get_mapped_range();
        
        // Remove padding from rows
        let mut result = Vec::with_capacity((self.width * self.height * 4) as usize);
        for y in 0..self.height {
            let start = (y * aligned_bytes_per_row) as usize;
            let end = start + (self.width * 4) as usize;
            result.extend_from_slice(&data[start..end]);
        }

        drop(data);
        self.staging_buffer.unmap();

        result
    }

    /// Prepare the texture for reading (flush pending draws without CPU readback).
    /// Returns a reference to the texture that can be used directly for sampling.
    pub fn prepare_texture(&mut self) -> &wgpu::Texture {
        self.flush();
        &self.texture
    }

    /// Get the cached sRGB texture view for the canvas texture.
    /// Note: Call prepare_texture() first to ensure all draws are flushed.
    /// Returns an sRGB view so samplers convert sRGB->linear automatically.
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.srgb_view
    }
}
