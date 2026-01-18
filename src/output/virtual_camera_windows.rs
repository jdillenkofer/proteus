//! Windows virtual camera output using OBS Virtual Camera protocol.
//!
//! This module implements the OBS shared memory protocol to send frames
//! to the OBS Virtual Camera DirectShow filter on Windows.

use super::OutputBackend;
use crate::frame::VideoFrame;
use anyhow::{anyhow, Result};
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{debug, info};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};

/// Shared memory name used by OBS Virtual Camera.
const VIDEO_NAME: &str = "OBSVirtualCamVideo";

/// Frame header size for alignment.
const FRAME_HEADER_SIZE: u32 = 32;

/// Queue states matching OBS protocol.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueueState {
    Invalid = 0,
    Starting = 1,
    Ready = 2,
    Stopping = 3,
}

/// Queue header structure matching OBS's shared-memory-queue.c
#[repr(C)]
struct QueueHeader {
    /// Current write index (triple buffered)
    write_idx: AtomicU32,
    /// Current read index
    read_idx: AtomicU32,
    /// Queue state
    state: AtomicU32,
    /// Offsets to each of the 3 frame buffers
    offsets: [u32; 3],
    /// Queue type (video = 0)
    queue_type: u32,
    /// Frame width
    cx: u32,
    /// Frame height
    cy: u32,
    /// Frame interval in 100ns units
    interval: u64,
    /// Reserved for future use
    reserved: [u32; 8],
}

/// Configuration for virtual camera output.
#[derive(Debug, Clone)]
pub struct VirtualCameraConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for VirtualCameraConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
        }
    }
}

/// Aligns size to 32-byte boundary.
fn align_size(size: u32) -> u32 {
    (size + 31) & !31
}

/// Virtual camera output using OBS shared memory protocol.
pub struct VirtualCameraOutput {
    config: VirtualCameraConfig,
    handle: HANDLE,
    header: *mut QueueHeader,
    frames: [*mut u8; 3],
    timestamps: [*mut u64; 3],
}

// SAFETY: The shared memory pointers are only accessed from one thread
unsafe impl Send for VirtualCameraOutput {}

impl VirtualCameraOutput {
    /// Creates a new virtual camera output.
    ///
    /// This creates the shared memory region that OBS Virtual Camera will read from.
    pub fn new(config: VirtualCameraConfig) -> Result<Self> {
        // Check if OBS is already using the shared memory
        let existing = Self::check_existing()?;
        if existing {
            return Err(anyhow!(
                "OBS Virtual Camera shared memory already in use. \
                Make sure OBS Virtual Camera is not active in OBS Studio."
            ));
        }

        let (handle, header, frames, timestamps) = Self::create_shared_memory(&config)?;

        info!(
            "Virtual camera output created ({}x{} @ {} fps)",
            config.width, config.height, config.fps
        );
        info!("Select 'OBS Virtual Camera' in your video application");

        Ok(Self {
            config,
            handle,
            header,
            frames,
            timestamps,
        })
    }

    /// Check if OBS is already using the shared memory.
    fn check_existing() -> Result<bool> {
        let name: Vec<u16> = VIDEO_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr()));

            if let Ok(h) = handle {
                CloseHandle(h)?;
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Create the shared memory region.
    fn create_shared_memory(
        config: &VirtualCameraConfig,
    ) -> Result<(HANDLE, *mut QueueHeader, [*mut u8; 3], [*mut u64; 3])> {
        // Calculate NV12 frame size: Y plane + UV plane (half height)
        let frame_size = config.width * config.height * 3 / 2;

        // Calculate offsets for triple buffering
        let mut size = std::mem::size_of::<QueueHeader>() as u32;
        size = align_size(size);

        let mut offset_frame = [0u32; 3];
        for i in 0..3 {
            offset_frame[i] = size;
            size += frame_size + FRAME_HEADER_SIZE;
            size = align_size(size);
        }

        // Create the shared memory
        let name: Vec<u16> = VIDEO_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                size,
                PCWSTR(name.as_ptr()),
            )?
        };

        // Map the view
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, 0) };

        if view.Value.is_null() {
            unsafe {
                CloseHandle(handle)?;
            }
            return Err(anyhow!("Failed to map shared memory view"));
        }

        let header = view.Value as *mut QueueHeader;

        // Initialize the header
        unsafe {
            (*header).write_idx = AtomicU32::new(0);
            (*header).read_idx = AtomicU32::new(0);
            (*header).state = AtomicU32::new(QueueState::Starting as u32);
            (*header).offsets = offset_frame;
            (*header).queue_type = 0; // Video
            (*header).cx = config.width;
            (*header).cy = config.height;
            // Interval in 100ns units (10,000,000 / fps)
            (*header).interval = 10_000_000 / config.fps as u64;
            (*header).reserved = [0; 8];
        }

        // Get pointers to frame buffers
        let base = view.Value as *mut u8;
        let mut frames = [ptr::null_mut(); 3];
        let mut timestamps = [ptr::null_mut(); 3];

        for i in 0..3 {
            let offset = offset_frame[i];
            unsafe {
                timestamps[i] = base.add(offset as usize) as *mut u64;
                frames[i] = base.add(offset as usize + FRAME_HEADER_SIZE as usize);
            }
        }

        debug!("Shared memory created: {} bytes", size);

        Ok((handle, header, frames, timestamps))
    }

    /// Write a frame to the shared memory queue.
    fn write_frame_internal(&mut self, frame: &VideoFrame) -> Result<()> {
        // Convert to NV12
        let nv12_start = std::time::Instant::now();
        let nv12 = frame.to_nv12();
        let nv12_elapsed = nv12_start.elapsed();

        // Get current write index and advance
        let header = unsafe { &*self.header };
        let inc = header.write_idx.fetch_add(1, Ordering::SeqCst) + 1;
        let idx = (inc % 3) as usize;

        // Get frame dimensions from header
        let cx = header.cx as usize;
        let cy = header.cy as usize;

        // Calculate sizes
        let y_size = cx * cy;
        let uv_size = y_size / 2;

        // Write timestamp
        let timestamp = frame.timestamp_us.unwrap_or(0) * 10; // Convert to 100ns
        unsafe {
            *self.timestamps[idx] = timestamp;
        }

        let copy_start = std::time::Instant::now();
        // Copy Y plane
        unsafe {
            ptr::copy_nonoverlapping(nv12.data.as_ptr(), self.frames[idx], y_size);
        }

        // Copy UV plane
        unsafe {
            ptr::copy_nonoverlapping(
                nv12.data.as_ptr().add(y_size),
                self.frames[idx].add(y_size),
                uv_size,
            );
        }
        let copy_elapsed = copy_start.elapsed();

        debug!("  [Perf] VCam Write - NV12 conv: {:?}, SharedMem copy: {:?}", nv12_elapsed, copy_elapsed);

        // Update read index and state
        header.read_idx.store(inc, Ordering::SeqCst);
        header.state.store(QueueState::Ready as u32, Ordering::SeqCst);

        Ok(())
    }
}

impl Drop for VirtualCameraOutput {
    fn drop(&mut self) {
        // Signal stopping
        unsafe {
            (*self.header)
                .state
                .store(QueueState::Stopping as u32, Ordering::SeqCst);
        }

        // Unmap and close
        unsafe {
            let view = MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.header as *mut _,
            };
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(self.handle);
        }

        debug!("Virtual camera output closed");
    }
}

impl OutputBackend for VirtualCameraOutput {
    fn write_frame(&mut self, frame: &VideoFrame) -> Result<()> {
        self.write_frame_internal(frame)
    }
}
