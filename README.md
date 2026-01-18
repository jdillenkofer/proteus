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
| `--video <PATH>` | Load video into next available texture slot| - |
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
