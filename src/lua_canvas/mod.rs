//! Lua-based canvas for dynamic texture generation.
//!
//! Uses mlua for Lua scripting and tiny-skia for 2D CPU rendering.
//! The Lua script defines init, update, and draw methods which are called
//! each frame to generate RGBA pixel data.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use anyhow::{anyhow, Result};
use fontdb::{Database, ID};
use mlua::{Function, Lua, Table};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use tiny_skia::{Color, Mask, Paint, PathBuilder, Pixmap, Stroke, Transform};
use tracing::{debug, error, info, warn};

/// A Lua-driven canvas that renders to an RGBA buffer each frame.
pub struct LuaCanvas {
    path: PathBuf,
    pub width: u32,
    pub height: u32,
    pixmap: Arc<Mutex<Pixmap>>,
    clip_mask: Arc<Mutex<Option<Mask>>>,
    font_db: Arc<FontDatabase>,
    lua: Lua,
    instance: Option<mlua::RegistryKey>,
    last_time: f32,
    initialized: bool,
    // File watching
    _watcher: Option<RecommendedWatcher>,
    reload_rx: Option<Receiver<std::result::Result<Event, notify::Error>>>,
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
    pub fn new(path: impl AsRef<Path>, width: u32, height: u32) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        info!("Creating LuaCanvas from {:?} ({}x{})", path, width, height);

        let pixmap = Pixmap::new(width, height)
            .ok_or_else(|| anyhow!("Failed to create pixmap {}x{}", width, height))?;
        let pixmap = Arc::new(Mutex::new(pixmap));

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
        
        let mut canvas = Self {
            path,
            width,
            height,
            pixmap,
            clip_mask: Arc::new(Mutex::new(None)),
            font_db,
            lua,
            instance: None,
            last_time: 0.0,
            initialized: false,
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
        let pixmap = self.pixmap.clone();
        let width = self.width;
        let height = self.height;

        let canvas_table = self.lua.create_table()?;

        // canvas.clear(r, g, b, a)
        {
            let pixmap = pixmap.clone();
            let clear_fn = self.lua.create_function(move |_, (r, g, b, a): (u8, u8, u8, u8)| {
                let color = Color::from_rgba8(r, g, b, a);
                if let Ok(mut pm) = pixmap.lock() {
                    pm.fill(color);
                }
                Ok(())
            })?;
            canvas_table.set("clear", clear_fn)?;
        }

        // canvas.fill_rect(x, y, w, h, r, g, b, a)
        {
            let pixmap = pixmap.clone();
            let clip_mask = self.clip_mask.clone();
            let fill_rect_fn = self.lua.create_function(move |_, (x, y, w, h, r, g, b, a): (f32, f32, f32, f32, u8, u8, u8, u8)| {
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, a);
                paint.anti_alias = true;

                if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
                    if let Ok(mut pm) = pixmap.lock() {
                        let mask = clip_mask.lock().ok();
                        let mask_ref = mask.as_ref().and_then(|m| m.as_ref());
                        pm.fill_rect(rect, &paint, Transform::identity(), mask_ref);
                    }
                }
                Ok(())
            })?;
            canvas_table.set("fill_rect", fill_rect_fn)?;
        }

        // canvas.fill_circle(cx, cy, radius, r, g, b, a)
        {
            let pixmap = pixmap.clone();
            let clip_mask = self.clip_mask.clone();
            let fill_circle_fn = self.lua.create_function(move |_, (cx, cy, radius, r, g, b, a): (f32, f32, f32, u8, u8, u8, u8)| {
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, a);
                paint.anti_alias = true;

                let mut pb = PathBuilder::new();
                pb.push_circle(cx, cy, radius);
                if let Some(path) = pb.finish() {
                    if let Ok(mut pm) = pixmap.lock() {
                        let mask = clip_mask.lock().ok();
                        let mask_ref = mask.as_ref().and_then(|m| m.as_ref());
                        pm.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), mask_ref);
                    }
                }
                Ok(())
            })?;
            canvas_table.set("fill_circle", fill_circle_fn)?;
        }

        // canvas.stroke_rect(x, y, w, h, r, g, b, a, stroke_width)
        {
            let pixmap = pixmap.clone();
            let clip_mask = self.clip_mask.clone();
            let stroke_rect_fn = self.lua.create_function(move |_, (x, y, w, h, r, g, b, a, stroke_width): (f32, f32, f32, f32, u8, u8, u8, u8, f32)| {
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, a);
                paint.anti_alias = true;

                let stroke = Stroke { width: stroke_width, ..Default::default() };

                let mut pb = PathBuilder::new();
                if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
                    pb.push_rect(rect);
                }
                if let Some(path) = pb.finish() {
                    if let Ok(mut pm) = pixmap.lock() {
                        let mask = clip_mask.lock().ok();
                        let mask_ref = mask.as_ref().and_then(|m| m.as_ref());
                        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), mask_ref);
                    }
                }
                Ok(())
            })?;
            canvas_table.set("stroke_rect", stroke_rect_fn)?;
        }

        // canvas.stroke_circle(cx, cy, radius, r, g, b, a, stroke_width)
        {
            let pixmap = pixmap.clone();
            let clip_mask = self.clip_mask.clone();
            let stroke_circle_fn = self.lua.create_function(move |_, (cx, cy, radius, r, g, b, a, stroke_width): (f32, f32, f32, u8, u8, u8, u8, f32)| {
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, a);
                paint.anti_alias = true;

                let stroke = Stroke { width: stroke_width, ..Default::default() };

                let mut pb = PathBuilder::new();
                pb.push_circle(cx, cy, radius);
                if let Some(path) = pb.finish() {
                    if let Ok(mut pm) = pixmap.lock() {
                        let mask = clip_mask.lock().ok();
                        let mask_ref = mask.as_ref().and_then(|m| m.as_ref());
                        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), mask_ref);
                    }
                }
                Ok(())
            })?;
            canvas_table.set("stroke_circle", stroke_circle_fn)?;
        }

        // canvas.draw_line(x1, y1, x2, y2, r, g, b, a, stroke_width)
        {
            let pixmap = pixmap.clone();
            let clip_mask = self.clip_mask.clone();
            let draw_line_fn = self.lua.create_function(move |_, (x1, y1, x2, y2, r, g, b, a, stroke_width): (f32, f32, f32, f32, u8, u8, u8, u8, f32)| {
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, a);
                paint.anti_alias = true;

                let stroke = Stroke { width: stroke_width, ..Default::default() };

                let mut pb = PathBuilder::new();
                pb.move_to(x1, y1);
                pb.line_to(x2, y2);
                if let Some(path) = pb.finish() {
                    if let Ok(mut pm) = pixmap.lock() {
                        let mask = clip_mask.lock().ok();
                        let mask_ref = mask.as_ref().and_then(|m| m.as_ref());
                        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), mask_ref);
                    }
                }
                Ok(())
            })?;
            canvas_table.set("draw_line", draw_line_fn)?;
        }

        // canvas.push_clip(x, y, w, h) - Set a clipping rectangle
        {
            let clip_mask = self.clip_mask.clone();
            let cw = width;
            let ch = height;
            let push_clip_fn = self.lua.create_function(move |_, (x, y, w, h): (f32, f32, f32, f32)| {
                if let Ok(mut mask) = clip_mask.lock() {
                    if let Some(mut new_mask) = Mask::new(cw, ch) {
                        let mut pb = PathBuilder::new();
                        if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
                            pb.push_rect(rect);
                        }
                        if let Some(path) = pb.finish() {
                            new_mask.fill_path(&path, tiny_skia::FillRule::Winding, true, Transform::identity());
                        }
                        *mask = Some(new_mask);
                    }
                }
                Ok(())
            })?;
            canvas_table.set("push_clip", push_clip_fn)?;
        }

        // canvas.pop_clip() - Clear clipping rectangle
        {
            let clip_mask = self.clip_mask.clone();
            let pop_clip_fn = self.lua.create_function(move |_, ()| {
                if let Ok(mut mask) = clip_mask.lock() {
                    *mask = None;
                }
                Ok(())
            })?;
            canvas_table.set("pop_clip", pop_clip_fn)?;
        }

        // canvas.draw_text(x, y, text, size, r, g, b, a) - Draw text with default font
        {
            let pixmap = pixmap.clone();
            let clip_mask = self.clip_mask.clone();
            let font_db = self.font_db.clone();
            let draw_text_fn = self.lua.create_function(
                move |_, (x, y, text, size, r, g, b, a): (f32, f32, String, f32, u8, u8, u8, u8)| {
                    draw_text_impl(&pixmap, &clip_mask, &font_db, None, x, y, &text, size, r, g, b, a);
                    Ok(())
                },
            )?;
            canvas_table.set("draw_text", draw_text_fn)?;
        }

        // canvas.draw_text_font(x, y, text, font_family, size, r, g, b, a) - Draw text with specific font
        {
            let pixmap = pixmap.clone();
            let clip_mask = self.clip_mask.clone();
            let font_db = self.font_db.clone();
            let draw_text_font_fn = self.lua.create_function(
                move |_,
                      (x, y, text, font_family, size, r, g, b, a): (
                    f32,
                    f32,
                    String,
                    String,
                    f32,
                    u8,
                    u8,
                    u8,
                    u8,
                )| {
                    draw_text_impl(
                        &pixmap,
                        &clip_mask,
                        &font_db,
                        Some(&font_family),
                        x,
                        y,
                        &text,
                        size,
                        r,
                        g,
                        b,
                        a,
                    );
                    Ok(())
                },
            )?;
            canvas_table.set("draw_text_font", draw_text_font_fn)?;
        }

        // canvas.measure_text(text, size) - Returns width, height of text with default font
        {
            let font_db = self.font_db.clone();
            let measure_text_fn = self.lua.create_function(move |_, (text, size): (String, f32)| {
                let (w, h) = measure_text_impl(&font_db, None, &text, size);
                Ok((w, h))
            })?;
            canvas_table.set("measure_text", measure_text_fn)?;
        }

        // canvas.measure_text_font(text, font_family, size) - Returns width, height with specific font
        {
            let font_db = self.font_db.clone();
            let measure_text_font_fn =
                self.lua
                    .create_function(move |_, (text, font_family, size): (String, String, f32)| {
                        let (w, h) = measure_text_impl(&font_db, Some(&font_family), &text, size);
                        Ok((w, h))
                    })?;
            canvas_table.set("measure_text_font", measure_text_font_fn)?;
        }

        // canvas.list_fonts() - Returns array of available font family names
        {
            let font_db = self.font_db.clone();
            let list_fonts_fn = self.lua.create_function(move |_, ()| {
                let families = font_db.list_families();
                Ok(families)
            })?;
            canvas_table.set("list_fonts", list_fonts_fn)?;
        }

        // canvas.width, canvas.height
        canvas_table.set("width", width)?;
        canvas_table.set("height", height)?;

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

        // Return the pixel data
        let copy_start = Instant::now();
        let pm = self.pixmap.lock().ok()?;
        let data = pm.data().to_vec();
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
}

/// Helper function to render text onto a pixmap.
fn draw_text_impl(
    pixmap: &Arc<Mutex<Pixmap>>,
    clip_mask: &Arc<Mutex<Option<Mask>>>,
    font_db: &Arc<FontDatabase>,
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
        .and_then(|family| font_db.find_font(family))
        .or_else(|| font_db.default_font());

    let Some(font_id) = font_id else {
        warn!("No fonts available for text rendering");
        return;
    };

    let Some(font_data) = font_db.get_font_data(font_id) else {
        warn!("Failed to load font data");
        return;
    };

    let Ok(font) = FontRef::try_from_slice(&font_data) else {
        warn!("Failed to parse font data");
        return;
    };

    let scale = PxScale::from(size);
    let scaled_font = font.as_scaled(scale);

    let mut pm = match pixmap.lock() {
        Ok(pm) => pm,
        Err(_) => return,
    };

    let mask_guard = clip_mask.lock().ok();

    // Calculate baseline position (y is the top of the text, add ascent to get baseline)
    let ascent = scaled_font.ascent();
    let baseline_y = y + ascent;

    let mut cursor_x = x;

    for c in text.chars() {
        let glyph_id = scaled_font.glyph_id(c);
        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(cursor_x, baseline_y));

        // Advance cursor
        cursor_x += scaled_font.h_advance(glyph_id);

        // Get outlined glyph for rendering
        if let Some(outlined) = scaled_font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            let pm_width = pm.width() as i32;
            let pm_height = pm.height() as i32;

            // Render glyph to pixmap
            outlined.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;

                if px < 0 || py < 0 || px >= pm_width || py >= pm_height {
                    return;
                }

                // Check clip mask
                if let Some(ref guard) = mask_guard {
                    if let Some(ref mask) = **guard {
                        let mask_idx = (py as usize) * (pm_width as usize) + (px as usize);
                        if mask.data()[mask_idx] == 0 {
                            return;
                        }
                    }
                }

                // Alpha blend the glyph onto the pixmap
                let alpha = (coverage * a as f32) as u8;
                if alpha == 0 {
                    return;
                }

                // Calculate pixel index
                let idx = (py as usize) * (pm_width as usize) + (px as usize);
                let pixels = pm.pixels_mut();
                let pixel = &mut pixels[idx];

                // Premultiplied alpha blending
                let src_a = alpha as f32 / 255.0;
                let dst_a = pixel.alpha() as f32 / 255.0;
                let out_a = src_a + dst_a * (1.0 - src_a);

                let blend = |src: u8, dst: u8| -> u8 {
                    if out_a == 0.0 {
                        0
                    } else {
                        let src_f = src as f32 / 255.0;
                        let dst_f = dst as f32 / 255.0;
                        let out = (src_f * src_a + dst_f * dst_a * (1.0 - src_a)) / out_a;
                        (out * 255.0).clamp(0.0, 255.0) as u8
                    }
                };

                if let Some(new_pixel) = tiny_skia::PremultipliedColorU8::from_rgba(
                    blend(r, pixel.red()),
                    blend(g, pixel.green()),
                    blend(b, pixel.blue()),
                    (out_a * 255.0).clamp(0.0, 255.0) as u8,
                ) {
                    *pixel = new_pixel;
                }
            });
        }
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
