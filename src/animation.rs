// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

const DEFAULT_DURATION_MS: u32 = 250;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Opening,
    Closing,
    Idle,
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Idle
    }
}

#[derive(Debug, Clone)]
pub struct AnimationState {
    pub phase: Phase,
    progress: f32,
    start: Instant,
    duration: Duration,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            progress: 1.0,
            start: Instant::now(),
            duration: Duration::from_millis(DEFAULT_DURATION_MS as u64),
        }
    }
}

/// Ease-out cubic: fast start, smooth deceleration.
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

impl AnimationState {
    pub fn set_duration_ms(&mut self, ms: u32) {
        let ms = if ms == 0 { DEFAULT_DURATION_MS } else { ms };
        self.duration = Duration::from_millis(ms as u64);
    }

    pub fn start_opening(&mut self) {
        self.phase = Phase::Opening;
        self.progress = 0.0;
        self.start = Instant::now();
    }

    pub fn start_closing(&mut self) {
        self.phase = Phase::Closing;
        self.progress = 0.0;
        self.start = Instant::now();
    }

    /// Advance animation based on elapsed time. Returns true if still animating.
    pub fn tick(&mut self) -> bool {
        if self.phase == Phase::Idle {
            return false;
        }
        let elapsed = self.start.elapsed();
        self.progress = (elapsed.as_secs_f32() / self.duration.as_secs_f32()).min(1.0);
        self.progress < 1.0
    }

    /// Returns the eased animation value (0.0 to 1.0).
    pub fn value(&self) -> f32 {
        ease_out_cubic(self.progress)
    }

    /// Returns the alpha multiplier for rendering. Always 1.0.
    pub fn alpha(&self) -> f32 {
        1.0
    }

    pub fn is_idle(&self) -> bool {
        self.phase == Phase::Idle
    }

    pub fn is_closing(&self) -> bool {
        self.phase == Phase::Closing
    }

    /// Returns position interpolation progress.
    /// Opening: 0 -> 1 (source desktop pos -> target grid pos)
    /// Closing: 1 -> 0 (grid pos -> back to desktop pos)
    /// Idle: 1 (at grid pos)
    pub fn position_progress(&self) -> f32 {
        match self.phase {
            Phase::Opening => self.value(),
            Phase::Closing => 1.0 - self.value(),
            Phase::Idle => 1.0,
        }
    }
}
