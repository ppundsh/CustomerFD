//! Animation timing helpers (all UI animations are ≤ 220 ms, ease-out).

use std::time::{Duration, Instant};

#[inline]
pub fn ease_out_cubic(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    1.0 - (1.0 - p).powi(3)
}

#[inline]
pub fn ease_out_quart(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    1.0 - (1.0 - p).powi(4)
}

/// A one-shot timed transition. `progress()` runs 0 → 1 over `dur` after `start()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tween {
    started: Option<Instant>,
    dur: Duration,
}

impl Tween {
    pub const fn new(ms: u64) -> Self {
        Self {
            started: None,
            dur: Duration::from_millis(ms),
        }
    }
    pub fn start(&mut self) {
        self.started = Some(Instant::now());
    }
    pub fn stop(&mut self) {
        self.started = None;
    }
    /// Raw linear progress; 1.0 when idle or finished.
    pub fn raw(&self) -> f32 {
        match self.started {
            Some(t) if !self.dur.is_zero() => {
                (t.elapsed().as_secs_f32() / self.dur.as_secs_f32()).clamp(0.0, 1.0)
            }
            _ => 1.0,
        }
    }
    pub fn running(&self) -> bool {
        self.started.is_some() && self.raw() < 1.0
    }
    /// Clears the start time once finished; returns whether it is still running.
    pub fn tick(&mut self) -> bool {
        if self.running() {
            true
        } else {
            self.started = None;
            false
        }
    }
    pub fn is_active(&self) -> bool {
        self.started.is_some()
    }
}

/// Exponential smoothing used by toggle switches: moves 35 % of the gap per 16 ms frame and
/// snaps when closer than 0.03. Returns `true` while still moving.
pub fn approach(cur: &mut f32, target: f32) -> bool {
    let d = target - *cur;
    if d.abs() > 0.03 {
        *cur += d * 0.35;
        true
    } else {
        *cur = target;
        false
    }
}
