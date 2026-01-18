//! Proteus: Cross-platform shader webcam transformer CLI.

mod config_utils;
use config_utils::{ConfigWatcher, load_shaders, load_textures, init_capture_with_retry};

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
    // Config hot-reloading
    config_watcher: Option<ConfigWatcher>,
}

impl ProteusApp {
    fn new(args: Args, ordered_inputs: Vec<(TextureInputType, PathBuf)>) -> Self {
        let frame_duration = Duration::from_secs_f64(1.0 / args.fps as f64);
        
        let config_watcher = ConfigWatcher::new(args.config.clone());

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
            config_watcher,
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
        
        if let Some(capture) = init_capture_with_retry(config) {
             let (cam_w, cam_h) = capture.frame_size();
             info!("Camera opened successfully at {}x{} (async capture)", cam_w, cam_h);
             self.capture = Some(capture);
        } else {
             error!("Failed to initialize camera capture");
             // Don't error out, just continue without capture (recoverable via config reload)
        }

        // Load shaders if provided
        let shaders = load_shaders(&self.args.shader);

        // Initialize shader pipeline
        // Build texture sources from ordered inputs (up to 4 total)
        let texture_sources = load_textures(&self.ordered_inputs);
        

        
        let context = self.context.clone().ok_or_else(|| anyhow::anyhow!("GPU context not initialized"))?;
        self.pipeline = Some(WgpuPipeline::new(context, self.args.width, self.args.height, shaders, texture_sources)?);
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
            let mut recreate_pipeline = false;

            // 1. Output Change (Unavailable)
            if new_config.output != old_config.output {
                tracing::warn!("Changing output mode requires a restart.");
            }

            // 2. Capture/Window Change (Input, Width, Height, FPS)
            if new_config.input != old_config.input ||
               new_config.width != old_config.width ||
               new_config.height != old_config.height ||
               new_config.fps != old_config.fps {
                
                info!("Config change detected: Re-initializing capture...");

                // Update FPS/Duration
                self.args.fps = new_config.fps;
                self.frame_duration = Duration::from_secs_f64(1.0 / new_config.fps as f64);
                
                // Update dimensions
                self.args.width = new_config.width;
                self.args.height = new_config.height;
                self.args.input = new_config.input;

                // Re-initialize capture
                let capture_config = CaptureConfig {
                    device_index: new_config.input,
                    width: new_config.width,
                    height: new_config.height,
                    fps: new_config.fps,
                };

                // Drop old capture first
                self.capture = None;
                
                if let Some(capture) = init_capture_with_retry(capture_config) {
                     self.capture = Some(capture);
                     info!("Capture re-initialized (Device: {}, {}x{} @ {}fps)", 
                           new_config.input, new_config.width, new_config.height, new_config.fps);
                } else {
                     error!("Failed to re-initialize capture");
                }

                // Resize Window if needed
                if new_config.width != old_config.width || new_config.height != old_config.height {
                    if let Some(window) = &self.window {
                        let size = PhysicalSize::new(new_config.width, new_config.height);
                        let _ = window.request_inner_size(size);
                    }
                    recreate_pipeline = true; // Pipeline depends on resolution
                }
            }

            // 3. Shader/Texture Change
            if new_config.shader != old_config.shader || new_config.textures != old_config.textures {
                recreate_pipeline = true;
            }

            if recreate_pipeline {
                info!("Reloading pipeline due to config changes...");
                if let Err(e) = self.rebuild_pipeline(&new_config) {
                     error!("Failed to rebuild pipeline: {}", e);
                } else {
                     info!("Pipeline reloaded successfully");
                }
            }
        } else {
             // Should not happen if initialized correctly
             // self.current_config = Some(new_config.clone());
        }
        // self.current_config = Some(new_config);
    }

    fn rebuild_pipeline(&mut self, config: &Config) -> Result<()> {
       // Load shaders
       let shaders = load_shaders(&config.shader);
       
       // Load textures - need to convert config textures to ordered inputs format temporarily or 
       // just update load_textures to handle simple iterator?
       // Actually load_textures takes `&[(TextureInputType, PathBuf)]`.
       // We should construct that vector here.
       let ordered_inputs: Vec<(TextureInputType, PathBuf)> = config.textures
           .iter()
           .map(|t| match t {
               TextureInput::Image { path } => (TextureInputType::Image, path.clone()),
               TextureInput::Video { path } => (TextureInputType::Video, path.clone()),
           })
           .collect();
           
       let texture_sources = load_textures(&ordered_inputs);
       
       let context = self.context.clone().ok_or_else(|| anyhow::anyhow!("No GPU context"))?;
       let pipeline = WgpuPipeline::new(context, self.args.width, self.args.height, shaders, texture_sources)?;
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
    let mut capture = Some(AsyncCapture::new(config)?);
    info!("Camera opened successfully (async capture)");

    // Load shaders if provided
    let shaders = load_shaders(&args.shader);

    // Build texture sources from ordered inputs (up to 4 total)
    let texture_sources = load_textures(&ordered_inputs);
    
    // Initialize config watcher if config file is used
    let mut config_watcher = ConfigWatcher::new(args.config.clone());

    // Initialize GPU Context (headless/no-window)
    let context = Arc::new(GpuContext::new(None)?);

    let mut pipeline = WgpuPipeline::new(context.clone(), args.width, args.height, shaders, texture_sources)?;
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

    let mut frame_duration = Duration::from_secs_f64(1.0 / args.fps as f64);
    let start_time = Instant::now();
    let mut frame_count = 0u32;
    let mut fps_last_time = Instant::now();
    info!("Starting virtual camera stream at {} fps", args.fps);

    // Main loop
    while running.load(Ordering::SeqCst) {
        // Check for config reload
        if let Some(watcher) = &mut config_watcher {
            if let Some((old_config_opt, new_config)) = watcher.check_for_changes() {
                 if let Some(old_config) = old_config_opt {
                     let mut recreate_pipeline = false;
                     
                     if new_config.output != old_config.output {
                         tracing::warn!("Changing output mode requires a restart.");
                     }

                     // Check for re-initialization fields
                    if new_config.input != old_config.input ||
                       new_config.width != old_config.width ||
                       new_config.height != old_config.height ||
                       new_config.fps != old_config.fps {
                        
                        info!("Config change detected: Re-initializing capture and output...");

                        // Update Loop timing
                        frame_duration = Duration::from_secs_f64(1.0 / new_config.fps as f64);
                        
                        // Re-initialize capture
                        let capture_config = CaptureConfig {
                            device_index: new_config.input,
                            width: new_config.width,
                            height: new_config.height,
                            fps: new_config.fps,
                        };
                        
                        // Drop old capture first
                        capture = None;
                        
                        if let Some(new_capture) = init_capture_with_retry(capture_config) {
                             capture = Some(new_capture);
                             info!("Capture re-initialized (Device: {}, {}x{} @ {}fps)", 
                                   new_config.input, new_config.width, new_config.height, new_config.fps);
                        } else {
                             error!("Failed to re-initialize capture");
                        }

                        // Re-initialize Virtual Camera Output
                        let vc_config = VirtualCameraConfig {
                            width: new_config.width,
                            height: new_config.height,
                            fps: new_config.fps,
                            ..Default::default()
                        };
                        match VirtualCameraOutput::new(vc_config) {
                            Ok(new_output) => output = new_output,
                            Err(e) => error!("Failed to re-initialize virtual camera output: {}", e),
                        }
                        
                        recreate_pipeline = true; // Pipeline depends on resolution
                    }

                    // Check for hot-reloadable changes
                    if new_config.shader != old_config.shader || new_config.textures != old_config.textures {
                        recreate_pipeline = true;
                    }

                    if recreate_pipeline {
                        info!("Reloading pipeline due to config changes...");
                        
                        // Load shaders
                        let new_shaders = load_shaders(&new_config.shader);
                       
                       // Load textures
                       let ordered_inputs: Vec<(TextureInputType, PathBuf)> = new_config.textures
                           .iter()
                           .map(|t| match t {
                               TextureInput::Image { path } => (TextureInputType::Image, path.clone()),
                               TextureInput::Video { path } => (TextureInputType::Video, path.clone()),
                           })
                           .collect();
                       let new_texture_sources = load_textures(&ordered_inputs);
                       
                       match WgpuPipeline::new(context.clone(), new_config.width, new_config.height, new_shaders, new_texture_sources) {
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
        frame_count += 1;
        let elapsed_fps = fps_last_time.elapsed();
        if elapsed_fps >= Duration::from_secs(1) {
            let fps = frame_count as f32 / elapsed_fps.as_secs_f32();
            info!("Virtual camera: {:.2} FPS", fps);
            frame_count = 0;
            fps_last_time = Instant::now();
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
