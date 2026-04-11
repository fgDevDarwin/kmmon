use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::Serialize;

// ---------------------------------------------------------------------------
// Raw events produced by capture.rs
// ---------------------------------------------------------------------------

/// Events captured from evdev devices, with all key identity discarded.
#[derive(Debug)]
pub enum RawEvent {
    /// Relative mouse movement (batched per EV_SYN frame).
    MouseRelMove { dx: i32, dy: i32 },
    /// Absolute mouse X position (touchpad / tablet).
    MouseAbsX(i32),
    /// Absolute mouse Y position (touchpad / tablet).
    MouseAbsY(i32),
    /// Scroll wheel delta.
    Scroll { dx: i32, dy: i32 },
    /// A key was pressed. Key identity has already been discarded.
    Keystroke,
}

// ---------------------------------------------------------------------------
// Processed output schemas (serialized to JSON for Foxglove)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MousePosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MouseScroll {
    pub dx: i32,
    pub dy: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardActivity {
    pub keystrokes_per_minute: u32,
    pub approx_wpm: f32,
    pub active: bool,
}

// ---------------------------------------------------------------------------
// KeyboardProcessor — privacy-first rolling-window WPM
// ---------------------------------------------------------------------------

pub struct KeyboardProcessor {
    timestamps: VecDeque<Instant>,
    window: Duration,
}

impl KeyboardProcessor {
    /// Creates a processor with the standard 60-second rolling window.
    pub fn new() -> Self {
        Self::with_window(Duration::from_secs(60))
    }

    /// Creates a processor with a custom window (useful for tests).
    pub fn with_window(window: Duration) -> Self {
        Self {
            timestamps: VecDeque::new(),
            window,
        }
    }

    /// Records a keystroke at the current instant. Key identity is never
    /// stored — only the timestamp is retained.
    pub fn record_keystroke(&mut self) {
        let now = Instant::now();
        self.timestamps.push_back(now);
        self.prune(now);
    }

    /// Returns the current activity snapshot and prunes expired timestamps.
    pub fn activity(&mut self) -> KeyboardActivity {
        let now = Instant::now();
        self.prune(now);
        let count = self.timestamps.len() as u32;
        KeyboardActivity {
            keystrokes_per_minute: count,
            approx_wpm: count as f32 / 5.0,
            active: count > 0,
        }
    }

    fn prune(&mut self, now: Instant) {
        let cutoff = now - self.window;
        while self.timestamps.front().map_or(false, |&t| t < cutoff) {
            self.timestamps.pop_front();
        }
    }
}

impl Default for KeyboardProcessor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn empty_window_returns_zero_activity() {
        let mut p = KeyboardProcessor::new();
        let a = p.activity();
        assert_eq!(a.keystrokes_per_minute, 0);
        assert_eq!(a.approx_wpm, 0.0);
        assert!(!a.active);
    }

    #[test]
    fn keystrokes_are_counted() {
        let mut p = KeyboardProcessor::new();
        for _ in 0..10 {
            p.record_keystroke();
        }
        let a = p.activity();
        assert_eq!(a.keystrokes_per_minute, 10);
        assert!(a.active);
    }

    #[test]
    fn wpm_is_keystrokes_divided_by_five() {
        let mut p = KeyboardProcessor::new();
        for _ in 0..50 {
            p.record_keystroke();
        }
        let a = p.activity();
        assert_eq!(a.keystrokes_per_minute, 50);
        assert_eq!(a.approx_wpm, 10.0);
    }

    #[test]
    fn expired_timestamps_are_pruned() {
        let mut p = KeyboardProcessor::with_window(Duration::from_millis(30));
        for _ in 0..5 {
            p.record_keystroke();
        }
        // Wait for all timestamps to expire.
        thread::sleep(Duration::from_millis(60));
        let a = p.activity();
        assert_eq!(a.keystrokes_per_minute, 0);
        assert!(!a.active);
    }

    #[test]
    fn only_recent_timestamps_counted() {
        let mut p = KeyboardProcessor::with_window(Duration::from_millis(100));
        // Record 3 "old" keystrokes, then wait for them to expire,
        // then record 2 fresh ones.
        for _ in 0..3 {
            p.record_keystroke();
        }
        thread::sleep(Duration::from_millis(150));
        for _ in 0..2 {
            p.record_keystroke();
        }
        let a = p.activity();
        assert_eq!(a.keystrokes_per_minute, 2);
        assert!(a.active);
    }
}
