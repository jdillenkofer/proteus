//! Lua-based canvas for dynamic texture generation.
//!
//! Uses mlua for Lua scripting and wgpu for GPU-based 2D rendering.
//! The Lua script defines init, update, and draw methods which are called
//! each frame to generate RGBA pixel data.

mod gpu_canvas;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use anyhow::{anyhow, Result};
use fontdb::{Database, ID};
use gpu_canvas::GpuCanvas;
use mlua::{Function, Lua, Table};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};

/// A Lua-driven canvas that renders to an RGBA buffer each frame.
pub struct LuaCanvas {
    path: PathBuf,
    pub width: u32,
    pub height: u32,
    gpu_canvas: Arc<Mutex<GpuCanvas>>,
    lua: Lua,
    instance: Option<mlua::RegistryKey>,
    last_time: f32,
    initialized: bool,
    view_dirty: bool,
    // API state for the high-performance batcher
    api_state: Arc<Mutex<GpuCanvasBatcherState>>,
    // File watching
    _watcher: Option<RecommendedWatcher>,
    reload_rx: Option<Receiver<std::result::Result<Event, notify::Error>>>,
}

/// Cached glyph entry in the atlas
struct GlyphCacheEntry {
    atlas_x: u32,
    atlas_y: u32,
    width: u32,
    height: u32,
    advance: f32,
    offset_x: f32,
    offset_y: f32,
}

/// Simple row-based atlas allocator
struct AtlasAllocator {
    current_x: u32,
    current_y: u32,
    row_height: u32,
    atlas_size: u32,
}

impl AtlasAllocator {
    fn new(atlas_size: u32) -> Self {
        Self {
            current_x: 0,
            current_y: 0,
            row_height: 0,
            atlas_size,
        }
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 {
            return Some((0, 0));
        }
        
        // Check if we need to start a new row
        if self.current_x + width > self.atlas_size {
            self.current_x = 0;
            self.current_y += self.row_height + 1; // +1 for padding
            self.row_height = 0;
        }
        
        // Check if we've run out of space
        if self.current_y + height > self.atlas_size {
            return None; // Atlas full
        }
        
        let x = self.current_x;
        let y = self.current_y;
        
        self.current_x += width + 1; // +1 for padding
        self.row_height = self.row_height.max(height);
        
        Some((x, y))
    }

    fn reset(&mut self) {
        self.current_x = 0;
        self.current_y = 0;
        self.row_height = 0;
    }
}

/// Shared state for the Lua API batcher
struct GpuCanvasBatcherState {
    width: u32,
    height: u32,
    commands: Vec<gpu_canvas::DrawCommand>,
    clip_active: bool,
    // Dependencies for immediate or complex draws
    gpu_canvas: Arc<Mutex<GpuCanvas>>,
    font_db: Arc<FontDatabase>,
    image_cache: Arc<Mutex<std::collections::HashMap<String, Arc<ImageData>>>>,
    // Glyph caching: key is (font_id, glyph_id, size_in_tenths)
    glyph_cache: std::collections::HashMap<(ID, u16, u32), GlyphCacheEntry>,
    atlas_allocator: AtlasAllocator,
}

/// Wrapper for Lua to call canvas methods efficiently



struct ImageData {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

/// Thread-safe font database with cached font data.
pub struct FontDatabase {
    db: Database,
    /// Cache of loaded font data (font ID -> font bytes)
    font_cache: Mutex<std::collections::HashMap<ID, Arc<Vec<u8>>>>,
}

impl FontDatabase {
    /// Create a new font database and load system fonts.
    pub fn new() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        info!("Loaded {} system fonts", db.len());
        Self {
            db,
            font_cache: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Find a font by family name, returning the font ID.
    pub fn find_font(&self, family: &str) -> Option<ID> {
        self.db
            .faces()
            .find(|f| {
                f.families
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(family))
            })
            .map(|f| f.id)
    }

    /// Get the default font ID (first available font).
    pub fn default_font(&self) -> Option<ID> {
        self.db.faces().next().map(|f| f.id)
    }

    /// Get cached font data for a font ID.
    pub fn get_font_data(&self, id: ID) -> Option<Arc<Vec<u8>>> {
        // Check cache first
        {
            let cache = self.font_cache.lock().ok()?;
            if let Some(data) = cache.get(&id) {
                return Some(data.clone());
            }
        }

        // Load and cache
        let data = self.db.face_source(id).and_then(|(source, _)| {
            match source {
                fontdb::Source::Binary(data) => Some(data.as_ref().as_ref().to_vec()),
                fontdb::Source::File(path) => std::fs::read(path).ok(),
                fontdb::Source::SharedFile(path, _) => std::fs::read(path).ok(),
            }
        })?;

        let data = Arc::new(data);
        if let Ok(mut cache) = self.font_cache.lock() {
            cache.insert(id, data.clone());
        }
        Some(data)
    }

    /// List all available font family names.
    pub fn list_families(&self) -> Vec<String> {
        let mut families: Vec<String> = self
            .db
            .faces()
            .flat_map(|f| f.families.iter().map(|(name, _)| name.clone()))
            .collect();
        families.sort();
        families.dedup();
        families
    }
}

/// Decoded frame from LuaCanvas (compatible with VideoPlayer pattern).
#[derive(Clone)]
pub struct LuaFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl LuaCanvas {
    /// Create a new LuaCanvas from a Lua script path.
    pub fn new(
        path: impl AsRef<Path>,
        width: u32,
        height: u32,
        device_queue: Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        info!("Creating LuaCanvas from {:?} ({}x{})", path, width, height);

        let gpu_canvas = if let Some((device, queue)) = device_queue {
            GpuCanvas::with_device_queue(device, queue, width, height)
        } else {
            GpuCanvas::new(width, height)
        };
        let gpu_canvas = Arc::new(Mutex::new(gpu_canvas));

        // Initialize font database with system fonts
        let font_db = Arc::new(FontDatabase::new());

        let lua = Lua::new();
        
        // Setup file watcher
        let (watcher, reload_rx) = {
            let (tx, rx) = channel();
            match RecommendedWatcher::new(tx, notify::Config::default()) {
                Ok(mut w) => {
                    if let Err(e) = w.watch(&path, RecursiveMode::NonRecursive) {
                        warn!("Failed to watch Lua script {:?}: {}", path, e);
                        (None, None)
                    } else {
                        info!("Watching Lua script {:?} for changes", path);
                        (Some(w), Some(rx))
                    }
                }
                Err(e) => {
                    warn!("Failed to create file watcher: {}", e);
                    (None, None)
                }
            }
        };
        
        let image_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));

        let mut canvas = Self {
            path,
            width,
            height,
            gpu_canvas: gpu_canvas.clone(),
            lua,
            instance: None,
            last_time: 0.0,
            initialized: false,
            view_dirty: true,
            api_state: Arc::new(Mutex::new(GpuCanvasBatcherState {
                width,
                height,
                commands: Vec::with_capacity(1024),
                clip_active: false,
                gpu_canvas,
                font_db,
                image_cache,
                glyph_cache: std::collections::HashMap::new(),
                atlas_allocator: AtlasAllocator::new(2048),
            })),
            _watcher: watcher,
            reload_rx,
        };

        canvas.load_script()?;
        Ok(canvas)
    }

    /// Load (or reload) the Lua script.
    fn load_script(&mut self) -> Result<()> {
        let code = std::fs::read_to_string(&self.path)
            .map_err(|e| anyhow!("Failed to read Lua script {:?}: {}", self.path, e))?;

        // Register canvas drawing functions
        self.register_canvas_api()?;
        
        // Expose script directory as a global
        let script_dir = self.path.parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        
        // Create a custom dofile function that loads files relative to script_dir
        let dofile_base = script_dir.clone();
        let dofile_fn = self.lua.create_function(move |lua, path: String| {
            // Resolve path relative to script directory
            let full_path = if std::path::Path::new(&path).is_absolute() {
                std::path::PathBuf::from(&path)
            } else {
                std::path::PathBuf::from(&dofile_base).join(&path)
            };
            
            let code = std::fs::read_to_string(&full_path)
                .map_err(|e| mlua::Error::external(format!("Failed to read {:?}: {}", full_path, e)))?;
            
            let result: mlua::Value = lua.load(&code).eval()
                .map_err(|e| mlua::Error::external(format!("Error in {:?}: {}", full_path, e)))?;
            
            Ok(result)
        })?;
        self.lua.globals().set("dofile", dofile_fn)?;

        // Load and execute the script to get the module table
        let module: Table = self.lua.load(&code).eval()
            .map_err(|e| anyhow!("Lua script error: {}", e))?;

        // Call M.new() to create an instance
        let new_fn: Function = module.get("new")
            .map_err(|e| anyhow!("Lua script must have a 'new' function: {}", e))?;
        
        let instance: Table = new_fn.call(())
            .map_err(|e| anyhow!("Error calling M.new(): {}", e))?;

        // Store instance in registry
        let key = self.lua.create_registry_value(instance)
            .map_err(|e| anyhow!("Failed to store Lua instance: {}", e))?;
        
        self.instance = Some(key);
        self.initialized = false;

        info!("Loaded Lua script {:?}", self.path);
        Ok(())
    }

    /// Register the canvas drawing API in Lua globals.
    fn register_canvas_api(&mut self) -> Result<()> {
        let state = self.api_state.clone();
        let lua = &self.lua;
        let canvas_table = lua.create_table()?;



        // canvas.clear(r, g, b, a)
        {
            let state = state.clone();
            let clear_fn = lua.create_function(move |_, (r, g, b, a): (u8, u8, u8, u8)| {
                let mut s = state.lock().unwrap();
                let (w, h) = (s.width as f32, s.height as f32);
                s.commands.clear();
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::PopClip,
                    uniforms: [0.0, 0.0, w, h, 0.0, 0.0, 0.0, 0.0, 0.0, w, h, 0.0, 0.0, 0.0, 0.0, 0.0],
                    clip_active: false,
                });
                s.clip_active = false;
                if let Ok(mut canvas) = s.gpu_canvas.lock() {
                    canvas.clear(r, g, b, a);
                }
                Ok(())
            })?;
            canvas_table.set("clear", clear_fn)?;
        }

        // canvas.fill_rect(x, y, w, h, r, g, b, a)
        {
            let state = state.clone();
            let fill_rect_fn = lua.create_function(move |_, (x, y, wr, hr, r, g, b, a): (f32, f32, f32, f32, u8, u8, u8, u8)| {
                let mut s = state.lock().unwrap();
                let (w, h) = (s.width as f32, s.height as f32);
                let clip = s.clip_active;
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::FillRect,
                    uniforms: [x, y, wr, hr, r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0, 0.0, w, h, 0.0, 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                Ok(())
            })?;
            canvas_table.set("fill_rect", fill_rect_fn)?;
        }

        // canvas.fill_circle(cx, cy, radius, r, g, b, a)
        {
            let state = state.clone();
            let fill_circle_fn = lua.create_function(move |_, (cx, cy, rad, r, g, b, a): (f32, f32, f32, u8, u8, u8, u8)| {
                let mut s = state.lock().unwrap();
                let (w, h) = (s.width as f32, s.height as f32);
                let clip = s.clip_active;
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::FillCircle,
                    uniforms: [cx, cy, rad, 0.0, r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0, 0.0, w, h, 0.0, 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                Ok(())
            })?;
            canvas_table.set("fill_circle", fill_circle_fn)?;
        }

        // ... repeat for others as needed ...
        // For brevity, I'll only add the ones used in rube_goldberg for now and then add the rest.
        // Actually, I'll add all of them to be safe.

        // canvas.stroke_rect(x, y, w, h, r, g, b, a, stroke)
        {
            let state = state.clone();
            let stroke_rect_fn = lua.create_function(move |_, (x, y, wr, hr, r, g, b, a, sw): (f32, f32, f32, f32, u8, u8, u8, u8, f32)| {
                let mut s = state.lock().unwrap();
                let (w, h) = (s.width as f32, s.height as f32);
                let clip = s.clip_active;
                let color = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0];
                let extra = [0.0, w, h, 0.0];
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::FillRect,
                    uniforms: [x, y, wr, sw, color[0], color[1], color[2], color[3], extra[0], extra[1], extra[2], extra[3], 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::FillRect,
                    uniforms: [x, y + hr - sw, wr, sw, color[0], color[1], color[2], color[3], extra[0], extra[1], extra[2], extra[3], 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::FillRect,
                    uniforms: [x, y, sw, hr, color[0], color[1], color[2], color[3], extra[0], extra[1], extra[2], extra[3], 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::FillRect,
                    uniforms: [x + wr - sw, y, sw, hr, color[0], color[1], color[2], color[3], extra[0], extra[1], extra[2], extra[3], 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                Ok(())
            })?;
            canvas_table.set("stroke_rect", stroke_rect_fn)?;
        }

        // canvas.stroke_circle(cx, cy, radius, r, g, b, a, stroke_width)
        {
            let state = state.clone();
            let stroke_circle_fn = lua.create_function(move |_, (cx, cy, rad, r, g, b, a, sw): (f32, f32, f32, u8, u8, u8, u8, f32)| {
                let mut s = state.lock().unwrap();
                let (w, h) = (s.width as f32, s.height as f32);
                let clip = s.clip_active;
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::StrokeCircle,
                    uniforms: [cx, cy, rad, 0.0, r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0, sw, w, h, 0.0, 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                Ok(())
            })?;
            canvas_table.set("stroke_circle", stroke_circle_fn)?;
        }

        // canvas.push_clip(x, y, w, h)
        {
            let state = state.clone();
            let push_clip_fn = lua.create_function(move |_, (x, y, wr, hr): (f32, f32, f32, f32)| {
                let mut s = state.lock().unwrap();
                let (w, h) = (s.width as f32, s.height as f32);
                let clip = s.clip_active;
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::PushClip,
                    uniforms: [x, y, wr, hr, 1.0, 1.0, 1.0, 1.0, 0.0, w, h, 0.0, 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                s.clip_active = true;
                Ok(())
            })?;
            canvas_table.set("push_clip", push_clip_fn)?;
        }

        // canvas.pop_clip()
        {
            let state = state.clone();
            let pop_clip_fn = lua.create_function(move |_, (): ()| {
                let mut s = state.lock().unwrap();
                let (w, h) = (s.width as f32, s.height as f32);
                let clip = s.clip_active;
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::PopClip,
                    uniforms: [0.0, 0.0, w, h, 0.0, 0.0, 0.0, 0.0, 0.0, w, h, 0.0, 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                s.clip_active = false;
                Ok(())
            })?;
            canvas_table.set("pop_clip", pop_clip_fn)?;
        }

        // canvas.draw_text(x, y, text, size, r, g, b, a)
        {
            let state = state.clone();
            let draw_text_fn = lua.create_function(move |_, (x, y, text, size, r, g, b, a): (f32, f32, String, f32, u8, u8, u8, u8)| {
                let mut s = state.lock().unwrap();
                draw_text_impl(&mut s, None, x, y, &text, size, r, g, b, a);
                Ok(())
            })?;
            canvas_table.set("draw_text", draw_text_fn)?;
        }
        
        // canvas.draw_line(x1, y1, x2, y2, r, g, b, a, sw)
        {
            let state = state.clone();
            let draw_line_fn = lua.create_function(move |_, (x1, y1, x2, y2, r, g, b, a, sw): (f32, f32, f32, f32, u8, u8, u8, u8, f32)| {
                let mut s = state.lock().unwrap();
                let (w, h) = (s.width as f32, s.height as f32);
                let clip = s.clip_active;
                s.commands.push(gpu_canvas::DrawCommand {
                    cmd_type: gpu_canvas::DrawCommandType::Line,
                    uniforms: [x1, y1, x2, y2, r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0, sw, w, h, 0.0, 0.0, 0.0, 0.0, 0.0],
                    clip_active: clip,
                });
                Ok(())
            })?;
            canvas_table.set("draw_line", draw_line_fn)?;
        }

        // canvas.draw_image(path, x, y, [w, h])
        {
            let state = state.clone();
            let draw_image_fn = lua.create_function(move |_, (path, x, y, img_w, img_h): (String, f32, f32, Option<f32>, Option<f32>)| {
                let mut s = state.lock().unwrap();
                // Ensure all batched commands are pushed to GPU before immediate draw
                if !s.commands.is_empty() {
                    let commands = std::mem::take(&mut s.commands);
                    if let Ok(mut canvas) = s.gpu_canvas.lock() {
                        canvas.add_commands(commands);
                    }
                }
                draw_image_impl(&s.gpu_canvas, &s.image_cache, &path, x, y, img_w, img_h);
                Ok(())
            })?;
            canvas_table.set("draw_image", draw_image_fn)?;
        }

        // canvas.draw_text_font(x, y, text, font, size, r, g, b, a)
        {
            let state = state.clone();
            let draw_text_font_fn = lua.create_function(move |_, (x, y, text, font, size, r, g, b, a): (f32, f32, String, String, f32, u8, u8, u8, u8)| {
                let mut s = state.lock().unwrap();
                draw_text_impl(&mut s, Some(&font), x, y, &text, size, r, g, b, a);
                Ok(())
            })?;
            canvas_table.set("draw_text_font", draw_text_font_fn)?;
        }

        // canvas.measure_text(text, size)
        {
            let state = state.clone();
            let measure_text_fn = lua.create_function(move |_, (text, size): (String, f32)| {
                let s = state.lock().unwrap();
                let (w, h) = measure_text_impl(&s.font_db, None, &text, size);
                Ok((w, h))
            })?;
            canvas_table.set("measure_text", measure_text_fn)?;
        }

        // canvas.measure_text_font(text, font, size)
        {
            let state = state.clone();
            let measure_text_font_fn = lua.create_function(move |_, (text, font, size): (String, String, f32)| {
                let s = state.lock().unwrap();
                let (w, h) = measure_text_impl(&s.font_db, Some(&font), &text, size);
                Ok((w, h))
            })?;
            canvas_table.set("measure_text_font", measure_text_font_fn)?;
        }

        // canvas.list_fonts()
        {
            let state = state.clone();
            let list_fonts_fn = lua.create_function(move |_, (): ()| {
                let s = state.lock().unwrap();
                Ok(s.font_db.list_families())
            })?;
            canvas_table.set("list_fonts", list_fonts_fn)?;
        }

        // width, height
        canvas_table.set("width", self.width)?;
        canvas_table.set("height", self.height)?;

        self.lua.globals().set("canvas", canvas_table)?;
        Ok(())
    }

    /// Check for file changes using notify watcher and reload if necessary.
    fn check_reload(&mut self) {
        let Some(rx) = &self.reload_rx else { return; };
        
        let mut needs_reload = false;
        // Drain channel to clear backlog and debounce
        while let Ok(res) = rx.try_recv() {
            match res {
                Ok(event) => {
                    if matches!(event.kind, notify::EventKind::Modify(_) | notify::EventKind::Create(_)) {
                        needs_reload = true;
                        info!("Lua script modified: {:?}", event.paths);
                    }
                }
                Err(e) => warn!("Watch error: {}", e),
            }
        }

        if needs_reload {
            info!("Reloading Lua script...");
            
            let saved_state = self.try_save_state();
            
            if let Err(e) = self.load_script() {
                error!("Failed to reload Lua script: {}", e);
            } else if let Some(state) = saved_state {
                self.try_load_state(state);
            }
        }
    }

    /// Try to call save_state() on the current instance if it exists.
    /// Returns the saved state as a Lua value, or None if not available.
    fn try_save_state(&self) -> Option<mlua::RegistryKey> {
        let instance_key = self.instance.as_ref()?;
        let instance: Table = self.lua.registry_value(instance_key).ok()?;
        
        // Check if save_state method exists
        let save_fn: Function = instance.get("save_state").ok()?;
        
        // Call save_state(self) and get the result
        match save_fn.call::<mlua::Value>(&instance) {
            Ok(state) => {
                // Store state in registry so it survives script reload
                match self.lua.create_registry_value(state) {
                    Ok(key) => {
                        info!("Saved Lua script state");
                        Some(key)
                    }
                    Err(e) => {
                        warn!("Failed to store saved state: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("Lua save_state() error: {}", e);
                None
            }
        }
    }

    /// Try to call load_state(state) on the new instance if it exists.
    /// If successful, marks the instance as initialized to skip init() call.
    fn try_load_state(&mut self, state_key: mlua::RegistryKey) {
        let Some(instance_key) = &self.instance else { return; };
        let Ok(instance) = self.lua.registry_value::<Table>(instance_key) else { return; };
        
        // Check if load_state method exists
        let Ok(load_fn) = instance.get::<Function>("load_state") else { return; };
        
        // Get the saved state from registry
        let Ok(state) = self.lua.registry_value::<mlua::Value>(&state_key) else {
            warn!("Failed to retrieve saved state");
            return;
        };
        
        // Call load_state(self, state)
        if let Err(e) = load_fn.call::<()>((&instance, state)) {
            warn!("Lua load_state() error: {}", e);
        } else {
            info!("Restored Lua script state");
            // Skip init() since state was restored - it would override the restored values
            self.initialized = true;
        }
    }

    /// Get the current frame for the given time.
    /// Returns RGBA pixel data.
    pub fn get_frame(&mut self, time: f32) -> Option<LuaFrame> {
        use std::time::Instant;
        
        let frame_start = Instant::now();
        
        // Check for hot reload
        self.check_reload();

        let instance_key = self.instance.as_ref()?;
        let instance: Table = self.lua.registry_value(instance_key).ok()?;

        // Call init once
        if !self.initialized {
            if let Ok(init_fn) = instance.get::<Function>("init") {
                if let Err(e) = init_fn.call::<()>((&instance, self.width, self.height)) {
                    warn!("Lua init() error: {}", e);
                }
            }
            self.initialized = true;
            self.last_time = time;
        }

        // Calculate delta time
        let dt = time - self.last_time;
        self.last_time = time;

        // Call update(dt)
        let update_start = Instant::now();
        if let Ok(update_fn) = instance.get::<Function>("update") {
            if let Err(e) = update_fn.call::<()>((&instance, dt)) {
                warn!("Lua update() error: {}", e);
            }
        }
        let update_time = update_start.elapsed();

        // Call draw()
        let draw_start = Instant::now();
        if let Ok(draw_fn) = instance.get::<Function>("draw") {
            if let Err(e) = draw_fn.call::<()>(&instance) {
                warn!("Lua draw() error: {}", e);
            }
        }
        let draw_time = draw_start.elapsed();

        // Read pixels from GPU
        let copy_start = Instant::now();
        let data = {
            let mut canvas = self.gpu_canvas.lock().ok()?;
            canvas.read_pixels()
        };
        let copy_time = copy_start.elapsed();
        
        let total_time = frame_start.elapsed();
        
        debug!(
            "[Perf] LuaCanvas - update: {:?}, draw: {:?}, copy: {:?}, total: {:?}",
            update_time,
            draw_time,
            copy_time,
            total_time,
        );
        
        Some(LuaFrame {
            data,
            width: self.width,
            height: self.height,
        })
    }

    /// Prepare the canvas texture for direct GPU access (no CPU readback).
    /// Runs update/draw Lua methods and returns a texture view.
    pub fn prepare_texture(&mut self, time: f32) -> Option<wgpu::TextureView> {
        use std::time::Instant;
        
        let frame_start = Instant::now();
        
        // Check for hot reload
        self.check_reload();

        let instance_key = self.instance.as_ref()?;
        let instance: Table = self.lua.registry_value(instance_key).ok()?;

        // Call init once
        if !self.initialized {
            if let Ok(init_fn) = instance.get::<Function>("init") {
                if let Err(e) = init_fn.call::<()>((&instance, self.width, self.height)) {
                    warn!("Lua init() error: {}", e);
                }
            }
            self.initialized = true;
            self.last_time = time;
        }

        // Calculate delta time
        let dt = time - self.last_time;
        self.last_time = time;

        // Call update(dt)
        let update_start = Instant::now();
        if let Ok(update_fn) = instance.get::<Function>("update") {
            if let Err(e) = update_fn.call::<()>((&instance, dt)) {
                warn!("Lua update() error: {}", e);
            }
        }
        let update_time = update_start.elapsed();

        // Call draw()
        let draw_start = Instant::now();
        if let Ok(draw_fn) = instance.get::<Function>("draw") {
            if let Err(e) = draw_fn.call::<()>(&instance) {
                warn!("Lua draw() error: {}", e);
            }
        }
        let draw_time = draw_start.elapsed();

        // Flush draws and get texture view (no CPU readback!)
        let flush_start = Instant::now();
        let (texture_view, is_dirty) = {
            let mut canvas = self.gpu_canvas.lock().ok()?;
            // Sync commands from our local batcher
            {
                let mut state = self.api_state.lock().unwrap();
                if !state.commands.is_empty() {
                    canvas.add_commands(std::mem::take(&mut state.commands));
                }
            }
            canvas.prepare_texture();
            let view = canvas.texture_view().clone();
            let dirty = self.view_dirty;
            self.view_dirty = false;
            (view, dirty)
        };
        let flush_time = flush_start.elapsed();
        
        let total_time = frame_start.elapsed();
        
        debug!(
            "[Perf] LuaCanvas (GPU) - update: {:?}, draw: {:?}, flush: {:?}, total: {:?}",
            update_time,
            draw_time,
            flush_time,
            total_time,
        );
        
        if is_dirty {
            Some(texture_view)
        } else {
            None
        }
    }
}

/// Helper function to render text onto the GPU canvas with glyph caching.
fn draw_text_impl(
    state: &mut GpuCanvasBatcherState,
    font_family: Option<&str>,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    // Find font
    let font_id = font_family
        .and_then(|family| state.font_db.find_font(family))
        .or_else(|| state.font_db.default_font());

    let Some(font_id) = font_id else {
        warn!("No fonts available for text rendering");
        return;
    };

    let Some(font_data) = state.font_db.get_font_data(font_id) else {
        warn!("Failed to load font data");
        return;
    };

    let Ok(font) = FontRef::try_from_slice(&font_data) else {
        warn!("Failed to parse font data");
        return;
    };

    let scale = PxScale::from(size);
    let scaled_font = font.as_scaled(scale);
    let size_key = (size * 10.0) as u32; // Tenths of a pixel for stable caching

    // Calculate baseline position
    let ascent = scaled_font.ascent();
    let baseline_y = y + ascent;
    let mut cursor_x = x;

    // We need to keep the canvas lock during the entire loop to batch commands correctly
    let Ok(mut canvas) = state.gpu_canvas.lock() else {
        return;
    };

    // If we have existing non-glyph commands, flush them first to maintain order
    // (though usually text is drawn on top or separately)
    if !state.commands.is_empty() {
        let commands = std::mem::take(&mut state.commands);
        canvas.add_commands(commands);
    }

    for c in text.chars() {
        let glyph_id = scaled_font.glyph_id(c);
        let key = (font_id, glyph_id.0, size_key);

        let entry = if let Some(entry) = state.glyph_cache.get(&key) {
            entry
        } else {
            // Not in cache, rasterize and upload
            let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
            if let Some(outlined) = scaled_font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let width = bounds.width() as u32;
                let height = bounds.height() as u32;

                if width > 0 && height > 0 {
                    let mut pixels = vec![0u8; (width * height) as usize];
                    outlined.draw(|gx, gy, coverage| {
                        if gx < width && gy < height {
                            pixels[(gy * width + gx) as usize] = (coverage * 255.0) as u8;
                        }
                    });

                    // Allocate atlas space
                    if let Some((ax, ay)) = state.atlas_allocator.allocate(width, height) {
                        // Upload to GPU atlas
                        canvas.upload_glyph_to_atlas(ax, ay, width, height, &pixels);
                        
                        let new_entry = GlyphCacheEntry {
                            atlas_x: ax,
                            atlas_y: ay,
                            width,
                            height,
                            advance: scaled_font.h_advance(glyph_id),
                            offset_x: bounds.min.x,
                            offset_y: bounds.min.y,
                        };
                        state.glyph_cache.insert(key, new_entry);
                        state.glyph_cache.get(&key).unwrap()
                    } else {
                        // Atlas full - reset and try again (simple strategy)
                        state.atlas_allocator.reset();
                        state.glyph_cache.clear();
                        // Recursive retry once
                        if let Some((ax, ay)) = state.atlas_allocator.allocate(width, height) {
                            canvas.upload_glyph_to_atlas(ax, ay, width, height, &pixels);
                            let new_entry = GlyphCacheEntry {
                                atlas_x: ax,
                                atlas_y: ay,
                                width,
                                height,
                                advance: scaled_font.h_advance(glyph_id),
                                offset_x: bounds.min.x,
                                offset_y: bounds.min.y,
                            };
                            state.glyph_cache.insert(key, new_entry);
                            state.glyph_cache.get(&key).unwrap()
                        } else {
                            continue; // Still fails? Skip.
                        }
                    }
                } else {
                    // Empty glyph (like space), just advance
                    cursor_x += scaled_font.h_advance(glyph_id);
                    continue;
                }
            } else {
                cursor_x += scaled_font.h_advance(glyph_id);
                continue;
            }
        };

        // Add draw command to canvas (batched)
        canvas.queue_glyph(
            cursor_x + entry.offset_x,
            baseline_y + entry.offset_y,
            entry.width as f32,
            entry.height as f32,
            entry.atlas_x as f32,
            entry.atlas_y as f32,
            entry.width as f32,
            entry.height as f32,
            r, g, b, a
        );

        cursor_x += entry.advance;
    }
}

/// Helper function to measure text dimensions.
fn measure_text_impl(font_db: &Arc<FontDatabase>, font_family: Option<&str>, text: &str, size: f32) -> (f32, f32) {
    // Find font
    let font_id = font_family
        .and_then(|family| font_db.find_font(family))
        .or_else(|| font_db.default_font());

    let Some(font_id) = font_id else {
        return (0.0, 0.0);
    };

    let Some(font_data) = font_db.get_font_data(font_id) else {
        return (0.0, 0.0);
    };

    let Ok(font) = FontRef::try_from_slice(&font_data) else {
        return (0.0, 0.0);
    };

    let scale = PxScale::from(size);
    let scaled_font = font.as_scaled(scale);

    let mut width = 0.0f32;
    for c in text.chars() {
        let glyph_id = scaled_font.glyph_id(c);
        width += scaled_font.h_advance(glyph_id);
    }

    let height = scaled_font.ascent() - scaled_font.descent();

    (width, height)
}

fn draw_image_impl(
    gpu_canvas: &Arc<Mutex<GpuCanvas>>,
    image_cache: &Arc<Mutex<std::collections::HashMap<String, Arc<ImageData>>>>,
    path: &str,
    x: f32,
    y: f32,
    w: Option<f32>,
    h: Option<f32>,
) {
    let img_data = {
        let Ok(mut cache) = image_cache.lock() else { return; };
        if let Some(data) = cache.get(path) {
            data.clone()
        } else {
            // Load image using local path
            match image::open(path) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let data = Arc::new(ImageData {
                        width: rgba.width(),
                        height: rgba.height(),
                        data: rgba.into_raw(),
                    });
                    cache.insert(path.to_string(), data.clone());
                    data
                }
                Err(e) => {
                    warn!("Failed to load image from {}: {}", path, e);
                    return;
                }
            }
        }
    };

    if let Ok(mut canvas) = gpu_canvas.lock() {
        // Important: Flush batch before writing image pixels
        canvas.flush();
        
        let target_w = w.unwrap_or(img_data.width as f32) as u32;
        let target_h = h.unwrap_or(img_data.height as f32) as u32;
        
        // Fast path for 1:1 image writes. Resize is not yet supported in this direct path.
        canvas.draw_image(x as i32, y as i32, target_w, target_h, &img_data.data);
    }
}
