//! The eversion state machine.
//!
//! All the gesture is, at heart, is a single scalar `t` in [0, 1] animating
//! towards a target (0 == outside, 1 == inside) at a rate fixed by
//! [`config::EVERSION_DURATION_MS`]. Everything visual (shell growth, camera
//! dolly, fold wave) is a pure function of the eased value of `t`, so the
//! whole gesture — including reversing it mid-flight — falls out of animating
//! this one number.

use crate::config;

/// Which way the gesture is currently settling.
///
/// Part of the module's public surface for the phases to come (HUD labels,
/// admission control, nested spheres); not all of it is wired up yet.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Outside,
    EvertingIn,
    Inside,
    EvertingOut,
}

pub struct Eversion {
    /// Raw linear progress, 0.0 (outside) .. 1.0 (inside).
    t: f32,
    /// Target the progress is animating towards: 0.0 or 1.0.
    target: f32,
}

impl Eversion {
    pub fn new() -> Self {
        Eversion {
            t: 0.0,
            target: 0.0,
        }
    }

    /// Begin enveloping the viewer (go inside).
    pub fn evert_in(&mut self) {
        self.target = 1.0;
    }

    /// Reverse: surface back out, return to the outside view.
    pub fn evert_out(&mut self) {
        self.target = 0.0;
    }

    /// Snap progress (and its target) to a settled value. Used when a
    /// surface-out transition needs to begin from the fully-inside state
    /// (t = 1) before animating back down.
    pub fn reset_to(&mut self, value: f32) {
        let v = value.clamp(0.0, 1.0);
        self.t = v;
        self.target = v;
    }

    /// Advance the animation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        let rate = dt / (config::EVERSION_DURATION_MS / 1000.0);
        if self.t < self.target {
            self.t = (self.t + rate).min(self.target);
        } else if self.t > self.target {
            self.t = (self.t - rate).max(self.target);
        }
    }

    /// True while the surface is mid-flight (this is our worst-case frame, and
    /// what the HUD's eversion budget counter watches).
    #[allow(dead_code)]
    pub fn is_animating(&self) -> bool {
        self.t != self.target
    }

    #[allow(dead_code)]
    pub fn phase(&self) -> Phase {
        match (self.t, self.target) {
            (t, _) if t <= 0.0 => Phase::Outside,
            (t, _) if t >= 1.0 => Phase::Inside,
            (_, target) if target >= 1.0 => Phase::EvertingIn,
            _ => Phase::EvertingOut,
        }
    }

    /// Raw progress, for any UI that wants the linear value.
    #[allow(dead_code)]
    pub fn raw(&self) -> f32 {
        self.t
    }

    /// Eased progress in [0, 1]. Smoothstep gives gentle ease-in/ease-out so
    /// the start and end of the gesture do not jerk.
    pub fn eased(&self) -> f32 {
        smoothstep(self.t)
    }
}

/// Classic smoothstep: 3t^2 - 2t^3.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
