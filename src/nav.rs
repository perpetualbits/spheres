//! Navigation state machine.
//!
//! Where you are is a `path` (the child indices taken from the root) plus an
//! optional in-flight `Transition`. The whole gesture rides one eased scalar
//! `t` in [0, 1] from the reused [`Eversion`]:
//!
//! * `t = 0` — resting in the OUTER (parent) world, looking at its spheres.
//! * `t = 1` — resting INSIDE the target child, looking at its spheres.
//!
//! Entering animates 0 -> 1 then commits (push the child onto the path).
//! Surfacing animates 1 -> 0 then commits (pop). The crucial property: **Esc
//! always drives `t` toward 0**, so "surface out" is reliable from any state —
//! resting, or mid-dive (which simply reverses, landing one level up).

use crate::eversion::Eversion;
use crate::world;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Dir {
    /// Diving into `target`; on completion the child is committed onto the path.
    In,
    /// Surfacing; on completion the transition simply ends (the pop, if any,
    /// already happened when the surface began).
    Out,
}

#[derive(Copy, Clone)]
pub struct Transition {
    /// Index (within the OUTER/parent world) of the sphere being entered/left.
    pub target: usize,
    pub dir: Dir,
}

pub struct Nav {
    /// Committed depth: the OUTER world's path at all times (during a dive the
    /// target is not pushed until completion; during a surface it is already
    /// popped).
    path: Vec<usize>,
    transition: Option<Transition>,
    eversion: Eversion,
}

impl Nav {
    pub fn new() -> Self {
        Nav {
            path: Vec::new(),
            transition: None,
            eversion: Eversion::new(),
        }
    }

    // --- queries ---------------------------------------------------------

    /// The OUTER world's path (parent during a transition, else the current
    /// world).
    pub fn path(&self) -> &[usize] {
        &self.path
    }

    pub fn transition(&self) -> Option<Transition> {
        self.transition
    }

    /// Committed depth (number of levels below root).
    pub fn depth(&self) -> usize {
        self.path.len()
    }

    /// Eased transition progress in [0, 1].
    pub fn eased(&self) -> f32 {
        self.eversion.eased()
    }

    /// True while a gesture is mid-flight — this is the worst-case frame the
    /// HUD's budget counter watches.
    pub fn is_everting(&self) -> bool {
        self.transition.is_some()
    }

    /// Human-readable trail, e.g. "Root > Amber > Azure", with "→ Emerald"
    /// appended while diving in.
    pub fn breadcrumb(&self) -> String {
        let mut s = String::from("Root");
        for &idx in &self.path {
            s.push_str(" > ");
            s.push_str(world::color_name(idx));
        }
        if let Some(tr) = self.transition {
            match tr.dir {
                Dir::In => {
                    s.push_str("  →  ");
                    s.push_str(world::color_name(tr.target));
                }
                Dir::Out => s.push_str("  ↑"),
            }
        }
        s
    }

    // --- gestures --------------------------------------------------------

    /// Enter child `target` of the current world. Ignored unless resting.
    pub fn evert_in(&mut self, target: usize) {
        if self.transition.is_none() {
            self.transition = Some(Transition {
                target,
                dir: Dir::In,
            });
            self.eversion.reset_to(0.0);
            self.eversion.evert_in();
        }
    }

    /// Surface out exactly one level. Reliable from any state — this is the
    /// most important interaction in the build.
    pub fn surface_out(&mut self) {
        match self.transition {
            // Resting: pop one level and run the reverse transition from the
            // fully-inside state.
            None => {
                if let Some(last) = self.path.pop() {
                    self.transition = Some(Transition {
                        target: last,
                        dir: Dir::Out,
                    });
                    self.eversion.reset_to(1.0);
                    self.eversion.evert_out();
                }
                // At root (empty path): nothing to surface to; no-op.
            }
            // Mid-dive: reverse it. We never pushed, so completing at t = 0
            // simply lands us back in the outer world — one level up from where
            // the dive was heading.
            Some(tr) if tr.dir == Dir::In => {
                self.transition = Some(Transition {
                    target: tr.target,
                    dir: Dir::Out,
                });
                self.eversion.evert_out();
            }
            // Already surfacing: leave it be.
            Some(_) => {}
        }
    }

    // --- per-frame -------------------------------------------------------

    pub fn update(&mut self, dt: f32) {
        self.eversion.update(dt);

        if let Some(tr) = self.transition {
            match tr.dir {
                Dir::In if self.eversion.raw() >= 1.0 => {
                    // Arrived inside: commit the descent.
                    self.path.push(tr.target);
                    self.transition = None;
                    self.eversion.reset_to(0.0);
                }
                Dir::Out if self.eversion.raw() <= 0.0 => {
                    // Back in the outer world (already popped, if this was a
                    // real surface-out).
                    self.transition = None;
                    self.eversion.reset_to(0.0);
                }
                _ => {}
            }
        }
    }
}
