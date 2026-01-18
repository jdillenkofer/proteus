//! Video decoding module for dynamic texture playback.
//! Uses the `ffmpeg` command-line tool via a subprocess to decode video frames.

use anyhow::{anyhow, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{error, info, warn};
use url::Url;

/// Supported streaming platforms
enum StreamingPlatform {
    YouTube,
    Twitch,
}

/// Check if a URL belongs to a known streaming platform by parsing the domain.
fn detect_streaming_platform(input: &str) -> Option<StreamingPlatform> {
    let parsed = Url::parse(input).ok()?;
    let host = parsed.host_str()?;
    
    // Strip "www." prefix if present for comparison
    let domain = host.strip_prefix("www.").unwrap_or(host);
    
    match domain {
        "youtube.com" | "youtu.be" => Some(StreamingPlatform::YouTube),
        "twitch.tv" => Some(StreamingPlatform::Twitch),
        _ => None,
    }
}

/// A video player that decodes frames using a background ffmpeg process.
pub struct VideoPlayer {
    /// Receiver for decoded RGBA frames
    frame_rx: Receiver<DecodedFrame>,
    /// Current frame cached for display
    current_frame: Option<DecodedFrame>,
    /// Next frame buffered (waiting for timestamp)
    next_frame: Option<DecodedFrame>,
    /// Video dimensions
    pub width: u32,
    pub height: u32,
    /// Video duration in seconds
    pub duration: f32,
    /// Playback start time (set when first frame is requested)
    start_time: Option<f32>,
    /// Decode thread handle
    _thread: JoinHandle<()>,
    /// Signal to stop the thread
    _stop_signal: Arc<Mutex<bool>>,
    /// Frame rate (fps) - needed to timestamp frames roughly if timestamps aren't piped
    _fps: f32,
}

/// A decoded video frame with RGBA data.
#[derive(Clone)]
pub struct DecodedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp: f32,
}

impl VideoPlayer {
    /// Opens a video file and starts decoding in a background thread.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        info!("Opening video via ffmpeg CLI: {:?}", path);

        // 0. Check if input is a streaming URL and resolve it
        let path_str = path.to_string_lossy();
        let resolved_path = match detect_streaming_platform(&path_str) {
            Some(StreamingPlatform::YouTube) => {
                info!("Detected YouTube URL, resolving stream via yt-dlp...");
                let output = Command::new("yt-dlp")
                    .args(&["-g", "-f", "bestvideo[height<=1080][vcodec^=avc1]/bestvideo[height<=1080]/best", &path_str])
                    .output()
                    .map_err(|e| anyhow!("Failed to run yt-dlp: {}", e))?;

                if !output.status.success() {
                    return Err(anyhow!("yt-dlp failed: {}", String::from_utf8_lossy(&output.stderr)));
                }
                
                let url = String::from_utf8(output.stdout)?
                    .lines()
                    .next()
                    .ok_or_else(|| anyhow!("yt-dlp returned no URL"))?
                    .to_string();
                    
                info!("Resolved YouTube stream");
                std::path::PathBuf::from(url)
            }
            Some(StreamingPlatform::Twitch) => {
                info!("Detected Twitch URL, resolving stream via streamlink...");
                let output = Command::new("streamlink")
                    .args(&["--stream-url", &path_str, "best"])
                    .output()
                    .map_err(|e| anyhow!("Failed to run streamlink: {}", e))?;

                if !output.status.success() {
                    return Err(anyhow!("streamlink failed: {}", String::from_utf8_lossy(&output.stderr)));
                }
                
                let url = String::from_utf8(output.stdout)?
                    .lines()
                    .next()
                    .ok_or_else(|| anyhow!("streamlink returned no URL"))?
                    .trim()
                    .to_string();
                    
                info!("Resolved Twitch stream");
                std::path::PathBuf::from(url)
            }
            None => path,
        };
        
        // 1. Get metadata via ffprobe
        // ffprobe -v error -select_streams v:0 -show_entries stream=width,height,duration,r_frame_rate -of csv=p=0 <file>
        let output = Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-select_streams", "v:0", // Select first video stream
                "-show_entries", "stream=width,height,duration,r_frame_rate",
                "-of", "csv=p=0",
                resolved_path.to_str().unwrap()
            ])
            .output()
            .map_err(|e| anyhow!("Failed to run ffprobe: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        let stdout = String::from_utf8(output.stdout)?;
        let parts: Vec<&str> = stdout.trim().split(',').collect();
        if parts.len() < 3 { // duration might be missing or N/A
             return Err(anyhow!("Invalid ffprobe output: {}", stdout));
        }

        let width: u32 = parts[0].parse()?;
        let height: u32 = parts[1].parse()?;
        
        // Parse duration (sometimes "N/A" for streams)
        // If 4 parts, it's w,h,r_frame_rate,duration (order depends on ffprobe version/flags sometimes? No, -show_entries respects order... usually)
        // Actually, let's just parse intuitively.
        // Wait, order corresponds to -show_entries. width,height,duration,r_frame_rate.
        
        // Let's protect against N/A
        let mut duration = 0.0;
        let mut fps = 30.0;

        // Try to parse parts based on index
        if parts.len() >= 4 {
             // width, height, r_frame_rate, duration (wait, -show_entries order is NOT guaranteed to match output CSV order in older versions, but usually does)
             // safe bet: width/height are first (integers). fps has / usually. duration is float.
             // Actually, typically the order is exactly as requested.
             
             // Parsed manually:
             // parts[0] -> width
             // parts[1] -> height
             // parts[2] -> duration or fps?
             
             // Let's use json output for safety? CSV is brittle if fields are missing.
             // Re-run with json to be safe? Or simple parsing.
             // For now assume standard order: width,height,duration,r_frame_rate
             
             // Note: ffprobe output might put duration before r_frame_rate or vice versa?
             // Let's look at parts[2]. If contains '/', it's fps (24/1 or 30000/1001).
             // If parts[3] contains '/', it's fps.
             
             let p2 = parts[2];
             let p3 = parts[3];
             
             if let Ok(d) = p2.parse::<f32>() {
                 duration = d;
                 fps = parse_fps(p3);
             } else if let Ok(d) = p3.parse::<f32>() {
                 duration = d;
                 fps = parse_fps(p2);
             } else {
                 // Try parsing fps from p2
                 fps = parse_fps(p2);
                 // p3 might be N/A
             }
        }
        
        info!("Video: {}x{}, {:.1}s, {:.1} fps", width, height, duration, fps);

        // Bounded channel to prevent memory explosion if decode is faster than playback
        let (frame_tx, frame_rx) = mpsc::sync_channel(5); 

        let stop_signal = Arc::new(Mutex::new(false));
        let stop_signal_clone = stop_signal.clone();
        
        let path_clone = resolved_path.clone();
        let thread = thread::spawn(move || {
            Self::decode_loop(path_clone, width, height, fps, frame_tx, stop_signal_clone);
        });

        Ok(Self {
            frame_rx,
            current_frame: None,
            next_frame: None,
            width,
             height,
             duration,
             start_time: None,
             _thread: thread,
             _stop_signal: stop_signal,
             _fps: fps,
        })
    }

    /// Background decode loop.
    fn decode_loop(path: std::path::PathBuf, width: u32, height: u32, fps: f32, tx: mpsc::SyncSender<DecodedFrame>, stop_signal: Arc<Mutex<bool>>) {
        let frame_size = (width * height * 4) as usize;
        let frame_duration = if fps > 0.0 { 1.0 / fps } else { 1.0 / 30.0 };
        
        loop {
            // Check stop signal
            if *stop_signal.lock().unwrap() { return; }

            info!("Starting ffmpeg process");
            
            // Build ffmpeg arguments - add network buffering for streaming sources
            let path_str = path.to_str().unwrap();
            let is_network_source = path_str.starts_with("http://") || path_str.starts_with("https://");
            
            let mut args: Vec<&str> = Vec::new();
            
            // Add network buffering options for streaming sources
            if is_network_source {
                args.extend_from_slice(&[
                    "-reconnect", "1",
                    "-reconnect_streamed", "1", 
                    "-reconnect_delay_max", "5",
                    "-thread_queue_size", "512",
                ]);
            }
            
            // Input and output format
            args.extend_from_slice(&[
                "-i", path_str,
                "-f", "image2pipe",
                "-pix_fmt", "rgba",
                "-vcodec", "rawvideo",
                "-"
            ]);
            
            // ffmpeg -i <file> -f image2pipe -pix_fmt rgba -vcodec rawvideo -
            let mut child = match Command::new("ffmpeg")
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()) // Enable stderr to see ffmpeg errors
                .spawn() 
            {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to spawn ffmpeg: {}", e);
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };

            // Spawn a thread to log ffmpeg stderr
            let mut stderr = child.stderr.take().unwrap();
            thread::spawn(move || {
                let mut buf = [0u8; 1024];
                loop {
                    match stderr.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let msg = String::from_utf8_lossy(&buf[..n]);
                            for line in msg.lines() {
                                if line.contains("Error") || line.contains("error") || line.contains("failed") {
                                    error!("ffmpeg: {}", line);
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            let mut stdout = child.stdout.take().unwrap();
            let mut buffer = vec![0u8; frame_size];
            let mut frame_count = 0;

            loop {
                // Check stop signal
                if *stop_signal.lock().unwrap() {
                    let _ = child.kill();
                    return;
                }

                // Read exact frame size
                if let Err(e) = stdout.read_exact(&mut buffer) {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        // End of stream
                        break;
                    }
                    warn!("Error reading from ffmpeg: {}", e);
                    break;
                }

                let timestamp = frame_count as f32 * frame_duration;
                frame_count += 1;

                let decoded = DecodedFrame {
                    data: buffer.clone(),
                    width,
                    height,
                    timestamp,
                };

                if tx.send(decoded).is_err() {
                    // Receiver dropped
                    let _ = child.kill();
                    return;
                }

                // Throttle? The pipe naturally throttles if we read slower than ffmpeg writes?
                // Actually ffmpeg writes as fast as it can. We should throttle to avoid filling memory.
                // Since channel is bounded (sync_channel(5)), we don't need to sleep manually.
                // The sender will block when channel is full.
                
                // thread::sleep(Duration::from_secs_f32(frame_duration * 0.5));
            }

            // Loop video
            let _ = child.wait();
            info!("Video loop restarting");
        }
    }

    /// Get the current frame for the given playback time.
    pub fn get_frame(&mut self, time: f32) -> Option<&DecodedFrame> {
         // Initialize start time on first call
        if self.start_time.is_none() {
            self.start_time = Some(time);
        }

        let playback_time = time - self.start_time.unwrap();
        
        // 1. Check if next_frame is ready to be shown
        if let Some(ref frame) = self.next_frame {
            if frame.timestamp <= playback_time {
                 // It's time! Move next to current
                 self.current_frame = self.next_frame.take();
            } else {
                // Not time yet, keep showing current
                return self.current_frame.as_ref();
            }
        }
        
        // 2. Consume frames from channel until we find one that is "in the future"
        loop {
            match self.frame_rx.try_recv() {
                Ok(frame) => {
                    if frame.timestamp <= playback_time {
                        // Frame is ready (or slightly late), show immediately
                        self.current_frame = Some(frame);
                        // Loop to see if there's an even newer one that is also ready (skip frames if lagging)
                    } else {
                        // Frame is in the future! Store it and stop reading
                        self.next_frame = Some(frame);
                        break;
                    }
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No more frames available right now
                    break;
                },
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Decoder dead
                    break;
                }
            }
        }
        
        self.current_frame.as_ref()
    }
}

fn parse_fps(s: &str) -> f32 {
    if let Some((num, den)) = s.split_once('/') {
        let n: f32 = num.parse().unwrap_or(0.0);
        let d: f32 = den.parse().unwrap_or(1.0);
        if d == 0.0 { 0.0 } else { n / d }
    } else {
        s.parse().unwrap_or(30.0)
    }
}
