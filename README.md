# Proteus

**Proteus** is a cross-platform webcam processor that applies GPU-accelerated shaders to your camera feed in real-time. It can output the processed video to a window or to a virtual camera device for use in apps like Zoom, Discord, and Teams.

## Features

- 🚀 **Real-time processing**: Uses `wgpu` for high-performance GPU shader execution
- 📷 **Virtual Camera Output**:
  - **Windows**: Outputs to OBS Virtual Camera (via shared memory)
  - **Linux**: Outputs to `v4l2loopback` device
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

## Usage

### Basic Usage (Window Output)

Run Proteus and display the camera feed in a window:

```bash
cargo run --release
```

### Applying Shaders

Apply a custom GLSL fragment shader:

```bash
cargo run --release -- --shader shaders/crt.frag
```

### Virtual Camera Output

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

## Configuration Options

| Option | Description | Default |
|--------|-------------|---------|
| `-i, --input <INDEX>` | Camera device index | 0 |
| `-s, --shader <PATH>` | Path to GLSL fragment shader | None (Passthrough) |
| `--width <PIXELS>` | Frame width | 1280 |
| `--height <PIXELS>` | Frame height | 720 |
| `--fps <FPS>` | Target frames per second | 30 |
| `--output <MODE>` | `window` or `virtual-camera` | window |
| `--list-devices` | List available cameras | - |

## License

MIT
