# Proteus

**Proteus** is a cross-platform webcam processor that applies GPU-accelerated shaders to your camera feed in real-time. It can output the processed video to a window or to a virtual camera device for use in apps like Zoom, Discord, and Teams.

## Features

- 🚀 **Real-time processing**: Uses `wgpu` for high-performance GPU shader execution
- 📷 **Virtual Camera Output**:
  - **Windows**: Outputs to OBS Virtual Camera (via shared memory)
  - **Linux**: Outputs to `v4l2loopback` device
  - **macOS**: Outputs to OBS Virtual Camera (via CMIOExtension)
- 🎨 **Custom Shaders**: Supports GLSL fragment shaders for effects (CRT, edge detection, etc.)
- 🦀 **Pure Rust**: Built for performance and safety

## Installation

### Prerequisites

1. **Rust**: Install from [rustup.rs](https://rustup.rs/)
2. **Platform Specifics**:

   **Windows**:
   - Install [OBS Studio](https://obsproject.com/) (Proteus uses the OBS Virtual Camera driver)

   **Linux**:
   - Install `v4l2loopback` kernel module:
     ```bash
     sudo apt install v4l2loopback-dkms  # Debian/Ubuntu
     # or
     sudo dnf install v4l2loopback       # Fedora
     # or
     sudo pacman -S v4l2loopback-dkms    # Arch Linux
     ```

   **macOS**:
   - Install [OBS Studio 30+](https://obsproject.com/)
   - Open OBS, click "Start Virtual Camera" (bottom right), then "Stop Virtual Camera"
   - Approve the System Extension in System Settings > Privacy & Security if prompted
   - You may need to restart your machine after approving the extension

3. **Runtime Dependency**:
   - **FFmpeg**: Must be installed and available in your system PATH (`ffmpeg` command).
     - **macOS**: `brew install ffmpeg`
     - **Linux**: `sudo apt install ffmpeg`
     - **Windows**: Download from [ffmpeg.org](https://ffmpeg.org/download.html) (Windows builds), extract, and add the `bin` folder to your System PATH.

## Usage

### Basic Usage (Window Output)

Run Proteus and display the camera feed in a window:

```bash
cargo run --release
```

### Applying Shaders

You can apply a custom GLSL fragment shader using the `--shader` or `-s` flag.

```bash
cargo run --release -- --shader shaders/crt.frag
```

### ML Segmentation

The **MediaPipe Selfie Segmentation** model is embedded directly in the binary at compile time.

#### Building from Source

Before compiling, download the ONNX model from HuggingFace:

```bash
mkdir -p models
curl -L -o models/mediapipe_selfie.onnx \
  "https://huggingface.co/onnx-community/mediapipe_selfie_segmentation_landscape/resolve/main/onnx/model.onnx"
```

Your directory structure should look like:
```
proteus/
├── models/
│   └── mediapipe_selfie.onnx
```

#### Usage

Segmentation is **automatically enabled** when using shaders that reference the mask texture (`t_mask`). Simply use a segmentation shader:

```bash
cargo run --release -- --shader shaders/background_blur.frag
```

### Chaining Shaders

You can chain multiple shaders together by specifying the `-s` flag multiple times. The output of one shader becomes the input of the next.

```bash
# Apply Plasma effect, then Ripple distortion
cargo run --release -- -s shaders/plasma.frag -s shaders/ripple.frag
```

#### Mask Propagation

Displacement effects effectively "warp" the segmentation mask along with the image. This ensures that subsequent effects (like background blur) applied after a displacement shader will use the correctly distorted mask, preventing visual artifacts where the blur doesn't match the displaced subject.

### Virtual Camera

#### Windows

1. Ensure OBS Studio is installed.
2. Run Proteus with the virtual camera output flag:
   ```powershell
   cargo run --release -- --output virtual-camera
   ```
3. Open your video app (Zoom, Discord, etc.) and select **"OBS Virtual Camera"**.

#### Linux

1. Load the kernel module (create a virtual device):
   ```bash
   sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="Proteus Camera" exclusive_caps=1
   ```
2. Run Proteus:
   ```bash
   cargo run --release -- --output virtual-camera
   ```
3. Open your video app and select **"Proteus Camera"**.

> **Note**: You may need write permissions for `/dev/video10`. If standard execution fails, try running with `sudo` or adding your user to the `video` group.

#### macOS

1. **Prerequisites**:
   - Install [OBS Studio 30+](https://obsproject.com/) (required for macOS 13 Ventura or later)
   - Open OBS, click "Start Virtual Camera", then "Stop Virtual Camera" (one-time setup)
   - Approve the System Extension in System Settings > Privacy & Security if prompted
   - Restart your machine if the virtual camera doesn't appear

2. Run Proteus with virtual camera output:
   ```bash
   cargo run --release -- --output virtual-camera
   ```

3. Open your video app (FaceTime, Zoom, etc.) and select **"OBS Virtual Camera"**.

### Video & Image Textures

You can provide video files (MP4, MKV, MOV) or images (PNG, JPG) as inputs for shaders. These are bound to texture slots (`t_image0`, `t_image1`, etc.) in the order they appear in the command line.

**Example**:
```bash
# Slot 0 = my_video.mp4, Slot 1 = overlay.png
cargo run --release -- --video my_video.mp4 --image overlay.png --shader shaders/mix_video.frag
```

- **Video Playback**: Videos are decoded using your system's `ffmpeg` CLI, ensuring broad format support without complex build dependencies.
- **Interleaved Order**: The order of `--video` and `--image` flags determines the slot index.
  ```bash
  cargo run -- --image bg.png --video v1.mp4 --video v2.mp4
  # t_image0 = bg.png
  # t_image1 = v1.mp4
  # t_image2 = v2.mp4
  ```

### YouTube Support

Proteus supports playing YouTube videos directly by providing the URL as a video input. 

**Requirements**: 
- `yt-dlp` must be installed and available in your system PATH.

**Example**:
```bash
cargo run --release -- --video "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
```
Proteus will automatically resolve the stream using `yt-dlp` and pipe it to `ffmpeg`.

### Twitch Support

Proteus supports playing Twitch streams directly by providing the URL as a video input.

**Requirements**:
- `streamlink` must be installed and available in your system PATH.
  - **macOS**: `brew install streamlink`
  - **Linux**: `pip install streamlink` or via your package manager
  - **Windows**: Download from [streamlink.github.io](https://streamlink.github.io/)

**Example**:
```bash
cargo run --release -- --video "https://www.twitch.tv/shroud"
```
Proteus will automatically resolve the stream using `streamlink` and pipe it to `ffmpeg`.

### Lua Canvas (Dynamic Textures)

Lua scripts can generate dynamic textures in real-time using GPU-accelerated 2D rendering. These are useful for procedural animations, particle effects, or interactive visualizations that get composited with your camera feed via shaders.

**Requirements**:
- A Lua script that follows the required module structure (see below)

**Configuration File Usage**:
```yaml
textures:
  - type: lua
    path: lua/bounce_circle.lua
```

#### Script Structure

Lua scripts must return a module table with a `new()` constructor. The canvas runtime calls these lifecycle methods:

| Method | Arguments | Description |
|--------|-----------|-------------|
| `M.new()` | — | **Required.** Constructor that returns a new instance |
| `M:init(w, h)` | width, height | Called once when the canvas is created |
| `M:update(dt)` | delta time (seconds) | Called every frame for state updates |
| `M:draw()` | — | Called every frame to render using the canvas API |
| `M:save_state()` | — | Optional. Returns a table to preserve state across hot reloads |
| `M:load_state(state)` | saved state table | Optional. Restores state after hot reload |

**Minimal Example** (`lua/bounce_circle.lua`):
```lua
local M = {}
M.__index = M

function M.new()
    return setmetatable({ x = 100, y = 100, vx = 200, vy = 150, t = 0 }, M)
end

function M:init(w, h)
    self.w, self.h = w, h
end

function M:update(dt)
    self.t = self.t + dt
    self.x = self.x + self.vx * dt
    self.y = self.y + self.vy * dt
    
    -- Bounce off walls
    if self.x < 50 or self.x > self.w - 50 then self.vx = -self.vx end
    if self.y < 50 or self.y > self.h - 50 then self.vy = -self.vy end
end

function M:draw()
    canvas.clear(20, 20, 30, 255)
    local r = math.floor(128 + 127 * math.sin(self.t * 2))
    canvas.fill_circle(self.x, self.y, 50, r, 100, 200, 255)
end

return M
```

#### Canvas Drawing API

The global `canvas` table provides these drawing functions. All colors are RGBA (0-255).

**Shape Drawing:**

| Function | Description |
|----------|-------------|
| `canvas.clear(r, g, b, a)` | Fill entire canvas with a solid color |
| `canvas.fill_rect(x, y, w, h, r, g, b, a)` | Draw a filled rectangle |
| `canvas.stroke_rect(x, y, w, h, r, g, b, a, stroke_width)` | Draw a rectangle outline |
| `canvas.fill_circle(cx, cy, radius, r, g, b, a)` | Draw a filled circle |
| `canvas.stroke_circle(cx, cy, radius, r, g, b, a, stroke_width)` | Draw a circle outline |
| `canvas.draw_line(x1, y1, x2, y2, r, g, b, a, stroke_width)` | Draw a line segment |
| `canvas.push_clip(x, y, w, h)` | Set a clipping rectangle (subsequent draws are masked) |
| `canvas.pop_clip()` | Clear the clipping rectangle |

**Text Rendering:**

| Function | Description |
|----------|-------------|
| `canvas.draw_text(x, y, text, size, r, g, b, a)` | Draw text at position with default system font |
| `canvas.draw_text_font(x, y, text, font_family, size, r, g, b, a)` | Draw text with specific font family |
| `canvas.measure_text(text, size)` | Returns `width, height` of text with default font |
| `canvas.measure_text_font(text, font_family, size)` | Returns `width, height` with specific font |
| `canvas.list_fonts()` | Returns array of available system font family names |

**Canvas Properties**:
- `canvas.width` — Canvas width in pixels
- `canvas.height` — Canvas height in pixels

#### Hot Reloading

Lua scripts are automatically watched for changes. When you save your script:

1. **State preservation**: If your script implements `save_state()`, its return value is captured
2. **Script reload**: The new code is loaded and a fresh instance is created via `new()`
3. **State restoration**: If the new script implements `load_state(state)`, the saved state is passed in

This enables live-coding workflows where you can tweak animations without restarting Proteus.

#### Using with Shaders

Lua canvases are bound to texture slots just like images and videos. Access them in shaders via `t_image0`, `t_image1`, etc.

```yaml
textures:
  - type: lua
    path: lua/particle_system.lua    # Bound to t_image0

shader:
  - shaders/background_image.frag   # Can sample t_image0
```

## Configuration Options

| Option | Description | Default |
|--------|-------------|---------|
| `-i, --input <ID>` | Camera device index (number) or name (string) | 0 |
| `-s, --shader <PATH>` | Path to GLSL fragment shader | None (Passthrough) |
| `--width <PIXELS>` | Frame width | 1920 |
| `--height <PIXELS>` | Frame height | 1080 |
| `--max-input-width <PIXELS>` | Maximum camera input width | Same as `--width` |
| `--max-input-height <PIXELS>` | Maximum camera input height | Same as `--height` |
| `--fps <FPS>` | Target frames per second | 30 |
| `--output <MODE>` | `window` or `virtual-camera` | window |
| `--image <PATH>` | Load image into next available texture slot | - |
| `--video <PATH>` | Load video into next available texture slot | - |
| `--lua <PATH>` | Load Lua script into next available texture slot | - |
| `--list-devices` | List available cameras | - |
| `--config <PATH>` | Load configuration from a YAML file | - |

### Configuration File

You can use a YAML configuration file instead of command line arguments for easier management of complex setups (multiple shaders, textures, etc.).

**`config.yaml` Example:**
```yaml
# Input camera device: Use index (0) or strict name ("FaceTime HD Camera")
input: "0"

# Resolution and framerate
width: 1920
height: 1080
fps: 30

# Maximum camera input resolution (optional, defaults to width/height)
# Useful to limit camera capture to a lower resolution for performance
max_input_width: 1280
max_input_height: 720

# Output mode: 'window' or 'virtual-camera'
output: window

# List of shaders to apply in order
shader:
  - shaders/background_image.frag
  - shaders/crt.frag

# List of texture inputs (images or videos)
# These map to t_image0, t_image1, etc. in extraction order
textures:
  # Load a local image
  - type: image
    path: assets/overlay.png
    
  # Load a local video
  - type: video
    path: assets/background_loop.mp4
    
  # Load a YouTube/Twitch stream
  - type: video
    path: https://www.youtube.com/watch?v=dQw4w9WgXcQ
```

Run with the config file:
```bash
cargo run --release -- --config config.yaml
```

**Hot Reloading**: The configuration file is watched for changes.
- **Shaders/Textures**: Hot-reloadable — updates instantly without restart.
- **Other settings** (input, width, height, max_input_width, max_input_height, fps, output): Require a restart (logged as a warning).

## License

MIT
