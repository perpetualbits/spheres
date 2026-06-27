//! Navigation state machine (id-keyed).
//!
//! Where you are is a `trail` of node ids (your history, NOT structural
//! parentage) plus an optional in-flight `Transition`. The empty trail is the
//! global OVERVIEW. The gesture rides one eased scalar `t` in [0, 1] from the
//! reused [`Eversion`]:
//!
//! * `t = 0` — resting in the OUTER world (overview, or the node at `trail`).
//! * `t = 1` — resting INSIDE the target node.
//!
//! Esc always drives `t` toward 0, so surfacing out is reliable from any state
//! (resting or mid-dive). Crucially, the *contents* of a node's world are a
//! pure function of its id (see `scene`/`graph`), never of the trail — so
//! reaching `core` two ways lands in the identical world (convergence).

use crate::eversion::Eversion;
use crate::graph::{Graph, NodeId};

/// The world you are resting in (or the OUTER world of a transition).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum World {
    /// The pulled-back global graph.
    Overview,
    /// Inside a node's local world (its neighbours).
    Node(NodeId),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Dir {
    In,
    Out,
}

#[derive(Copy, Clone)]
pub struct Transition {
    /// Node being entered/left.
    pub target: NodeId,
    pub dir: Dir,
}

pub struct Nav {
    trail: Vec<NodeId>,
    transition: Option<Transition>,
    eversion: Eversion,
}

impl Nav {
    pub fn new() -> Self {
        Nav {
            trail: Vec::new(),
            transition: None,
            eversion: Eversion::new(),
        }
    }

    // --- queries ---------------------------------------------------------

    /// The OUTER world (current resting world, or the parent during a dive).
    pub fn outer(&self) -> World {
        match self.trail.last() {
            None => World::Overview,
            Some(&id) => World::Node(id),
        }
    }

    pub fn transition(&self) -> Option<Transition> {
        self.transition
    }

    pub fn depth(&self) -> usize {
        self.trail.len()
    }

    pub fn eased(&self) -> f32 {
        self.eversion.eased()
    }

    pub fn is_everting(&self) -> bool {
        self.transition.is_some()
    }

    /// The node currently in focus (target of a dive, else the resting node).
    pub fn focus_node(&self) -> Option<NodeId> {
        match self.transition {
            Some(t) => Some(t.target),
            None => self.trail.last().copied(),
        }
    }

    /// Resting camera distance for a world.
    pub fn world_dist(world: World) -> f32 {
        match world {
            World::Overview => crate::config::OVERVIEW_DIST,
            World::Node(_) => crate::config::LOCAL_DIST,
        }
    }

    /// Eye distance for this frame, interpolating outer→inner across a dive,
    /// minus the small forward bob.
    pub fn eye_distance(&self) -> f32 {
        let eased = self.eased();
        let outer = Self::world_dist(self.outer());
        let inner = match self.transition {
            Some(t) => Self::world_dist(World::Node(t.target)),
            None => outer,
        };
        let d = outer + (inner - outer) * eased;
        d - crate::config::CAMERA_PUSH * (std::f32::consts::PI * eased).sin()
    }

    /// Breadcrumb trail, e.g. "overview > io > core" with "→ name" while diving.
    pub fn breadcrumb(&self, graph: &Graph) -> String {
        let mut s = String::from("overview");
        for &id in &self.trail {
            s.push_str(" > ");
            s.push_str(graph.node(id).name);
        }
        if let Some(t) = self.transition {
            match t.dir {
                Dir::In => {
                    s.push_str("  →  ");
                    s.push_str(graph.node(t.target).name);
                }
                Dir::Out => s.push_str("  ↑"),
            }
        }
        s
    }

    /// Background tint for the current context (Requirement 7).
    pub fn ambient(&self, graph: &Graph) -> [f64; 4] {
        let base = crate::config::CLEAR_OVERVIEW;
        let Some(id) = self.focus_node() else {
            return base;
        };
        // Skip while in the overview proper.
        if self.depth() == 0 && self.transition.is_none() {
            return base;
        }
        let tint = graph.node(id).kind.tint();
        let s = crate::config::AMBIENT_TINT_STRENGTH;
        [
            base[0] + tint.x as f64 * s,
            base[1] + tint.y as f64 * s,
            base[2] + tint.z as f64 * s,
            1.0,
        ]
    }

    // --- gestures --------------------------------------------------------

    /// Enter node `target`. Ignored unless resting.
    pub fn evert_in(&mut self, target: NodeId) {
        if self.transition.is_none() {
            self.transition = Some(Transition {
                target,
                dir: Dir::In,
            });
            self.eversion.reset_to(0.0);
            self.eversion.evert_in();
        }
    }

    /// Surface out one level. Reliable from any state.
    pub fn surface_out(&mut self) {
        match self.transition {
            None => {
                if let Some(last) = self.trail.pop() {
                    self.transition = Some(Transition {
                        target: last,
                        dir: Dir::Out,
                    });
                    self.eversion.reset_to(1.0);
                    self.eversion.evert_out();
                }
                // At the overview: nothing above; no-op.
            }
            Some(tr) if tr.dir == Dir::In => {
                // Reverse the dive (lands back in the outer world).
                self.transition = Some(Transition {
                    target: tr.target,
                    dir: Dir::Out,
                });
                self.eversion.evert_out();
            }
            Some(_) => {}
        }
    }

    // --- per-frame -------------------------------------------------------

    pub fn update(&mut self, dt: f32) {
        self.eversion.update(dt);
        if let Some(tr) = self.transition {
            match tr.dir {
                Dir::In if self.eversion.raw() >= 1.0 => {
                    self.trail.push(tr.target);
                    self.transition = None;
                    self.eversion.reset_to(0.0);
                }
                Dir::Out if self.eversion.raw() <= 0.0 => {
                    self.transition = None;
                    self.eversion.reset_to(0.0);
                }
                _ => {}
            }
        }
    }
}
