//! Proteus: Cross-platform shader webcam transformer CLI.

use anyhow::Result;
use clap::Parser;
use proteus::capture::{CaptureBackend, CaptureConfig, NokhwaCapture};
use proteus::output::window_output::WindowRenderer;
use proteus::shader::{ShaderPipeline, ShaderSource, WgpuPipeline};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

/// Cross-platform shader webcam transformer.
#[derive(Parser, Debug)]
#[command(name = "proteus")]
#[command(about = "Apply GPU shaders to webcam video in real-time")]
struct Args {
    /// Camera device index
    #[arg(short, long, default_value = "0")]
    input: u32,

    /// Path to GLSL fragment shader file
    #[arg(short, long)]
    shader: Option<PathBuf>,

    /// Frame width
    #[arg(long, default_value = "1280")]
    width: u32,

    /// Frame height
    #[arg(long, default_value = "720")]
    height: u32,

    /// Target frames per second
    #[arg(long, default_value = "30")]
    fps: u32,

    /// List available cameras and exit
    #[arg(long)]
    list_devices: bool,
}

/// Application state for the event loop.
struct ProteusApp {
    args: Args,
    window: Option<Arc<Window>>,
    renderer: Option<WindowRenderer>,
    capture: Option<NokhwaCapture>,
    pipeline: Option<WgpuPipeline>,
    last_frame_time: Instant,
    frame_duration: Duration,
}

impl ProteusApp {
    fn new(args: Args) -> Self {
        let frame_duration = Duration::from_secs_f64(1.0 / args.fps as f64);
        Self {
            args,
            window: None,
            renderer: None,
            capture: None,
            pipeline: None,
            last_frame_time: Instant::now(),
            frame_duration,
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
        self.capture = Some(NokhwaCapture::open(config)?);
        info!("Camera opened successfully");

        // Load shader if provided
        let shader = if let Some(path) = &self.args.shader {
            info!("Loading shader from {:?}", path);
            let source = fs::read_to_string(path)?;
            Some(ShaderSource::Glsl(source))
        } else {
            info!("Using passthrough shader");
            None
        };

        // Initialize shader pipeline
        self.pipeline = Some(WgpuPipeline::new(self.args.width, self.args.height, shader)?);
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

        // Capture frame
        match capture.capture_frame() {
            Ok(frame) => {
                // Process through shader
                match pipeline.process_frame(&frame) {
                    Ok(processed) => {
                        // Display in window
                        renderer.set_frame(processed);
                        if let Err(e) = renderer.render() {
                            error!("Render error: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Shader processing error: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Capture error: {}", e);
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

                // Create renderer
                match WindowRenderer::new(window) {
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

    let args = Args::parse();

    // List devices mode
    if args.list_devices {
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

    info!("Starting Proteus...");

    // Create event loop and run
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = ProteusApp::new(args);
    event_loop.run_app(&mut app)?;

    Ok(())
}
