//! Shared GPU context for wgpu resources.

use std::sync::Arc;
use anyhow::{anyhow, Result};
use winit::window::Window;

/// Shared GPU resources used by multiple components.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
}

impl GpuContext {
    /// Initialize GPU context compatible with the given window surface.
    /// If window is None, initializes for headless/offscreen use.
    pub fn new(window: Option<&Arc<Window>>) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // If we have a window, we need a surface to ensure compatibility
        let surface = if let Some(window) = window {
             Some(instance.create_surface(window.clone())?)
        } else {
             None
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface.as_ref(),
            force_fallback_adapter: false,
        }))
        .map_err(|_| anyhow!("Failed to obtain GPU adapter"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Proteus Device"),
                required_features: wgpu::Features::empty(),
                required_limits: if surface.is_some() {
                    wgpu::Limits::default()
                } else {
                    wgpu::Limits::downlevel_defaults()
                },
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            },

        ))?;

        Ok(Self {
            device,
            queue,
            instance,
            adapter,
        })
    }
}
