use std::time::{Duration, Instant};

/// A utility for tracking frames per second.
pub struct FpsCounter {
    frame_count: u32,
    last_time: Instant,
    interval: Duration,
}

impl FpsCounter {
    /// Create a new FPS counter with the given reporting interval (default 1.0 second).
    pub fn new() -> Self {
        Self {
            frame_count: 0,
            last_time: Instant::now(),
            interval: Duration::from_secs(1),
        }
    }

    /// Update the counter with a new frame.
    /// Returns Some(fps) if the reporting interval has passed, otherwise None.
    pub fn update(&mut self) -> Option<f32> {
        self.frame_count += 1;
        let elapsed = self.last_time.elapsed();
        
        if elapsed >= self.interval {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.frame_count = 0;
            self.last_time = Instant::now();
            Some(fps)
        } else {
            None
        }
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self::new()
    }
}
