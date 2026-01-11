//! wgpu-based GPU shader pipeline.

use super::{ShaderPipeline, ShaderSource};
use crate::frame::{PixelFormat, QuadVertex, VideoFrame};
use crate::video::VideoPlayer;
use anyhow::{anyhow, Result};
use naga::front::glsl::{Frontend, Options};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::ShaderStage;
use std::borrow::Cow;
use tracing::info;
use wgpu::util::DeviceExt;

use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event};
use std::sync::mpsc::{channel, Receiver};

/// Source for a texture slot - either a static image or a video.
pub enum TextureSlot {
    /// Path to a static image file
    Image(std::path::PathBuf),
    /// Video player for dynamic frames
    Video(VideoPlayer),
    /// Empty slot (will use 1x1 black texture)
    Empty,
}

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
    pub seed: f32,
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
    segmentation_engine: Option<crate::ml::AsyncSegmentationEngine>,
    mask_texture: wgpu::Texture,
    image_textures: [wgpu::Texture; 4],
    loaded_textures: [Option<wgpu::Texture>; 4], // Keep original loaded textures to avoid reloading images
    current_video_texture_sizes: [Option<(u32, u32)>; 4],
    /// Video players for dynamic texture slots
    video_players: Vec<VideoPlayer>,
    /// Which texture slots are videos (index into video_players)
    video_slot_map: [Option<usize>; 4],

    // Performance Cache
    input_texture: Option<wgpu::Texture>,
    output_textures: Vec<wgpu::Texture>,
    readback_buffer: Option<wgpu::Buffer>,
    bind_groups: Vec<wgpu::BindGroup>,
    cached_width: u32,
    cached_height: u32,
    cached_mask_width: u32,
    cached_mask_height: u32,
    frame_count: u64,
    
    // Shader hot-reloading
    watcher: Option<RecommendedWatcher>,
    reload_rx: Option<Receiver<std::result::Result<Event, notify::Error>>>,
    shader_sources: Vec<ShaderSource>, // Keep sources to re-compile
    vertex_shader_module: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
}

impl WgpuPipeline {
    /// Creates a new wgpu pipeline with the given shaders.
    /// Segmentation is automatically enabled if any shader uses the mask binding (binding 3).
    /// Texture sources (up to 4) are used for bindings 4-7 in the order specified.
    pub fn new(
        width: u32,
        height: u32,
        shaders: Vec<ShaderSource>,
        texture_sources: Vec<TextureSlot>,
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| anyhow!("Failed to find GPU adapter: {:?}", e))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Proteus Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            },
        ))?;

        // Prepare shader sources and detect if any shader uses the mask binding
        let mut needs_segmentation = false;
        let shader_sources = if shaders.is_empty() {
            vec![(DEFAULT_FRAGMENT_SHADER.to_string(), "fs_main")]
        } else {
            let mut sources = Vec::new();
            for shader in &shaders {
                let (fragment_wgsl, fragment_entry_point, uses_mask) = match shader {
                    ShaderSource::Glsl { code: glsl, .. } => {
                        let (wgsl, uses_mask) = Self::glsl_to_wgsl(&glsl)?;
                        (wgsl, "main", uses_mask)
                    }
                    ShaderSource::Wgsl { code: wgsl, .. } => {
                        let uses_mask = Self::wgsl_uses_mask(&wgsl);
                        (wgsl.clone(), "fs_main", uses_mask)
                    }
                };
                if uses_mask {
                    needs_segmentation = true;
                }
                sources.push((fragment_wgsl, fragment_entry_point));
            }
            sources
        };
        
        if needs_segmentation {
            info!("Auto-enabling segmentation: shader uses t_mask binding");
        }

        // Create shader modules
        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(VERTEX_SHADER)),
        });

        // Create bind group layout
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Image textures (t_image0 through t_image3)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
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
                multiview_mask: None,
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
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniforms = Uniforms {
            time: 0.0,
            width: width as f32,
            height: height as f32,
            seed: 0.0,
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let segmentation_engine = if needs_segmentation {
             crate::ml::AsyncSegmentationEngine::new()?
        } else {
            None
        };

        let mask_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Segmentation Mask"),
            size: wgpu::Extent3d {
                width: if segmentation_engine.is_some() { width } else { 1 },
                height: if segmentation_engine.is_some() { height } else { 1 },
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
             wgpu::TexelCopyTextureInfo {
                texture: &mask_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8], // Default to WHITE (1.0) so person is visible if ML off
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );

        // Process texture sources (videos, images, or empty)
        let mut video_players: Vec<VideoPlayer> = Vec::new();
        let mut video_slot_map: [Option<usize>; 4] = [None; 4];
        let mut loaded_textures: Vec<Option<wgpu::Texture>> = vec![None; 4];
        
        for (i, source) in texture_sources.into_iter().enumerate() {
            if i >= 4 { break; }
            match source {
                TextureSlot::Image(path) => {
                    match image::open(&path) {
                        Ok(img) => {
                            let rgba = img.to_rgba8();
                            let (w, h) = rgba.dimensions();
                            info!("Loaded image {} from {:?} ({}x{})", i, path, w, h);
                            let texture = device.create_texture(&wgpu::TextureDescriptor {
                                label: Some(&format!("Image Texture {}", i)),
                                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: wgpu::TextureFormat::Rgba8Unorm,
                                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                                view_formats: &[],
                            });
                            queue.write_texture(
                                wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                                &rgba,
                                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
                                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                            );
                            loaded_textures[i] = Some(texture);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load image {:?}: {}. Using black texture.", path, e);
                        }
                    }
                }
                TextureSlot::Video(player) => {
                    info!("Video slot {} ({}x{})", i, player.width, player.height);
                    video_slot_map[i] = Some(video_players.len());
                    video_players.push(player);
                }
                TextureSlot::Empty => {}
            }
        }
        
        // Create textures for each slot (use loaded or black fallback)
        
        // Setup file watcher
        let (watcher, reload_rx) = if !shaders.is_empty() {
             let (tx, rx) = channel();
             match RecommendedWatcher::new(tx, notify::Config::default()) {
                 Ok(mut w) => {
                     for source in &shaders {
                         if let ShaderSource::Glsl { path: Some(p), .. } | ShaderSource::Wgsl { path: Some(p), .. } = source {
                             if let Err(e) = w.watch(p, RecursiveMode::NonRecursive) {
                                 tracing::warn!("Failed to watch shader file {:?}: {}", p, e);
                             } else {
                                 info!("Watching shader file {:?} for changes", p);
                             }
                         }
                     }
                     (Some(w), Some(rx))
                 }
                 Err(e) => {
                     tracing::warn!("Failed to create file watcher: {}", e);
                     (None, None)
                 }
             }
        } else {
            (None, None)
        };

        let image_textures = std::array::from_fn(|i| {
            loaded_textures[i].take().unwrap_or_else(|| Self::create_black_texture(&device, &queue, i))
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
            segmentation_engine,
            mask_texture,
            image_textures,
            loaded_textures: [None, None, None, None], // Consumed above
            current_video_texture_sizes: [None; 4],
            video_players,
            video_slot_map,
            input_texture: None,
            output_textures: Vec::new(),
            readback_buffer: None,
            bind_groups: Vec::new(),
            cached_width: 0,
            cached_height: 0,
            cached_mask_width: 0,
            cached_mask_height: 0,
            frame_count: 0,
            watcher,
            reload_rx,
            shader_sources: shaders,
            vertex_shader_module: vertex_module,
            pipeline_layout,
        })
    }

    /// Check for shader file updates and reload if necessary.
    fn check_reload(&mut self) {
        let Some(rx) = &self.reload_rx else { return; };
        
        let mut needs_reload = false;
        // Drain channel to clear backlog and debounce
        while let Ok(res) = rx.try_recv() {
            match res {
                Ok(event) => {
                    // Check for modify events (and also create/remove just in case editors do weird atomic saves)
                    if matches!(event.kind, notify::EventKind::Modify(_) | notify::EventKind::Create(_)) {
                         needs_reload = true;
                         info!("Shader file modified: {:?}", event.paths);
                    }
                }
                Err(e) => tracing::warn!("Watch error: {}", e),
            }
        }

        if needs_reload {
            // Re-read and re-compile all shaders that changed (or just all for simplicity)
            info!("Reloading shaders...");
            
            // Re-create pipelines
            for (i, source) in self.shader_sources.iter_mut().enumerate() {
                // Clone path to release borrow on source so we can mutate it later
                let path = match source {
                     ShaderSource::Glsl { path: Some(p), .. } => p.clone(),
                     ShaderSource::Wgsl { path: Some(p), .. } => p.clone(),
                     _ => continue,
                };
                
                // Read file
                let code = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to read shader file {:?}: {}", path, e);
                        continue;
                    }
                };
                
                // Update source in memory
                match source {
                    ShaderSource::Glsl { code: c, .. } => *c = code.clone(),
                    ShaderSource::Wgsl { code: c, .. } => *c = code.clone(),
                }
                
                // Compile
                let (fragment_wgsl, fragment_entry_point) = match source {
                     ShaderSource::Glsl { code: glsl, .. } => {
                         match Self::glsl_to_wgsl(glsl) {
                             Ok((wgsl, _)) => (wgsl, "main"),
                             Err(e) => {
                                 tracing::error!("GLSL compilation error in {:?}: {}", path, e);
                                 continue;
                             }
                         }
                     }
                     ShaderSource::Wgsl { code: wgsl, .. } => (wgsl.clone(), "fs_main"),
                };

                let fragment_module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&format!("Fragment Shader {}", i)),
                    source: wgpu::ShaderSource::Wgsl(Cow::Owned(fragment_wgsl)),
                });

                let render_pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(&format!("Render Pipeline {}", i)),
                    layout: Some(&self.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.vertex_shader_module,
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
                    multiview_mask: None,
                    cache: None,
                });
                
                // Replace pipeline
                if i < self.render_pipelines.len() {
                    self.render_pipelines[i] = render_pipeline;
                    info!("Successfully reloaded shader {}", i);
                }
            }
        }
    }

    /// Update or create cached textures/buffers if dimensions changed
    fn ensure_resources(&mut self, width: u32, height: u32, mask_w: u32, mask_h: u32) -> Result<()> {
        if self.cached_width == width && self.cached_height == height 
           && self.cached_mask_width == mask_w && self.cached_mask_height == mask_h {
            return Ok(());
        }

        info!("Creating GPU resources (Frame: {}x{}, Mask: {}x{})", width, height, mask_w, mask_h);
        
        // 1. Mask Texture (Create if size changed)
        if self.cached_mask_width != mask_w || self.cached_mask_height != mask_h {
            self.mask_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Segmentation Mask"),
                size: wgpu::Extent3d { width: mask_w, height: mask_h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
        }

        // 2. Input Texture
        self.input_texture = Some(self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Input Texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        }));

        // 2. Output Textures (Intermediate frames)
        self.output_textures.clear();
        for i in 0..self.render_pipelines.len() {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("Intermediate Texture {}", i)),
                size: wgpu::Extent3d { width: self.output_width, height: self.output_height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.output_textures.push(tex);
        }

        // 3. Readback Buffer
        let size = (self.output_width * self.output_height * 4) as wgpu::BufferAddress;
        self.readback_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Readback Buffer"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        }));

        // 4. Bind Groups
        self.bind_groups.clear();
        let mask_view = self.mask_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let image_views: [wgpu::TextureView; 4] = std::array::from_fn(|i| {
            self.image_textures[i].create_view(&wgpu::TextureViewDescriptor::default())
        });
        
        for i in 0..self.render_pipelines.len() {
            let input_view = if i == 0 {
                self.input_texture.as_ref().unwrap().create_view(&wgpu::TextureViewDescriptor::default())
            } else {
                self.output_textures[i-1].create_view(&wgpu::TextureViewDescriptor::default())
            };

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Bind Group {}", i)),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&input_view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.uniform_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&mask_view) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&image_views[0]) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&image_views[1]) },
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&image_views[2]) },
                    wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&image_views[3]) },
                ],
            });
            self.bind_groups.push(bind_group);
        }

        self.cached_width = width;
        self.cached_height = height;
        self.cached_mask_width = mask_w;
        self.cached_mask_height = mask_h;
        Ok(())
    }

    /// Creates a 1x1 black RGBA texture as fallback for missing image inputs.
    fn create_black_texture(device: &wgpu::Device, queue: &wgpu::Queue, index: usize) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("Black Texture {}", index)),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &[0u8, 0u8, 0u8, 255u8], // Black RGBA
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        texture
    }

    /// Converts GLSL fragment shader to WGSL.
    /// Returns (wgsl_source, uses_mask_binding) where uses_mask_binding is true if the shader
    /// references binding 3 (t_mask texture for segmentation).
    fn glsl_to_wgsl(glsl: &str) -> Result<(String, bool)> {
        let mut frontend = Frontend::default();
        let options = Options::from(ShaderStage::Fragment);
        let module = frontend.parse(&options, glsl).map_err(|e| anyhow!("GLSL parse error: {:?}", e))?;
        
        // Check if shader uses binding 3 (t_mask) via naga reflection
        let uses_mask = module.global_variables.iter().any(|(_, var)| {
            matches!(var.binding, Some(naga::ResourceBinding { group: 0, binding: 3 }))
        });
        
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        let info = validator.validate(&module).map_err(|e| anyhow!("Shader validation error: {:?}", e))?;
        let wgsl = naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty()).map_err(|e| anyhow!("WGSL generation error: {:?}", e))?;
        Ok((wgsl, uses_mask))
    }

    /// Check if a WGSL shader uses binding 3 (t_mask for segmentation).
    fn wgsl_uses_mask(wgsl: &str) -> bool {
        match naga::front::wgsl::parse_str(wgsl) {
            Ok(module) => module.global_variables.iter().any(|(_, var)| {
                matches!(var.binding, Some(naga::ResourceBinding { group: 0, binding: 3 }))
            }),
            Err(_) => false, // If parsing fails, assume no mask usage (error will surface later)
        }
    }

    pub fn device_and_queue(&self) -> (&wgpu::Device, &wgpu::Queue) { (&self.device, &self.queue) }
    pub fn render_pipelines(&self) -> &[wgpu::RenderPipeline] { &self.render_pipelines }
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout { &self.bind_group_layout }
    pub fn buffers(&self) -> (&wgpu::Buffer, &wgpu::Buffer) { (&self.vertex_buffer, &self.index_buffer) }
    pub fn sampler(&self) -> &wgpu::Sampler { &self.sampler }
}

impl ShaderPipeline for WgpuPipeline {
    fn process_frame(&mut self, input: &VideoFrame, time: f32) -> Result<VideoFrame> {
        // Check for hot-reloads
        self.check_reload();

        let start = std::time::Instant::now();
        let rgba_input = input.to_rgba();
        self.frame_count += 1;

        // 1. Try to send frame to ML worker (Non-blocking)
        if let Some(engine) = &mut self.segmentation_engine {
            engine.try_predict(rgba_input.clone());
        }

        // 2. Poll for latest mask result
        let mut mask_result = None;
        if let Some(engine) = &mut self.segmentation_engine {
             mask_result = engine.poll_result();
        }

        // 3. Ensure resources (base size 1920x1080, mask size varies)
        // If no new mask was polled, we just reuse the old sizes so ensure_resources does nothing.
        let (mask_w, mask_h) = if let Some((_, w, h)) = &mask_result { (*w, *h) } else { (self.cached_mask_width, self.cached_mask_height) };
        // Initial case: if everything is 0, default to 1x1
        let final_mask_w = if mask_w == 0 { 1 } else { mask_w };
        let final_mask_h = if mask_h == 0 { 1 } else { mask_h };

        self.ensure_resources(rgba_input.width, rgba_input.height, final_mask_w, final_mask_h)?;
        
        // 3. Update uniform buffer
        let uniforms = Uniforms { 
            time, 
            width: self.output_width as f32, 
            height: self.output_height as f32, 
            seed: rand::random::<f32>(),
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // 4. Upload Mask
        if let Some((mask_data, w, h)) = mask_result {
            let align_mask = 255;
            let padded_width = (w as usize + align_mask) & !align_mask;
            let upload_data = if padded_width == w as usize {
                std::borrow::Cow::Borrowed(&mask_data)
            } else {
                let mut aligned = vec![0u8; padded_width * h as usize];
                for y in 0..h as usize {
                        let src = y * w as usize;
                        let dst = y * padded_width;
                        aligned[dst..dst + w as usize].copy_from_slice(&mask_data[src..src+w as usize]);
                }
                std::borrow::Cow::Owned(aligned)
            };

            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.mask_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &upload_data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_width as u32), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }

        // 5. Update video textures with current frames
        let mut bind_groups_need_update = false;

        for (slot_index, player_index) in self.video_slot_map.iter().enumerate() {
            if let Some(player_idx) = player_index {
                // If the player has a new frame for the current time
                if let Some(frame) = self.video_players[*player_idx].get_frame(time) {
                    let current_texture = &self.image_textures[slot_index];
                    
                    // Check if texture needs resizing
                    if current_texture.width() != frame.width || current_texture.height() != frame.height {
                        info!("Resizing video texture slot {} to {}x{}", slot_index, frame.width, frame.height);
                        
                        let new_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: Some(&format!("Video Texture {}", slot_index)),
                            size: wgpu::Extent3d { width: frame.width, height: frame.height, depth_or_array_layers: 1 },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        });
                        
                        self.image_textures[slot_index] = new_texture;
                        bind_groups_need_update = true;
                    }

                    // Upload video frame to texture
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo { 
                            texture: &self.image_textures[slot_index], 
                            mip_level: 0, 
                            origin: wgpu::Origin3d::ZERO, 
                            aspect: wgpu::TextureAspect::All 
                        },
                        &frame.data,
                        wgpu::TexelCopyBufferLayout { 
                            offset: 0, 
                            bytes_per_row: Some(frame.width * 4), 
                            rows_per_image: Some(frame.height) 
                        },
                        wgpu::Extent3d { width: frame.width, height: frame.height, depth_or_array_layers: 1 },
                    );
                }
            }
        }

        // Recreate bind groups if any texture was resized
        if bind_groups_need_update {
            self.cached_width = 0; // Force update
            self.ensure_resources(rgba_input.width, rgba_input.height, final_mask_w, final_mask_h)?;
        }

        let upload_start = std::time::Instant::now();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: self.input_texture.as_ref().unwrap(), mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &rgba_input.data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(rgba_input.width * 4), rows_per_image: Some(rgba_input.height) },
            wgpu::Extent3d { width: rgba_input.width, height: rgba_input.height, depth_or_array_layers: 1 },
        );
        tracing::debug!("  [Perf] Texture Upload: {:?}", upload_start.elapsed());

        let shader_start = std::time::Instant::now();
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

        for (i, pipeline) in self.render_pipelines.iter().enumerate() {
            let output_view = self.output_textures[i].create_view(&wgpu::TextureViewDescriptor::default());

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&format!("Render Pass {}", i)),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output_view,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                render_pass.set_pipeline(pipeline);
                render_pass.set_bind_group(0, &self.bind_groups[i], &[]);
                render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, 0..1);
            }
        }

        let final_texture = self.output_textures.last().unwrap();
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: final_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: self.readback_buffer.as_ref().unwrap(), layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.output_width * 4), rows_per_image: Some(self.output_height) } },
            wgpu::Extent3d { width: self.output_width, height: self.output_height, depth_or_array_layers: 1 },
        );

        self.queue.submit(std::iter::once(encoder.finish()));
        tracing::debug!("  [Perf] Shader Dispatch: {:?}", shader_start.elapsed());

        let readback_start = std::time::Instant::now();
        let buffer_slice = self.readback_buffer.as_ref().unwrap().slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| sender.send(result).unwrap());
        self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).unwrap();
        receiver.recv()??;

        let data = buffer_slice.get_mapped_range();
        let output_data = data.to_vec();
        drop(data);
        self.readback_buffer.as_ref().unwrap().unmap();
        
        tracing::debug!("  [Perf] GPU Readback: {:?}", readback_start.elapsed());
        tracing::debug!("  [Perf] TOTAL FRAME: {:?}", start.elapsed());

        Ok(VideoFrame::from_data(self.output_width, self.output_height, PixelFormat::Rgba, output_data))
    }


}
