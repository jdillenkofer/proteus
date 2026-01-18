//! Proteus: Cross-platform shader webcam transformer CLI.

use anyhow::Result;
use clap::{CommandFactory, Parser, ValueEnum};
use proteus::capture::{AsyncCapture, CaptureBackend, CaptureConfig, NokhwaCapture};
use proteus::output::window_output::WindowRenderer;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use proteus::output::{OutputBackend, VirtualCameraConfig, VirtualCameraOutput};
use proteus::shader::{ShaderSource, TextureSlot, WgpuPipeline};
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use proteus::shader::ShaderPipeline;
use proteus::shader::gpu_context::GpuContext;
use proteus::video::VideoPlayer;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

/// Output mode for processed video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputMode {
    /// Display in a window (default)
    Window,
    /// Output to virtual camera
    /// - Windows: Requires OBS Virtual Camera
    /// - Linux: Requires v4l2loopback
    /// - macOS: Requires OBS 30+ Virtual Camera
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    VirtualCamera,
}

/// A texture input for shaders (image or video).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TextureInput {
    Image { path: PathBuf },
    Video { path: PathBuf },
}

/// Configuration file structure.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Camera device index
    pub input: u32,
    /// Path to GLSL fragment shader file(s)
    pub shader: Vec<PathBuf>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Target frames per second
    pub fps: u32,
    /// Output mode: window or virtual-camera
    pub output: OutputMode,
    /// Ordered texture inputs (images and videos)
    pub textures: Vec<TextureInput>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input: 0,
            shader: Vec::new(),
            width: 1920,
            height: 1080,
            fps: 30,
            output: OutputMode::Window,
            textures: Vec::new(),
        }
    }
}

/// Cross-platform shader webcam transformer.
#[derive(Parser, Debug)]
#[command(name = "proteus")]
#[command(about = "Apply GPU shaders to webcam video in real-time")]
#[command(group = clap::ArgGroup::new("config_or_options")
    .required(false)
    .args(["config"])
    .conflicts_with_all(["input", "shader", "width", "height", "fps", "output", "image", "video"])
)]
struct Args {
    /// Path to YAML configuration file (mutually exclusive with other options)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Camera device index
    #[arg(short, long, default_value = "0")]
    input: u32,

    /// Path to GLSL fragment shader file(s)
    #[arg(short, long, num_args = 1..)]
    shader: Vec<PathBuf>,

    /// Frame width
    #[arg(long, default_value = "1920")]
    width: u32,

    /// Frame height
    #[arg(long, default_value = "1080")]
    height: u32,

    /// Target frames per second
    #[arg(long, default_value = "30")]
    fps: u32,

    /// List available cameras and exit
    #[arg(long)]
    list_devices: bool,

    /// Output mode: window or virtual-camera
    #[arg(long, value_enum, default_value = "window")]
    output: OutputMode,

    /// Path to image file(s) for shader use (up to 4 total with videos, black if not provided)
    #[arg(long, num_args = 0..=4)]
    image: Vec<PathBuf>,

    /// Path to video file(s) for shader use (up to 4 total with images)
    #[arg(long, num_args = 0..=4)]
    video: Vec<PathBuf>,
}

/// Application state for the event loop.
struct ProteusApp {
    args: Args,
    ordered_inputs: Vec<(TextureInputType, PathBuf)>,
    window: Option<Arc<Window>>,
    renderer: Option<WindowRenderer>,
    capture: Option<AsyncCapture>,
    context: Option<Arc<GpuContext>>,
    pipeline: Option<WgpuPipeline>,
    last_frame_time: Instant,
    frame_duration: Duration,
    start_time: Instant,
    frame_count: u32,
    fps_last_time: Instant,
}

impl ProteusApp {
    fn new(args: Args, ordered_inputs: Vec<(TextureInputType, PathBuf)>) -> Self {
        let frame_duration = Duration::from_secs_f64(1.0 / args.fps as f64);
        Self {
            args,
            ordered_inputs,
            window: None,
            renderer: None,
            capture: None,
            context: None,
            pipeline: None,
            last_frame_time: Instant::now(),
            frame_duration,
            start_time: Instant::now(),
            frame_count: 0,
            fps_last_time: Instant::now(),
        }
    }

    fn initialize(&mut self) -> Result<()> {
        // Initialize camera capture
        let config = CaptureConfig {
            device_index: self.args.input,
            width: self.args.width,
            height: self.args.height,
            fps: self.args.fps,
        };

        info!("Opening camera device {}...", self.args.input);
        let capture = AsyncCapture::new(config)?;
        let (cam_w, cam_h) = capture.frame_size();
        info!("Camera opened successfully at {}x{} (async capture)", cam_w, cam_h);
        self.capture = Some(capture);

        // Load shaders if provided
        let mut shaders = Vec::new();
        if self.args.shader.is_empty() {
            info!("Using passthrough shader");
            // Empty list implies default behavior in pipeline or we can pass None equivalent
        } else {
            for path in &self.args.shader {
                info!("Loading shader from {:?}", path);
                let source = fs::read_to_string(path)?;
                shaders.push(ShaderSource::Glsl { code: source, path: Some(path.clone()) });
            }
        }

        // Initialize shader pipeline
        // Build texture sources from ordered inputs (up to 4 total)
        let mut texture_sources: Vec<TextureSlot> = Vec::new();
        
        for (input_type, path) in &self.ordered_inputs {
            if texture_sources.len() >= 4 { break; }
            match input_type {
                TextureInputType::Video => {
                    match VideoPlayer::new(path) {
                        Ok(player) => texture_sources.push(TextureSlot::Video(player)),
                        Err(e) => {
                            error!("Failed to open video {:?}: {}", path, e);
                            texture_sources.push(TextureSlot::Empty);
                        }
                    }
                },
                TextureInputType::Image => {
                    texture_sources.push(TextureSlot::Image(path.clone()));
                }
            }
        }
        

        
        let context = self.context.clone().ok_or_else(|| anyhow::anyhow!("GPU context not initialized"))?;
        self.pipeline = Some(WgpuPipeline::new(context, self.args.width, self.args.height, shaders, texture_sources)?);
        info!("Shader pipeline initialized");

        Ok(())
    }

    fn process_frame(&mut self) {
        let Some(capture) = &mut self.capture else {
            return;
        };
        let Some(pipeline) = &mut self.pipeline else {
            return;
        };
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        self.frame_count += 1;
        let elapsed = self.fps_last_time.elapsed();
        if elapsed >= Duration::from_secs(1) {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();
            debug!("[Perf] Rendering at {:.2} FPS (Resolution: {}x{})", fps, self.args.width, self.args.height);
            self.frame_count = 0;
            self.fps_last_time = Instant::now();
        }

        // Get latest frame (non-blocking)
        if let Some(frame) = capture.get_latest_frame() {
            // Calculate time
            let time = self.start_time.elapsed().as_secs_f32();
            
            // Optimized path: Render directly on GPU without CPU readback
            if let Err(e) = pipeline.process_frame_gpu(&frame, time) {
                error!("Shader processing error: {}", e);
                return;
            }

            // Display in window by sharing texture
            if let Some(texture) = pipeline.output_texture() {
                 let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                 if let Err(e) = renderer.render_texture(&view) {
                      error!("Render error: {}", e);
                 }
            }
        }
    }
}

impl ApplicationHandler for ProteusApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Create window
        let window_attrs = WindowAttributes::default()
            .with_title("Proteus - Shader Webcam")
            .with_inner_size(PhysicalSize::new(self.args.width, self.args.height));

        match event_loop.create_window(window_attrs) {
            Ok(window) => {
                let window = Arc::new(window);
                self.window = Some(window.clone());

                // Create GPU context shared between pipeline and renderer
                match GpuContext::new(Some(&window)) {
                    Ok(context) => {
                        let context = Arc::new(context);
                        self.context = Some(context.clone());

                        // Create renderer
                        match WindowRenderer::new(window, context) {
                            Ok(renderer) => {
                                self.renderer = Some(renderer);
                                info!("Window created successfully");

                                // Initialize capture and pipeline
                                if let Err(e) = self.initialize() {
                                    error!("Initialization error: {}", e);
                                    event_loop.exit();
                                }
                            }
                            Err(e) => {
                                error!("Failed to create renderer: {}", e);
                                event_loop.exit();
                            }
                        }
                    }
                    Err(e) => {
                         error!("Failed to create GPU context: {}", e);
                         event_loop.exit();
                    }
                }
            }
            Err(e) => {
                error!("Failed to create window: {}", e);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Window closed");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if now.duration_since(self.last_frame_time) >= self.frame_duration {
                    self.process_frame();
                    self.last_frame_time = now;
                }

                // Request next frame
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Initialize ONNX Runtime
    // We ignore errors here because if ML is not used/model missing, we might survive?
    // But if we want auto-download or proper setup, we should check it.
    // However, if the user doesn't use segmentation, we don't want to crash?
    // But the error happens when loading the dylib.
    if let Err(e) = proteus::ml::SegmentationEngine::init() {
        tracing::warn!("Failed to initialize ONNX Runtime: {}. Segmentation will be unavailable.", e);
    }

    let cli_args = Args::parse();

    // List devices mode (allowed with or without config)
    if cli_args.list_devices {
        println!("Available cameras:");
        match NokhwaCapture::list_devices() {
            Ok(devices) => {
                for device in devices {
                    println!("  [{}] {}", device.index, device.name);
                }
            }
            Err(e) => {
                eprintln!("Failed to list devices: {}", e);
            }
        }
        return Ok(());
    }

    // Load config from file or use CLI args
    let (args, ordered_inputs) = if let Some(config_path) = &cli_args.config {
        load_config(config_path)?
    } else {
        // Build ordered inputs from CLI args
        let ordered_inputs = build_ordered_inputs_from_cli(&cli_args);
        (cli_args, ordered_inputs)
    };

    info!("Starting Proteus...");

    // Dispatch based on output mode
    match args.output {
        OutputMode::Window => run_window_mode(args, ordered_inputs)?,
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        OutputMode::VirtualCamera => run_virtual_camera_mode(args, ordered_inputs)?,
    }

    Ok(())
}

/// Load configuration from a YAML file and convert to Args.
fn load_config(path: &PathBuf) -> Result<(Args, Vec<(TextureInputType, PathBuf)>)> {
    let content = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file {:?}: {}", path, e))?;
    
    let config: Config = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file {:?}: {}", path, e))?;
    
    info!("Loaded configuration from {:?}", path);
    
    // Convert textures to ordered inputs
    let ordered_inputs: Vec<(TextureInputType, PathBuf)> = config.textures
        .iter()
        .map(|t| match t {
            TextureInput::Image { path } => (TextureInputType::Image, path.clone()),
            TextureInput::Video { path } => (TextureInputType::Video, path.clone()),
        })
        .collect();
    
    // Convert Config to Args
    let args = Args {
        config: Some(path.clone()),
        input: config.input,
        shader: config.shader,
        width: config.width,
        height: config.height,
        fps: config.fps,
        list_devices: false,
        output: config.output,
        image: Vec::new(), // Not used when loading from config
        video: Vec::new(), // Not used when loading from config
    };
    
    Ok((args, ordered_inputs))
}

/// Build ordered texture inputs from CLI arguments.
fn build_ordered_inputs_from_cli(args: &Args) -> Vec<(TextureInputType, PathBuf)> {
    let matches = Args::command().get_matches();
    let mut ordered_inputs: Vec<(usize, TextureInputType, PathBuf)> = Vec::new();

    if let Some(indices) = matches.indices_of("video") {
        let paths: Vec<&PathBuf> = args.video.iter().collect();
        for (i, idx) in indices.enumerate() {
            if i < paths.len() {
                ordered_inputs.push((idx, TextureInputType::Video, paths[i].clone()));
            }
        }
    }

    if let Some(indices) = matches.indices_of("image") {
        let paths: Vec<&PathBuf> = args.image.iter().collect();
        for (i, idx) in indices.enumerate() {
            if i < paths.len() {
                ordered_inputs.push((idx, TextureInputType::Image, paths[i].clone()));
            }
        }
    }

    ordered_inputs.sort_by_key(|k| k.0);
    ordered_inputs.into_iter().map(|(_, t, p)| (t, p)).collect()
}

/// Run in window output mode (default).
fn run_window_mode(args: Args, ordered_inputs: Vec<(TextureInputType, PathBuf)>) -> Result<()> {
    let mut app = ProteusApp::new(args, ordered_inputs);

    // Create event loop
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    // Run app
    event_loop.run_app(&mut app)?;

    Ok(())
}

#[derive(Debug, Clone)]
enum TextureInputType {
    Video,
    Image,
}

/// Run in virtual camera output mode.
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
fn run_virtual_camera_mode(args: Args, ordered_inputs: Vec<(TextureInputType, PathBuf)>) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    // Set up signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("Received interrupt signal, shutting down...");
        r.store(false, Ordering::SeqCst);
    })?;

    // Initialize camera capture (async for better performance)
    let config = CaptureConfig {
        device_index: args.input,
        width: args.width,
        height: args.height,
        fps: args.fps,
    };

    info!("Opening camera device {}...", args.input);
    let mut capture = AsyncCapture::new(config)?;
    info!("Camera opened successfully (async capture)");

    // Load shaders if provided
    let mut shaders = Vec::new();
    if args.shader.is_empty() {
        info!("Using passthrough shader");
    } else {
        for path in &args.shader {
            info!("Loading shader from {:?}", path);
            let source = fs::read_to_string(path)?;
            shaders.push(ShaderSource::Glsl { code: source, path: Some(path.clone()) });
        }
    }

    // Build texture sources from ordered inputs (up to 4 total)
    let mut texture_sources: Vec<TextureSlot> = Vec::new();

    for (input_type, path) in &ordered_inputs {
        if texture_sources.len() >= 4 { break; }
        match input_type {
            TextureInputType::Video => {
                match VideoPlayer::new(path) {
                    Ok(player) => texture_sources.push(TextureSlot::Video(player)),
                    Err(e) => {
                        error!("Failed to open video {:?}: {}", path, e);
                        texture_sources.push(TextureSlot::Empty);
                    }
                }
            },
            TextureInputType::Image => {
                texture_sources.push(TextureSlot::Image(path.clone()));
            }
        }
    }
    
    // Initialize GPU Context (headless/no-window)
    let context = Arc::new(GpuContext::new(None)?);

    let mut pipeline = WgpuPipeline::new(context, args.width, args.height, shaders, texture_sources)?;
    info!("Shader pipeline initialized");

    // Initialize virtual camera output
    let vc_config = VirtualCameraConfig {
        width: args.width,
        height: args.height,
        fps: args.fps,
        ..Default::default()
    };
    let mut output = VirtualCameraOutput::new(vc_config)?;
    info!("Virtual camera output initialized");

    let frame_duration = Duration::from_secs_f64(1.0 / args.fps as f64);
    let start_time = Instant::now();
    let mut frame_count = 0u32;
    let mut fps_last_time = Instant::now();
    info!("Starting virtual camera stream at {} fps", args.fps);

    // Main loop
    while running.load(Ordering::SeqCst) {
        let frame_start = Instant::now();

        // FPS counter
        frame_count += 1;
        let elapsed_fps = fps_last_time.elapsed();
        if elapsed_fps >= Duration::from_secs(1) {
            let fps = frame_count as f32 / elapsed_fps.as_secs_f32();
            info!("Virtual camera: {:.2} FPS", fps);
            frame_count = 0;
            fps_last_time = Instant::now();
        }

        // Get latest frame (non-blocking)
        if let Some(frame) = capture.get_latest_frame() {
            // Process through shader
            let time = start_time.elapsed().as_secs_f32();
            match pipeline.process_frame(frame, time) {
                Ok(processed) => {
                    // Write to virtual camera
                    if let Err(e) = output.write_frame(&processed) {
                        error!("Output error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Shader processing error: {}", e);
                }
            }
        }

        // Frame rate limiting
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
    }

    info!("Virtual camera stream stopped");
    Ok(())
}
