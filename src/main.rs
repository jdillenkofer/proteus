//! Proteus: Cross-platform shader webcam transformer CLI.

mod config_utils;
mod utils;
use config_utils::{ConfigDiff, ConfigWatcher, load_shaders, load_textures, init_capture};
use utils::FpsCounter;

use anyhow::Result;
use clap::{CommandFactory, Parser, ValueEnum};
use proteus::capture::{AsyncCapture, CaptureBackend, CaptureConfig, NokhwaCapture};
use proteus::output::window_output::WindowRenderer;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use proteus::output::{OutputBackend, VirtualCameraConfig, VirtualCameraOutput};
use proteus::shader::{WgpuPipeline, ShaderPipeline};
use proteus::shader::gpu_context::GpuContext;
use serde::Deserialize;
use std::path::PathBuf;
use std::fs;
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
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TextureInput {
    Image { path: PathBuf },
    Video { path: PathBuf },
}

/// Configuration file structure.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Path to the config file (if loaded from file)
    #[serde(skip)]
    pub config_path: Option<PathBuf>,
    /// Camera device ID (index or name)
    pub input: String,
    /// Path to GLSL fragment shader file(s)
    pub shader: Vec<PathBuf>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Maximum frame width (for camera format selection)
    pub max_input_width: Option<u32>,
    /// Maximum frame height (for camera format selection)
    pub max_input_height: Option<u32>,
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
            config_path: None,
            input: "0".to_string(),
            shader: Vec::new(),
            width: 1920,
            height: 1080,
            max_input_width: None,
            max_input_height: None,
            fps: 30,
            output: OutputMode::Window,
            textures: Vec::new(),
        }
    }
}

impl Config {
    /// Create a Config from CLI arguments.
    /// This handles the texture ordering from interleaved --image and --video flags.
    fn from_cli_args(args: Args) -> Self {
        // Build ordered texture inputs from CLI image/video args
        let matches = Args::command().get_matches();
        let mut ordered_inputs: Vec<(usize, TextureInput)> = Vec::new();

        if let Some(indices) = matches.indices_of("video") {
            let paths: Vec<&PathBuf> = args.video.iter().collect();
            for (i, idx) in indices.enumerate() {
                if i < paths.len() {
                    ordered_inputs.push((idx, TextureInput::Video { path: paths[i].clone() }));
                }
            }
        }

        if let Some(indices) = matches.indices_of("image") {
            let paths: Vec<&PathBuf> = args.image.iter().collect();
            for (i, idx) in indices.enumerate() {
                if i < paths.len() {
                    ordered_inputs.push((idx, TextureInput::Image { path: paths[i].clone() }));
                }
            }
        }

        ordered_inputs.sort_by_key(|k| k.0);
        let textures = ordered_inputs.into_iter().map(|(_, t)| t).collect();

        Self {
            config_path: None,
            input: args.input,
            shader: args.shader,
            width: args.width,
            height: args.height,
            max_input_width: args.max_input_width,
            max_input_height: args.max_input_height,
            fps: args.fps,
            output: args.output,
            textures,
        }
    }
    
    /// Load configuration from a YAML file.
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {:?}: {}", path, e))?;
        
        let mut config: Config = serde_yaml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file {:?}: {}", path, e))?;
        
        config.config_path = Some(path.clone());
        info!("Loaded configuration from {:?}", path);
        
        Ok(config)
    }
}

/// Cross-platform shader webcam transformer.
#[derive(Parser, Debug)]
#[command(name = "proteus")]
#[command(about = "Apply GPU shaders to webcam video in real-time")]
#[command(group = clap::ArgGroup::new("config_or_options")
    .required(false)
    .args(["config"])
    .conflicts_with_all(["input", "shader", "width", "height", "max_input_width", "max_input_height", "fps", "output", "image", "video"])
)]
struct Args {
    /// Path to YAML configuration file (mutually exclusive with other options)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Camera device ID (index or name)
    #[arg(short, long, default_value = "0")]
    input: String,

    /// Path to GLSL fragment shader file(s)
    #[arg(short, long, num_args = 1..)]
    shader: Vec<PathBuf>,

    /// Frame width
    #[arg(long, default_value = "1920")]
    width: u32,

    /// Frame height
    #[arg(long, default_value = "1080")]
    height: u32,

    /// Maximum frame width (defaults to width if not specified)
    #[arg(long)]
    max_input_width: Option<u32>,

    /// Maximum frame height (defaults to height if not specified)
    #[arg(long)]
    max_input_height: Option<u32>,

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
    config: Config,
    window: Option<Arc<Window>>,
    renderer: Option<WindowRenderer>,
    capture: Option<AsyncCapture>,
    context: Option<Arc<GpuContext>>,
    pipeline: Option<WgpuPipeline>,
    last_frame_time: Instant,
    frame_duration: Duration,
    start_time: Instant,
    fps_counter: FpsCounter,
    // Config hot-reloading
    config_watcher: Option<ConfigWatcher>,
}

impl ProteusApp {
    fn new(config: Config) -> Self {
        let frame_duration = Duration::from_secs_f64(1.0 / config.fps as f64);
        
        let config_watcher = ConfigWatcher::new(config.config_path.clone());

        Self {
            config,
            window: None,
            renderer: None,
            capture: None,
            context: None,
            pipeline: None,
            last_frame_time: Instant::now(),
            frame_duration,
            start_time: Instant::now(),
            fps_counter: FpsCounter::new(),
            config_watcher,
        }
    }

    fn initialize(&mut self) -> Result<()> {
        // Initialize camera capture
        let capture_config = CaptureConfig {
            device_id: self.config.input.clone(),
            width: self.config.width,
            height: self.config.height,
            max_input_width: self.config.max_input_width.unwrap_or(self.config.width),
            max_input_height: self.config.max_input_height.unwrap_or(self.config.height),
            fps: self.config.fps,
        };

        info!("Opening camera device {}...", self.config.input);
        
        if let Some(capture) = init_capture(capture_config) {
             let (cam_w, cam_h) = capture.frame_size();
             info!("Camera opened successfully at {}x{} (async capture)", cam_w, cam_h);
             self.capture = Some(capture);
        } else {
             error!("Failed to initialize camera capture");
             // Don't error out, just continue without capture (recoverable via config reload)
        }

        // Load shaders if provided
        let shaders = load_shaders(&self.config.shader);

        // Initialize shader pipeline with textures from config
        let texture_sources = load_textures(&self.config.textures);
        
        let context = self.context.clone().ok_or_else(|| anyhow::anyhow!("GPU context not initialized"))?;
        self.pipeline = Some(WgpuPipeline::new(context, self.config.width, self.config.height, shaders, texture_sources)?);
        info!("Shader pipeline initialized");

        Ok(())
    }

    fn process_frame(&mut self) {
        // Check for config reload first
        self.check_config_reload();

        let Some(capture) = &mut self.capture else {
            return;
        };
        let Some(pipeline) = &mut self.pipeline else {
            return;
        };
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        if let Some(fps) = self.fps_counter.update() {
            debug!("[Perf] Rendering at {:.2} FPS (Resolution: {}x{})", fps, self.config.width, self.config.height);
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

    /// Check for config file updates and reload if necessary.
    fn check_config_reload(&mut self) {
        if let Some(watcher) = &mut self.config_watcher {
            if let Some((old_config, new_config)) = watcher.check_for_changes() {
                self.handle_config_change(old_config, new_config);
            }
        }
    }

    fn handle_config_change(&mut self, old_config_opt: Option<Config>, new_config: Config) {
        if let Some(old_config) = old_config_opt {
            let diff = ConfigDiff::compare(&old_config, &new_config);
            
            if diff.requires_restart {
                tracing::warn!("Changes to output, input, width, height, max_input_width, max_input_height, or fps require a restart.");
            }

            if diff.needs_pipeline_reload() {
                info!("Reloading pipeline due to shader/texture changes...");
                if let Err(e) = self.rebuild_pipeline(&new_config) {
                     error!("Failed to rebuild pipeline: {}", e);
                } else {
                     info!("Pipeline reloaded successfully");
                }
            }
        }
    }

    fn rebuild_pipeline(&mut self, config: &Config) -> Result<()> {
       let shaders = load_shaders(&config.shader);
       let texture_sources = load_textures(&config.textures);
       
       let context = self.context.clone().ok_or_else(|| anyhow::anyhow!("No GPU context"))?;
       let pipeline = WgpuPipeline::new(context, self.config.width, self.config.height, shaders, texture_sources)?;
       self.pipeline = Some(pipeline);
       Ok(())
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
            .with_inner_size(PhysicalSize::new(self.config.width, self.config.height));

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

    // Load config from file or build from CLI args
    let config = if let Some(config_path) = &cli_args.config {
        Config::from_file(config_path)?
    } else {
        Config::from_cli_args(cli_args)
    };

    info!("Starting Proteus...");

    // Dispatch based on output mode
    match config.output {
        OutputMode::Window => run_window_mode(config)?,
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        OutputMode::VirtualCamera => run_virtual_camera_mode(config)?,
    }

    Ok(())
}

/// Run in window output mode (default).
fn run_window_mode(config: Config) -> Result<()> {
    let mut app = ProteusApp::new(config);

    // Create event loop
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    // Run app
    event_loop.run_app(&mut app)?;

    Ok(())
}

/// Run in virtual camera output mode.
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
fn run_virtual_camera_mode(config: Config) -> Result<()> {
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
    let capture_config = CaptureConfig {
        device_id: config.input.clone(),
        width: config.width,
        height: config.height,
        max_input_width: config.max_input_width.unwrap_or(config.width),
        max_input_height: config.max_input_height.unwrap_or(config.height),
        fps: config.fps,
    };

    info!("Opening camera device {}...", config.input);
    let mut capture = Some(AsyncCapture::new(capture_config)?);
    info!("Camera opened successfully (async capture)");

    // Load shaders if provided
    let shaders = load_shaders(&config.shader);

    // Build texture sources from config textures
    let texture_sources = load_textures(&config.textures);
    
    // Initialize config watcher if config file is used
    let mut config_watcher = ConfigWatcher::new(config.config_path.clone());

    // Initialize GPU Context (headless/no-window)
    let context = Arc::new(GpuContext::new(None)?);

    let mut pipeline = WgpuPipeline::new(context.clone(), config.width, config.height, shaders, texture_sources)?;
    info!("Shader pipeline initialized");

    // Initialize virtual camera output
    let vc_config = VirtualCameraConfig {
        width: config.width,
        height: config.height,
        fps: config.fps,
        ..Default::default()
    };
    let mut output = VirtualCameraOutput::new(vc_config)?;
    info!("Virtual camera output initialized");

    let frame_duration = Duration::from_secs_f64(1.0 / config.fps as f64);
    let start_time = Instant::now();
    let mut fps_counter = FpsCounter::new();
    info!("Starting virtual camera stream at {} fps", config.fps);

    // Main loop
    while running.load(Ordering::SeqCst) {
        // Check for config reload
        if let Some(watcher) = &mut config_watcher {
            if let Some((old_config_opt, new_config)) = watcher.check_for_changes() {
                 if let Some(old_config) = old_config_opt {
                     let diff = ConfigDiff::compare(&old_config, &new_config);
                     
                     if diff.requires_restart {
                         tracing::warn!("Changes to output, input, width, height, max_input_width, max_input_height, or fps require a restart.");
                     }

                    if diff.needs_pipeline_reload() {
                        info!("Reloading pipeline due to shader/texture changes...");
                        let new_shaders = load_shaders(&new_config.shader);
                        let new_texture_sources = load_textures(&new_config.textures);
                       
                        match WgpuPipeline::new(context.clone(), config.width, config.height, new_shaders, new_texture_sources) {
                           Ok(new_pipeline) => {
                               pipeline = new_pipeline;
                               info!("Pipeline reloaded successfully");
                           }
                           Err(e) => error!("Failed to rebuild pipeline: {}", e),
                        }
                    }
                 }
            }
        }

        let frame_start = Instant::now();

        // FPS counter
        if let Some(fps) = fps_counter.update() {
            info!("Virtual camera: {:.2} FPS", fps);
        }

        // Get latest frame (non-blocking)
        let frame_option = if let Some(cap) = &mut capture {
            cap.get_latest_frame()
        } else {
            None
        };

        if let Some(frame) = frame_option {
            // Process through shader
            let time = start_time.elapsed().as_secs_f32();
            let shader_start = Instant::now();
            match pipeline.process_frame(frame, time) {
                Ok(processed) => {
                    let shader_elapsed = shader_start.elapsed();
                    // Write to virtual camera
                    let write_start = Instant::now();
                    if let Err(e) = output.write_frame(&processed) {
                        error!("Output error: {}", e);
                    }
                    let write_elapsed = write_start.elapsed();
                    debug!("[Perf] Virtual Camera - Shader: {:?}, Write: {:?}", shader_elapsed, write_elapsed);
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
