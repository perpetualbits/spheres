//! The graph model.
//!
//! A hand-authored, directed, typed graph of the "eno" project. Every node has
//! a STABLE id (its index into the fixed node array) — never keyed by the path
//! taken to reach it — so the same node reached two ways is the same node
//! (id-keyed convergence, Requirement 4).
//!
//! The graph reveals structure a file tree hides: `core`'s centrality (five
//! libraries depend on it), `carve`/`glint`'s hidden coupling (each touches two
//! worlds), and one person (`Roland`) spanning libraries and tools at once.

use glam::Vec3;

use crate::config;

pub type NodeId = usize;

/// What a node *is*. Drives material/tint (Requirement 2) and, eventually,
/// which FORM it takes.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Production,
    Library,
    Tool,
    Person,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Production => "production",
            Kind::Library => "library",
            Kind::Tool => "tool",
            Kind::Person => "person",
        }
    }

    pub fn tint(self) -> Vec3 {
        Vec3::from(match self {
            Kind::Production => config::TINT_PRODUCTION,
            Kind::Library => config::TINT_LIBRARY,
            Kind::Tool => config::TINT_TOOL,
            Kind::Person => config::TINT_PERSON,
        })
    }

    pub fn glow(self) -> f32 {
        match self {
            Kind::Production => config::GLOW_PRODUCTION,
            Kind::Library => config::GLOW_LIBRARY,
            Kind::Tool => config::GLOW_TOOL,
            Kind::Person => config::GLOW_PERSON,
        }
    }
}

/// How a node is presented. For THIS phase every node is a container, so every
/// node is a `Sphere`. `Form` is derived from kind, NOT hardcoded as an
/// assumption, because other forms are coming.
//
// TODO (later phase): leaf content (individual files) takes other forms with
// their own gestures — `Scroll`, `Book` — derived here from kind/leaf-ness.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Form {
    Sphere,
}

impl Form {
    /// Derive the form from a node's kind. Today everything is a container
    /// (sphere); this is the seam where leaf forms will branch in.
    pub fn of(_kind: Kind) -> Form {
        Form::Sphere
    }
}

/// A directed, named relation between two nodes.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Relation {
    Uses,
    PackedBy,
    DependsOn,
    Feeds,
    AuthorsFor,
    ShadersFor,
    Owns,
}

impl Relation {
    pub fn label(self) -> &'static str {
        match self {
            Relation::Uses => "uses",
            Relation::PackedBy => "packed-by",
            Relation::DependsOn => "depends-on",
            Relation::Feeds => "feeds",
            Relation::AuthorsFor => "authors-for",
            Relation::ShadersFor => "shaders-for",
            Relation::Owns => "owns",
        }
    }

    /// Colour of the link for this relation.
    pub fn color(self) -> Vec3 {
        Vec3::from(match self {
            Relation::Uses => [0.70, 0.70, 0.80],
            Relation::PackedBy => [0.80, 0.70, 0.35],
            Relation::DependsOn => [0.45, 0.65, 0.95],
            Relation::Feeds => [0.40, 0.85, 0.70],
            Relation::AuthorsFor => [0.45, 0.90, 0.55],
            Relation::ShadersFor => [0.55, 0.95, 0.60],
            Relation::Owns => [0.98, 0.45, 0.78], // people — the showpiece
        })
    }

    pub fn is_ownership(self) -> bool {
        matches!(self, Relation::Owns)
    }
}

pub struct Node {
    pub id: NodeId,
    pub name: &'static str,
    pub kind: Kind,
    /// Derived from kind. Unused while every node is a sphere; present because
    /// other forms (scroll, book) are coming and the seam must exist now.
    #[allow(dead_code)]
    pub form: Form,
    /// Fixed position in the global overview layout.
    pub pos: Vec3,
    /// Total graph degree (in + out), used to size the node.
    pub degree: u32,
    /// Data-driven ring markers, derived from the graph (see `derive_bits`).
    pub bits: u32,
}

impl Node {
    pub fn radius(&self) -> f32 {
        config::NODE_BASE_RADIUS * (1.0 + config::NODE_DEGREE_SCALE * self.degree as f32)
    }
}

#[derive(Copy, Clone)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub rel: Relation,
}

/// A neighbour of some node, with how it is related.
#[derive(Copy, Clone)]
pub struct Neighbor {
    pub id: NodeId,
    pub rel: Relation,
    /// True if the edge points FROM the focus node TO this neighbour. Kept for
    /// future arrow/direction rendering.
    #[allow(dead_code)]
    pub outgoing: bool,
}

pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    /// The hard-coded "eno" graph. Positions are authored so that core sits at
    /// the centre with its dependents ringed around it, production above, tools
    /// to the sides (each touching two worlds), and people in a front shell
    /// whose ownership edges rake across the library/tool boundary.
    pub fn eno() -> Graph {
        use Kind::*;
        use Relation::*;

        // (name, kind, position). Index = stable NodeId.
        let spec: &[(&str, Kind, [f32; 3])] = &[
            // 0: the hub, dead centre.
            ("core", Library, [0.0, 0.0, 0.0]),
            // 1-5: the five libraries that depend on core, ringed around it.
            ("crest", Library, [0.0, 3.1, 0.0]),
            ("siftr", Library, [-2.95, 0.95, 0.0]),
            ("fx", Library, [-1.82, -2.5, 0.0]),
            ("gfx", Library, [1.82, -2.5, 0.0]),
            ("io", Library, [2.95, 0.95, 0.0]),
            // 6: production, above, fanning down into the libraries.
            ("desert-monument", Production, [0.0, 5.6, -0.4]),
            // 7-9: tools, to the sides, each reaching into two worlds.
            ("carve", Tool, [-5.0, 3.4, 0.6]),
            ("glint", Tool, [4.2, -4.2, 0.6]),
            ("smolr", Tool, [3.4, 5.2, 0.6]),
            // 10-13: people, in a front shell; their edges cross groups.
            ("Roland", Person, [-6.4, 0.2, 1.8]),
            ("Simon", Person, [-5.2, -4.0, 1.8]),
            ("Elise", Person, [6.2, 2.6, 1.8]),
            ("Segher", Person, [6.0, 5.4, 1.8]),
        ];

        let mut nodes: Vec<Node> = spec
            .iter()
            .enumerate()
            .map(|(id, &(name, kind, pos))| Node {
                id,
                name,
                kind,
                form: Form::of(kind),
                pos: Vec3::from(pos),
                degree: 0,
                bits: 0,
            })
            .collect();

        // Resolve names → ids for the edge list.
        let idx = |name: &str| spec.iter().position(|n| n.0 == name).expect("known node");
        let e = |from: &str, rel: Relation, to: &str| Edge {
            from: idx(from),
            to: idx(to),
            rel,
        };

        let edges = vec![
            // desert-monument uses the libraries...
            e("desert-monument", Uses, "crest"),
            e("desert-monument", Uses, "siftr"),
            e("desert-monument", Uses, "fx"),
            e("desert-monument", Uses, "gfx"),
            e("desert-monument", Uses, "core"),
            e("desert-monument", Uses, "io"),
            // ...and is packed by a tool.
            e("desert-monument", PackedBy, "smolr"),
            // the libraries all depend on core (the hub).
            e("crest", DependsOn, "core"),
            e("siftr", DependsOn, "core"),
            e("fx", DependsOn, "core"),
            e("gfx", DependsOn, "core"),
            e("io", DependsOn, "core"),
            // a hidden coupling a file tree wouldn't surface.
            e("siftr", Feeds, "fx"),
            // tools touching two worlds each.
            e("carve", AuthorsFor, "crest"),
            e("carve", AuthorsFor, "desert-monument"),
            e("glint", ShadersFor, "gfx"),
            e("glint", ShadersFor, "desert-monument"),
            // people — first-class, spanning groups.
            e("Roland", Owns, "core"),
            e("Roland", Owns, "crest"),
            e("Roland", Owns, "carve"),
            e("Roland", Owns, "glint"),
            e("Simon", Owns, "siftr"),
            e("Simon", Owns, "fx"),
            e("Elise", Feeds, "io"),
            e("Elise", Feeds, "siftr"),
            e("Segher", Owns, "smolr"),
        ];

        // Degree (in + out).
        for edge in &edges {
            nodes[edge.from].degree += 1;
            nodes[edge.to].degree += 1;
        }

        let graph = Graph { nodes, edges };
        // Data-driven ring markers, derived from the resolved graph.
        let bits: Vec<u32> = (0..graph.nodes.len()).map(|id| graph.derive_bits(id)).collect();
        let mut graph = graph;
        for (id, b) in bits.into_iter().enumerate() {
            graph.nodes[id].bits = b;
        }
        graph
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn production(&self) -> NodeId {
        self.nodes
            .iter()
            .position(|n| n.kind == Kind::Production)
            .unwrap_or(0)
    }

    /// All neighbours of a node, in a STABLE order (by neighbour id), so the
    /// local layout is identical every visit — id-keyed convergence.
    pub fn neighborhood(&self, id: NodeId) -> Vec<Neighbor> {
        let mut out = Vec::new();
        for e in &self.edges {
            if e.from == id {
                out.push(Neighbor {
                    id: e.to,
                    rel: e.rel,
                    outgoing: true,
                });
            } else if e.to == id {
                out.push(Neighbor {
                    id: e.from,
                    rel: e.rel,
                    outgoing: false,
                });
            }
        }
        // De-dup (a pair can share more than one edge) keeping first relation,
        // then sort by id for a stable arrangement.
        out.sort_by_key(|n| n.id);
        out.dedup_by_key(|n| n.id);
        out
    }

    /// Derive the ring marker bits from the graph itself, so the ring readout
    /// *means* something. Bit layout (RING_MARKER_COUNT segments):
    ///   bit 0 — owned (some person owns this)
    ///   bit 1 — depends on core
    ///   bit 2 — hub (in-degree >= 3)
    ///   bit 3 — cross-cutting (touches >= 3 distinct kinds)
    fn derive_bits(&self, id: NodeId) -> u32 {
        let mut bits = 0u32;

        let owned = self
            .edges
            .iter()
            .any(|e| e.to == id && e.rel == Relation::Owns);
        if owned {
            bits |= 1 << 0;
        }

        let core = 0; // core is node 0
        let depends_core = self
            .edges
            .iter()
            .any(|e| e.from == id && e.to == core && e.rel == Relation::DependsOn);
        if depends_core {
            bits |= 1 << 1;
        }

        let in_degree = self.edges.iter().filter(|e| e.to == id).count();
        if in_degree >= 3 {
            bits |= 1 << 2;
        }

        let mut kinds = std::collections::HashSet::new();
        for nb in self.neighborhood(id) {
            kinds.insert(self.nodes[nb.id].kind);
        }
        if kinds.len() >= 3 {
            bits |= 1 << 3;
        }

        bits
    }
}
