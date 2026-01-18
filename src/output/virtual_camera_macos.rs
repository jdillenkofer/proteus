//! macOS virtual camera output using OBS Virtual Camera via CoreMediaIO.
//!
//! This module implements the OBS CMIOExtension protocol to send frames
//! to the OBS Virtual Camera on macOS 13+ with OBS 30+.

use super::OutputBackend;
use crate::frame::VideoFrame;
use anyhow::{anyhow, Result};
use core_foundation::base::{CFRelease, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::ptr;
use std::sync::Mutex;
use tracing::{debug, info};

/// OBS Virtual Camera device UUID (from OBS source code).
const OBS_DEVICE_UUID: &str = "7626645E-4425-469E-9D8B-97E0FA59AC75";

// CoreMediaIO type definitions
type CMIOObjectID = u32;
type CMIOStreamID = u32;
type OSStatus = i32;

const K_CMIO_OBJECT_SYSTEM_OBJECT: CMIOObjectID = 1;

// Property selectors
const K_CMIO_HARDWARE_PROPERTY_DEVICES: u32 = 0x64657623; // 'dev#'
const K_CMIO_DEVICE_PROPERTY_DEVICE_UID: u32 = 0x75696420; // 'uid '
const K_CMIO_DEVICE_PROPERTY_STREAMS: u32 = 0x73746d23; // 'stm#'

// Property scope and element
const K_CMIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = 0x676c6f62; // 'glob'
const K_CMIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

// CoreVideo pixel format
const K_CV_PIXEL_FORMAT_TYPE_422_YP_CB_CR8: u32 = 0x32767579; // '2vuy' (UYVY)

#[repr(C)]
struct CMIOObjectPropertyAddress {
    m_selector: u32,
    m_scope: u32,
    m_element: u32,
}

// CoreVideo types
type CVPixelBufferRef = *mut c_void;
type CVPixelBufferPoolRef = *mut c_void;
type CVReturn = i32;

const K_CV_RETURN_SUCCESS: CVReturn = 0;

// CoreMedia types
type CMSampleBufferRef = *mut c_void;
type CMFormatDescriptionRef = *mut c_void;
type CMSimpleQueueRef = *mut c_void;

/// CMTime struct - represents time as a rational number (value/timescale).
/// This is a 24-byte struct on 64-bit systems.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CMTime {
    /// The value of the CMTime (numerator)
    value: i64,
    /// The timescale (denominator) - number of units per second
    timescale: i32,
    /// Flags indicating validity, rounding, etc.
    flags: u32,
    /// Epoch for distinguishing different timelines
    epoch: i64,
}

impl CMTime {
    /// Create a valid CMTime with the given value and timescale.
    fn new(value: i64, timescale: i32) -> Self {
        const K_CM_TIME_FLAGS_VALID: u32 = 1;
        Self {
            value,
            timescale,
            flags: K_CM_TIME_FLAGS_VALID,
            epoch: 0,
        }
    }
}

#[link(name = "CoreMediaIO", kind = "framework")]
extern "C" {
    fn CMIOObjectGetPropertyDataSize(
        object_id: CMIOObjectID,
        address: *const CMIOObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: *mut u32,
    ) -> OSStatus;

    fn CMIOObjectGetPropertyData(
        object_id: CMIOObjectID,
        address: *const CMIOObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: u32,
        data_used: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;

    fn CMIODeviceStartStream(device_id: CMIOObjectID, stream_id: CMIOStreamID) -> OSStatus;
    fn CMIODeviceStopStream(device_id: CMIOObjectID, stream_id: CMIOStreamID) -> OSStatus;
    
    fn CMIOStreamCopyBufferQueue(
        stream_id: CMIOStreamID,
        callback: extern "C" fn(CMIOStreamID, *mut c_void, *mut c_void),
        refcon: *mut c_void,
        queue: *mut CMSimpleQueueRef,
    ) -> OSStatus;
}

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVPixelBufferPoolCreate(
        allocator: *const c_void,
        pool_attributes: *const c_void,
        pixel_buffer_attributes: *const c_void,
        pool_out: *mut CVPixelBufferPoolRef,
    ) -> CVReturn;

    fn CVPixelBufferPoolCreatePixelBuffer(
        allocator: *const c_void,
        pool: CVPixelBufferPoolRef,
        pixel_buffer_out: *mut CVPixelBufferRef,
    ) -> CVReturn;

    fn CVPixelBufferPoolRelease(pool: CVPixelBufferPoolRef);

    fn CVPixelBufferLockBaseAddress(pixel_buffer: CVPixelBufferRef, lock_flags: u64) -> CVReturn;
    fn CVPixelBufferUnlockBaseAddress(pixel_buffer: CVPixelBufferRef, lock_flags: u64) -> CVReturn;
    fn CVPixelBufferGetBaseAddress(pixel_buffer: CVPixelBufferRef) -> *mut u8;
    fn CVPixelBufferGetDataSize(pixel_buffer: CVPixelBufferRef) -> usize;
    fn CVPixelBufferRelease(pixel_buffer: CVPixelBufferRef);
}

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    fn CMVideoFormatDescriptionCreate(
        allocator: *const c_void,
        codec_type: u32,
        width: i32,
        height: i32,
        extensions: *const c_void,
        format_description_out: *mut CMFormatDescriptionRef,
    ) -> OSStatus;

    fn CMSampleBufferCreateForImageBuffer(
        allocator: *const c_void,
        image_buffer: CVPixelBufferRef,
        data_ready: bool,
        make_data_ready_callback: *const c_void,
        make_data_ready_refcon: *const c_void,
        format_description: CMFormatDescriptionRef,
        sample_timing: *const CMSampleTimingInfo,
        sample_buffer_out: *mut CMSampleBufferRef,
    ) -> OSStatus;

    fn CMSimpleQueueEnqueue(queue: CMSimpleQueueRef, element: *const c_void) -> OSStatus;
}

/// Sample timing info structure for CMSampleBuffer.
#[repr(C)]
struct CMSampleTimingInfo {
    /// Duration of the sample (can be kCMTimeInvalid)
    duration: CMTime,
    /// Presentation timestamp
    presentation_time_stamp: CMTime,
    /// Decode timestamp (can be kCMTimeInvalid for video)
    decode_time_stamp: CMTime,
}

#[link(name = "Foundation", kind = "framework")]
extern "C" {
    fn CFStringGetCStringPtr(the_string: CFStringRef, encoding: u32) -> *const i8;
}

const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

// Callback for CMIOStreamCopyBufferQueue (no-op)
extern "C" fn queue_callback(_stream_id: CMIOStreamID, _token: *mut c_void, _refcon: *mut c_void) {}

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

/// Global mutex to ensure only one virtual camera instance at a time.
static INSTANCE_MUTEX: Mutex<()> = Mutex::new(());

/// Virtual camera output using OBS CMIOExtension protocol.
pub struct VirtualCameraOutput {
    _config: VirtualCameraConfig,
    device_id: CMIOObjectID,
    stream_id: CMIOStreamID,
    queue: CMSimpleQueueRef,
    pixel_buffer_pool: CVPixelBufferPoolRef,
    format_description: CMFormatDescriptionRef,
    frame_size: usize,
    _lock: std::sync::MutexGuard<'static, ()>,
}

// SAFETY: The CoreMediaIO handles are thread-safe when used correctly
unsafe impl Send for VirtualCameraOutput {}

impl VirtualCameraOutput {
    /// Creates a new virtual camera output.
    ///
    /// This finds the OBS Virtual Camera device and sets up the stream.
    /// Requires OBS Studio 30+ to be installed and the virtual camera
    /// to have been started at least once.
    pub fn new(config: VirtualCameraConfig) -> Result<Self> {
        // Acquire global lock to prevent multiple instances
        let lock = INSTANCE_MUTEX.lock().map_err(|_| {
            anyhow!("Failed to acquire virtual camera lock")
        })?;

        // Find OBS Virtual Camera device
        let device_id = Self::find_obs_device()?;
        debug!("Found OBS Virtual Camera device: {}", device_id);

        // Get streams for the device
        let stream_id = Self::get_stream(device_id)?;
        debug!("Found OBS Virtual Camera stream: {}", stream_id);

        // Get buffer queue
        let queue = Self::get_queue(stream_id)?;
        debug!("Got buffer queue");

        // Create pixel buffer pool
        let pixel_buffer_pool = Self::create_pixel_buffer_pool(config.width, config.height)?;
        debug!("Created pixel buffer pool");

        // Create format description
        let format_description = Self::create_format_description(config.width, config.height)?;
        debug!("Created format description");

        // Start the stream
        let result = unsafe { CMIODeviceStartStream(device_id, stream_id) };
        if result != 0 {
            return Err(anyhow!("Failed to start OBS Virtual Camera stream (error {})", result));
        }

        let frame_size = (config.width as usize) * (config.height as usize) * 2; // UYVY = 2 bytes/pixel

        info!(
            "Virtual camera output created ({}x{} @ {} fps)",
            config.width, config.height, config.fps
        );
        info!("Select 'OBS Virtual Camera' in your video application");

        Ok(Self {
            _config: config,
            device_id,
            stream_id,
            queue,
            pixel_buffer_pool,
            format_description,
            frame_size,
            _lock: lock,
        })
    }

    /// Find the OBS Virtual Camera device by UUID.
    fn find_obs_device() -> Result<CMIOObjectID> {
        let mut size: u32 = 0;
        let address = CMIOObjectPropertyAddress {
            m_selector: K_CMIO_HARDWARE_PROPERTY_DEVICES,
            m_scope: K_CMIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: K_CMIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        // Get size of device list
        let result = unsafe {
            CMIOObjectGetPropertyDataSize(
                K_CMIO_OBJECT_SYSTEM_OBJECT,
                &address,
                0,
                ptr::null(),
                &mut size,
            )
        };

        if result != 0 {
            return Err(anyhow!("Failed to get CMIO device list size (error {})", result));
        }

        let device_count = size as usize / std::mem::size_of::<CMIOObjectID>();
        if device_count == 0 {
            return Err(anyhow!(
                "No camera devices found. Make sure OBS Studio 30+ is installed \
                and you have used 'Start Virtual Camera' at least once."
            ));
        }

        let mut devices = vec![0u32; device_count];
        let mut used: u32 = 0;

        // Get device list
        let result = unsafe {
            CMIOObjectGetPropertyData(
                K_CMIO_OBJECT_SYSTEM_OBJECT,
                &address,
                0,
                ptr::null(),
                size,
                &mut used,
                devices.as_mut_ptr() as *mut c_void,
            )
        };

        if result != 0 {
            return Err(anyhow!("Failed to get CMIO device list (error {})", result));
        }

        // Search for OBS device by UID
        let uid_address = CMIOObjectPropertyAddress {
            m_selector: K_CMIO_DEVICE_PROPERTY_DEVICE_UID,
            m_scope: K_CMIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: K_CMIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        for device_id in devices {
            let mut uid_size: u32 = 0;
            let result = unsafe {
                CMIOObjectGetPropertyDataSize(device_id, &uid_address, 0, ptr::null(), &mut uid_size)
            };

            if result != 0 {
                continue;
            }

            let mut uid_cf: CFStringRef = ptr::null();
            let mut uid_used: u32 = 0;
            let result = unsafe {
                CMIOObjectGetPropertyData(
                    device_id,
                    &uid_address,
                    0,
                    ptr::null(),
                    uid_size,
                    &mut uid_used,
                    &mut uid_cf as *mut _ as *mut c_void,
                )
            };

            if result != 0 || uid_cf.is_null() {
                continue;
            }

            // Get UTF-8 string from CFString
            let uid_ptr = unsafe { CFStringGetCStringPtr(uid_cf, K_CF_STRING_ENCODING_UTF8) };
            
            let matches = if !uid_ptr.is_null() {
                let uid_str = unsafe { std::ffi::CStr::from_ptr(uid_ptr) };
                uid_str.to_str().map(|s| s == OBS_DEVICE_UUID).unwrap_or(false)
            } else {
                // Fallback: use CFString comparison
                let cf_string = unsafe { CFString::wrap_under_get_rule(uid_cf) };
                cf_string.to_string() == OBS_DEVICE_UUID
            };

            unsafe { CFRelease(uid_cf as *const c_void) };

            if matches {
                return Ok(device_id);
            }
        }

        Err(anyhow!(
            "OBS Virtual Camera not found. Please ensure:\n\
            1. OBS Studio 30+ is installed\n\
            2. You have clicked 'Start Virtual Camera' in OBS at least once\n\
            3. The System Extension is approved in System Settings > Privacy & Security\n\
            4. You may need to restart your machine after approving the extension"
        ))
    }

    /// Get the output stream for the device.
    fn get_stream(device_id: CMIOObjectID) -> Result<CMIOStreamID> {
        let mut size: u32 = 0;
        let address = CMIOObjectPropertyAddress {
            m_selector: K_CMIO_DEVICE_PROPERTY_STREAMS,
            m_scope: K_CMIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: K_CMIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let result = unsafe {
            CMIOObjectGetPropertyDataSize(device_id, &address, 0, ptr::null(), &mut size)
        };

        if result != 0 {
            return Err(anyhow!("Failed to get stream list size (error {})", result));
        }

        let stream_count = size as usize / std::mem::size_of::<CMIOStreamID>();
        if stream_count < 2 {
            return Err(anyhow!("OBS Virtual Camera stream not found (expected 2 streams, found {})", stream_count));
        }

        let mut streams = vec![0u32; stream_count];
        let mut used: u32 = 0;

        let result = unsafe {
            CMIOObjectGetPropertyData(
                device_id,
                &address,
                0,
                ptr::null(),
                size,
                &mut used,
                streams.as_mut_ptr() as *mut c_void,
            )
        };

        if result != 0 {
            return Err(anyhow!("Failed to get stream list (error {})", result));
        }

        Ok(streams[1])
    }

    /// Get the buffer queue for the stream.
    fn get_queue(stream_id: CMIOStreamID) -> Result<CMSimpleQueueRef> {
        let mut queue: CMSimpleQueueRef = ptr::null_mut();
        
        let result = unsafe {
            CMIOStreamCopyBufferQueue(stream_id, queue_callback, ptr::null_mut(), &mut queue)
        };

        if result != 0 || queue.is_null() {
            return Err(anyhow!("Failed to get buffer queue (error {})", result));
        }

        Ok(queue)
    }

    /// Create a pixel buffer pool for UYVY frames.
    fn create_pixel_buffer_pool(width: u32, height: u32) -> Result<CVPixelBufferPoolRef> {
        use core_foundation::dictionary::CFMutableDictionary;
        use core_foundation::number::CFNumber;
        use core_foundation::base::TCFType;

        // CoreVideo pixel buffer attribute keys (from CoreVideo/CVPixelBuffer.h)
        #[link(name = "CoreVideo", kind = "framework")]
        extern "C" {
            static kCVPixelBufferPixelFormatTypeKey: CFStringRef;
            static kCVPixelBufferWidthKey: CFStringRef;
            static kCVPixelBufferHeightKey: CFStringRef;
            static kCVPixelBufferIOSurfacePropertiesKey: CFStringRef;
        }

        // Build pixel buffer attributes dictionary
        let mut pb_attrs: CFMutableDictionary<CFString, core_foundation::base::CFType> = CFMutableDictionary::new();
        
        unsafe {
            // Set pixel format
            let pixel_format = CFNumber::from(K_CV_PIXEL_FORMAT_TYPE_422_YP_CB_CR8 as i32);
            pb_attrs.set(
                CFString::wrap_under_get_rule(kCVPixelBufferPixelFormatTypeKey),
                pixel_format.as_CFType(),
            );

            // Set width
            let width_val = CFNumber::from(width as i32);
            pb_attrs.set(
                CFString::wrap_under_get_rule(kCVPixelBufferWidthKey),
                width_val.as_CFType(),
            );

            // Set height
            let height_val = CFNumber::from(height as i32);
            pb_attrs.set(
                CFString::wrap_under_get_rule(kCVPixelBufferHeightKey),
                height_val.as_CFType(),
            );

            // Set IOSurface properties (empty dict, but required for sharing)
            let io_surface_props: CFMutableDictionary<CFString, core_foundation::base::CFType> = CFMutableDictionary::new();
            pb_attrs.set(
                CFString::wrap_under_get_rule(kCVPixelBufferIOSurfacePropertiesKey),
                io_surface_props.as_CFType(),
            );
        }

        let mut pool: CVPixelBufferPoolRef = ptr::null_mut();
        let result = unsafe {
            CVPixelBufferPoolCreate(
                ptr::null(),
                ptr::null(), // pool attributes (none needed)
                pb_attrs.as_concrete_TypeRef() as *const c_void,
                &mut pool,
            )
        };

        if result != K_CV_RETURN_SUCCESS || pool.is_null() {
            return Err(anyhow!("Failed to create pixel buffer pool (error {})", result));
        }

        Ok(pool)
    }

    /// Create a format description for UYVY video.
    fn create_format_description(width: u32, height: u32) -> Result<CMFormatDescriptionRef> {
        let mut format_desc: CMFormatDescriptionRef = ptr::null_mut();
        
        let result = unsafe {
            CMVideoFormatDescriptionCreate(
                ptr::null(),
                K_CV_PIXEL_FORMAT_TYPE_422_YP_CB_CR8,
                width as i32,
                height as i32,
                ptr::null(),
                &mut format_desc,
            )
        };

        if result != 0 || format_desc.is_null() {
            return Err(anyhow!("Failed to create format description (error {})", result));
        }

        Ok(format_desc)
    }

    /// Write a frame to the virtual camera.
    fn write_frame_internal(&mut self, frame: &VideoFrame) -> Result<()> {
        // Convert frame to UYVY
        let uyvy = frame.to_uyvy();

        // Create pixel buffer from pool
        let mut pixel_buffer: CVPixelBufferRef = ptr::null_mut();
        let result = unsafe {
            CVPixelBufferPoolCreatePixelBuffer(
                ptr::null(),
                self.pixel_buffer_pool,
                &mut pixel_buffer,
            )
        };

        if result != K_CV_RETURN_SUCCESS || pixel_buffer.is_null() {
            return Err(anyhow!("Failed to create pixel buffer (error {})", result));
        }

        // Lock buffer and copy data
        unsafe {
            CVPixelBufferLockBaseAddress(pixel_buffer, 0);
            
            let dst = CVPixelBufferGetBaseAddress(pixel_buffer);
            let dst_size = CVPixelBufferGetDataSize(pixel_buffer);

            if dst_size >= self.frame_size {
                ptr::copy_nonoverlapping(uyvy.data.as_ptr(), dst, self.frame_size);
            }

            CVPixelBufferUnlockBaseAddress(pixel_buffer, 0);
        }

        // Create sample buffer with timing
        // Use high-resolution clock for timestamp (nanoseconds)
        let timestamp = unsafe {
            let mut time_info = libc::timespec { tv_sec: 0, tv_nsec: 0 };
            libc::clock_gettime(libc::CLOCK_UPTIME_RAW, &mut time_info);
            let ns = time_info.tv_sec as i64 * 1_000_000_000 + time_info.tv_nsec as i64;
            CMTime::new(ns, 1_000_000_000)
        };

        // kCMTimeInvalid for duration and decode timestamp
        let invalid_time = CMTime {
            value: 0,
            timescale: 0,
            flags: 0, // no valid flag = invalid
            epoch: 0,
        };

        let timing_info = CMSampleTimingInfo {
            duration: invalid_time,
            presentation_time_stamp: timestamp,
            decode_time_stamp: invalid_time,
        };

        let mut sample_buffer: CMSampleBufferRef = ptr::null_mut();
        let result = unsafe {
            CMSampleBufferCreateForImageBuffer(
                ptr::null(),
                pixel_buffer,
                true,
                ptr::null(),
                ptr::null(),
                self.format_description,
                &timing_info,
                &mut sample_buffer,
            )
        };

        if result != 0 || sample_buffer.is_null() {
            unsafe { CVPixelBufferRelease(pixel_buffer) };
            return Err(anyhow!("Failed to create sample buffer (error {})", result));
        }

        // Enqueue the sample buffer
        let result = unsafe { CMSimpleQueueEnqueue(self.queue, sample_buffer) };

        // Release pixel buffer (sample buffer retains it)
        unsafe { CVPixelBufferRelease(pixel_buffer) };

        if result != 0 {
            return Err(anyhow!("Failed to enqueue sample buffer (error {})", result));
        }

        Ok(())
    }
}

impl Drop for VirtualCameraOutput {
    fn drop(&mut self) {
        // Stop the stream
        unsafe {
            CMIODeviceStopStream(self.device_id, self.stream_id);
        }

        // Release resources
        if !self.format_description.is_null() {
            unsafe { CFRelease(self.format_description) };
        }

        if !self.pixel_buffer_pool.is_null() {
            unsafe { CVPixelBufferPoolRelease(self.pixel_buffer_pool) };
        }

        debug!("Virtual camera output closed");
    }
}

impl OutputBackend for VirtualCameraOutput {
    fn write_frame(&mut self, frame: &VideoFrame) -> Result<()> {
        self.write_frame_internal(frame)
    }
}
