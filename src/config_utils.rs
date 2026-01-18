use crate::{Config, TextureInputType};
use proteus::capture::{AsyncCapture, CaptureConfig};
use proteus::shader::{ShaderSource, TextureSlot};
use proteus::video::VideoPlayer;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::fs;
use tracing::{info, error};

/// Manages configuration file watching and reloading.
pub struct ConfigWatcher {
    path: PathBuf,
    _watcher: RecommendedWatcher,
    rx: Receiver<std::result::Result<Event, notify::Error>>,
    current_config: Option<Config>,
}

impl ConfigWatcher {
    /// Create a new config watcher if a path is provided.
    pub fn new(path: Option<PathBuf>) -> Option<Self> {
        let path = path?;
        let (tx, rx) = channel();
        
        match RecommendedWatcher::new(tx, notify::Config::default()) {
            Ok(mut watcher) => {
                if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
                    tracing::warn!("Failed to watch config file {:?}: {}", path, e);
                    return None;
                }
                info!("Watching config file {:?} for changes", path);
                
                // Load initial config
                let current_config = match fs::read_to_string(&path) {
                    Ok(content) => serde_yaml::from_str::<Config>(&content).ok(),
                    Err(_) => None,
                };

                Some(Self {
                    path,
                    _watcher: watcher,
                    rx,
                    current_config,
                })
            }
            Err(e) => {
                tracing::warn!("Failed to create config watcher: {}", e);
                None
            }
        }
    }

    /// Check for changes and return (old_config, new_config) if changed.
    pub fn check_for_changes(&mut self) -> Option<(Option<Config>, Config)> {
        let mut needs_reload = false;
        while let Ok(res) = self.rx.try_recv() {
            if let Ok(event) = res {
                if matches!(event.kind, notify::EventKind::Modify(_) | notify::EventKind::Create(_)) {
                    needs_reload = true;
                }
            }
        }

        if needs_reload {
            info!("Config file changed, checking for updates...");
            match fs::read_to_string(&self.path) {
                Ok(content) => match serde_yaml::from_str::<Config>(&content) {
                    Ok(new_config) => {
                        let old = self.current_config.clone();
                        self.current_config = Some(new_config.clone());
                        return Some((old, new_config));
                    }
                    Err(e) => error!("Failed to parse new config: {}", e),
                },
                Err(e) => error!("Failed to read config file: {}", e),
            }
        }
        None
    }
}

/// Helper to load shaders from paths.
pub fn load_shaders(paths: &[PathBuf]) -> Vec<ShaderSource> {
    if paths.is_empty() {
        info!("Using passthrough shader");
        return Vec::new();
    }
    
    let mut shaders = Vec::new();
    for path in paths {
        info!("Loading shader from {:?}", path);
        match fs::read_to_string(path) {
            Ok(source) => shaders.push(ShaderSource::Glsl { code: source, path: Some(path.clone()) }),
            Err(e) => error!("Failed to read shader {:?}: {}", path, e),
        }
    }
    shaders
}

/// Helper to load texture sources from ordered inputs.
pub fn load_textures(ordered_inputs: &[(TextureInputType, PathBuf)]) -> Vec<TextureSlot> {
    let mut texture_sources = Vec::new();
    for (input_type, path) in ordered_inputs {
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
    texture_sources
}

/// Helper to load textures directly from Config textures.
pub fn load_textures_from_config(textures: &[crate::TextureInput]) -> Vec<TextureSlot> {
    let ordered_inputs = textures_to_ordered_inputs(textures);
    load_textures(&ordered_inputs)
}

/// Convert Config textures to ordered inputs format.
pub fn textures_to_ordered_inputs(textures: &[crate::TextureInput]) -> Vec<(TextureInputType, PathBuf)> {
    textures
        .iter()
        .map(|t| match t {
            crate::TextureInput::Image { path } => (TextureInputType::Image, path.clone()),
            crate::TextureInput::Video { path } => (TextureInputType::Video, path.clone()),
        })
        .collect()
}

/// Helper to initialize camera.
pub fn init_capture(config: CaptureConfig) -> Option<AsyncCapture> {
    match AsyncCapture::new(config) {
        Ok(capture) => Some(capture),
        Err(e) => {
             error!("Failed to initialize capture: {}", e);
             None
        }
    }
}
