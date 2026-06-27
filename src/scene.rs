//! Per-frame scene assembly.
//!
//! Turns the graph + navigation state into the renderer's instance lists
//! (node spheres, ring readouts, edge links) plus billboarded labels. Two
//! kinds of "world" are drawn with the same primitives:
//!
//! * the global OVERVIEW (depth 0) — every node at its fixed position, every
//!   edge, so the structure (core's hub, the cross-couplings, a person's span)
//!   is visible at a glance;
//! * a node's LOCAL world — its neighbours laid out around you, edges radiating
//!   from the centre. The layout is a pure function of the node's id, so the
//!   same node reached two ways is the same world (convergence).

use glam::Vec3;

use crate::camera::Camera;
use crate::config;
use crate::graph::{Graph, NodeId, Relation};
use crate::hud::Label;
use crate::nav::{Nav, World};
use crate::render::{EdgeInstance, Instance, RingInstance};

pub struct Frame {
    pub clear: [f64; 4],
    pub nodes: Vec<Instance>,
    pub rings: Vec<RingInstance>,
    pub edges: Vec<EdgeInstance>,
    pub labels: Vec<Label>,
}

struct Placement {
    id: NodeId,
    pos: Vec3,
    radius: f32,
}

struct EdgeSpec {
    a: Vec3,
    b: Vec3,
    rel: Relation,
    from: Option<NodeId>,
    to: Option<NodeId>,
}

pub fn build(graph: &Graph, nav: &Nav, camera: &Camera, time: f32, cursor: (f32, f32)) -> Frame {
    let mut f = Frame {
        clear: nav.ambient(graph),
        nodes: Vec::new(),
        rings: Vec::new(),
        edges: Vec::new(),
        labels: Vec::new(),
    };

    let eased = nav.eased();
    let eye_z = nav.eye_distance();
    let (ray_o, ray_d) = camera.ray(eye_z, cursor.0, cursor.1);

    match nav.transition() {
        None => {
            // Resting: render the current world fully.
            let world = nav.outer();
            let placements = placements_of(graph, world);
            let hovered = pick_placement(&placements, ray_o, ray_d);

            for p in &placements {
                let focus = if Some(p.id) == hovered { 1.0 } else { 0.0 };
                push_node(&mut f, graph, p.id, p.pos, p.radius, 1.0, 0.0, focus, time, true);
            }
            if let World::Node(c) = world {
                f.labels.push(center_label(graph, c));
            }
            let local = matches!(world, World::Node(_));
            for es in edges_of(graph, world) {
                let hot = hovered.is_some() && (es.from == hovered || es.to == hovered);
                push_edge(&mut f, &es, edge_glow(es.rel, hot), 1.0);
                // In a local world (few edges) name the relation at its midpoint.
                if local {
                    let c = es.rel.color();
                    f.labels.push(Label {
                        text: es.rel.label().to_string(),
                        pos: es.a.lerp(es.b, 0.5),
                        rgb: [c.x, c.y, c.z],
                        alpha: 0.6,
                    });
                }
            }
        }
        Some(tr) => {
            // Eversion: outer world fades out, target grows to envelop, inner
            // world fades in — exactly the resting inner world by t = 1.
            let outer = nav.outer();
            let sib = 1.0 - smoothstep(eased, 0.0, 0.55);
            let shell_a = 1.0 - smoothstep(eased, 0.62, 1.0);
            let inner_a = smoothstep(eased, 0.45, 1.0);
            let shell_e = smoothstep(eased, 0.0, 1.0);

            for p in placements_of(graph, outer) {
                if p.id == tr.target {
                    let center = p.pos.lerp(Vec3::ZERO, shell_e);
                    let radius = lerp(p.radius, config::ENVELOP_RADIUS, shell_e);
                    // The enveloping shell: no readout, drives the fold.
                    push_node(&mut f, graph, p.id, center, radius, shell_a, eased, 1.0, time, false);
                } else {
                    push_node(&mut f, graph, p.id, p.pos, p.radius, sib, 0.0, 0.0, time, sib > 0.25);
                }
            }
            for es in edges_of(graph, outer) {
                // Skip edges touching the moving target; fade the rest.
                if es.from == Some(tr.target) || es.to == Some(tr.target) {
                    continue;
                }
                push_edge(&mut f, &es, edge_glow(es.rel, false), sib);
            }

            let inner = World::Node(tr.target);
            for p in placements_of(graph, inner) {
                let r = p.radius * inner_a.max(0.001);
                push_node(&mut f, graph, p.id, p.pos, r, inner_a, 0.0, 0.0, time, inner_a > 0.4);
            }
            for es in edges_of(graph, inner) {
                push_edge(&mut f, &es, edge_glow(es.rel, false), inner_a);
            }
        }
    }

    f
}

/// Pick the node under the cursor in the current resting world (for clicks).
pub fn pick(graph: &Graph, nav: &Nav, camera: &Camera, ndc_x: f32, ndc_y: f32) -> Option<NodeId> {
    if nav.is_everting() {
        return None;
    }
    let (o, d) = camera.ray(nav.eye_distance(), ndc_x, ndc_y);
    let placements = placements_of(graph, nav.outer());
    pick_placement(&placements, o, d)
}

// --- world geometry ------------------------------------------------------

fn placements_of(graph: &Graph, world: World) -> Vec<Placement> {
    match world {
        World::Overview => graph
            .nodes
            .iter()
            .map(|n| Placement {
                id: n.id,
                pos: n.pos,
                radius: n.radius(),
            })
            .collect(),
        World::Node(c) => local_layout(graph, c)
            .into_iter()
            .map(|(id, _rel, pos)| Placement {
                id,
                pos,
                radius: graph.node(id).radius(),
            })
            .collect(),
    }
}

fn edges_of(graph: &Graph, world: World) -> Vec<EdgeSpec> {
    match world {
        World::Overview => graph
            .edges
            .iter()
            .map(|e| EdgeSpec {
                a: graph.node(e.from).pos,
                b: graph.node(e.to).pos,
                rel: e.rel,
                from: Some(e.from),
                to: Some(e.to),
            })
            .collect(),
        World::Node(c) => local_layout(graph, c)
            .into_iter()
            .map(|(id, rel, pos)| EdgeSpec {
                a: Vec3::ZERO, // edges radiate from "you" (the node you are in)
                b: pos,
                rel,
                from: Some(c),
                to: Some(id),
            })
            .collect(),
    }
}

/// Deterministic layout of a node's neighbours around the origin. Pure
/// function of the node id (and its neighbour set), so it is identical every
/// visit — this is what makes id-keyed convergence visible.
fn local_layout(graph: &Graph, c: NodeId) -> Vec<(NodeId, Relation, Vec3)> {
    let nbrs = graph.neighborhood(c);
    let n = nbrs.len().max(1);
    let phase = c as f32 * 1.30219; // stable per-node rotation
    nbrs.iter()
        .enumerate()
        .map(|(k, nb)| {
            let ang = phase + k as f32 / n as f32 * std::f32::consts::TAU;
            let z = (k as f32 * 0.7).sin() * 0.5;
            let pos = Vec3::new(
                ang.cos() * config::LOCAL_LAYOUT_RADIUS,
                ang.sin() * config::LOCAL_LAYOUT_RADIUS,
                z,
            );
            (nb.id, nb.rel, pos)
        })
        .collect()
}

// --- emit primitives -----------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn push_node(
    f: &mut Frame,
    graph: &Graph,
    id: NodeId,
    center: Vec3,
    radius: f32,
    alpha: f32,
    evert: f32,
    focus: f32,
    time: f32,
    readout: bool,
) {
    if alpha <= 0.01 {
        return;
    }
    let node = graph.node(id);
    let tint = node.kind.tint();
    let glow = node.kind.glow();
    f.nodes.push(Instance::node(
        center,
        radius,
        tint,
        alpha,
        evert,
        focus,
        glow,
        time * 0.15,
    ));

    if readout {
        f.rings
            .push(RingInstance::new(center, radius, tint, node.bits, alpha));
        f.labels.push(Label {
            text: format!("{}  ·  {}", node.name, node.kind.name()),
            pos: center + Vec3::Y * (radius * 1.2),
            rgb: [tint.x, tint.y, tint.z],
            alpha,
        });
    }
}

fn push_edge(f: &mut Frame, es: &EdgeSpec, glow: f32, alpha: f32) {
    let width = config::EDGE_WIDTH * if es.rel.is_ownership() { 1.5 } else { 1.0 };
    f.edges
        .push(EdgeInstance::new(es.a, es.b, width, glow, es.rel.color(), alpha));
}

fn center_label(graph: &Graph, c: NodeId) -> Label {
    let node = graph.node(c);
    let tint = node.kind.tint();
    Label {
        text: format!("▸ {}  ({})", node.name, node.kind.name()),
        pos: Vec3::ZERO,
        rgb: [tint.x, tint.y, tint.z],
        alpha: 0.8,
    }
}

fn edge_glow(rel: Relation, hovered: bool) -> f32 {
    let mut g = config::EDGE_GLOW;
    if rel.is_ownership() {
        g *= config::EDGE_OWNS_BOOST;
    }
    if hovered {
        g *= config::EDGE_HOVER_BOOST;
    }
    g
}

// --- picking -------------------------------------------------------------

fn pick_placement(placements: &[Placement], o: Vec3, d: Vec3) -> Option<NodeId> {
    let mut best: Option<(f32, NodeId)> = None;
    for p in placements {
        if let Some(t) = ray_sphere(o, d, p.pos, p.radius * 1.1) {
            if best.map_or(true, |(bt, _)| t < bt) {
                best = Some((t, p.id));
            }
        }
    }
    best.map(|(_, id)| id)
}

fn ray_sphere(o: Vec3, d: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = o - center;
    let b = oc.dot(d);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let t0 = -b - s;
    let t1 = -b + s;
    if t0 > 0.0 {
        Some(t0)
    } else if t1 > 0.0 {
        Some(t1)
    } else {
        None
    }
}

fn smoothstep(x: f32, e0: f32, e1: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
