//! The flight graph of the dragon (`D:2054-2244`): 24 nodes built once per fight, held as a resource. The rules (the layout, the closest
//! node, the A* over the adjacency masks, the holding ring and the strafe's first leg) are in `rules/dragon_nodes.rs`; this file
//! keeps the resource and hands its nodes to them. Nothing here names the dragon: `LAYOUT` is the End's.
use crate::rules::dragon_nodes as rule;
use bevy_ecs::prelude::Resource;
use horizon_core::geometry::Vec3;
use horizon_core::random::LegacyRandom;

pub use crate::rules::dragon_nodes::{Layout, Path, Ring, LAYOUT};

/// The nodes of one fight, and the top of the pillar (`podium_top`, the landing's point).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct FlightNodes {
    pub nodes: [[i32; 3]; 24],
    pub pillar_top: i32,
}

impl FlightNodes {
    /// `ground(x, z)` is `MOTION_BLOCKING_NO_LEAVES` at that column.
    pub fn build(layout: &Layout, ground: impl Fn(i32, i32) -> i32) -> Self {
        let nodes = rule::build_nodes(layout, &ground);
        Self { nodes, pillar_top: ground(0, 0) }
    }

    /// `podium_top`: the centre of the pillar's top.
    pub fn podium(&self) -> Vec3 {
        Vec3::new(0.5, f64::from(self.pillar_top), 0.5)
    }

    /// `closest_node`: the nearest node to `p` (the first on ties). Without crystals the inner ring is out (start at 12).
    pub fn closest(&self, crystals_alive: u16, p: Vec3) -> usize {
        rule::closest_node(&self.nodes, crystals_alive, p)
    }
}

impl Path {
    pub fn next_pos(&self, g: &FlightNodes) -> [i32; 3] {
        self.next_node(&g.nodes)
    }
}

/// `findPath` (D:2111), see `rule::find_path`.
pub fn find_path(g: &FlightNodes, crystals_alive: u16, from: usize, to: usize, last: Option<[i32; 3]>) -> Option<Path> {
    rule::find_path(&g.nodes, crystals_alive, from, to, last)
}

/// `ring_path` (D:2192), see `rule::ring_path`.
pub fn ring_path(g: &FlightNodes, crystals_alive: u16, current: usize, flip: bool, clockwise: &mut bool) -> Option<Path> {
    rule::ring_path(&g.nodes, crystals_alive, current, flip, clockwise)
}

/// `next_target` (D:2175), see `rule::next_target`.
pub fn next_target(path: &mut Option<Path>, g: &FlightNodes, rng: &mut LegacyRandom) -> Option<Vec3> {
    rule::next_target(path, &g.nodes, rng)
}

/// `DragonStrafePlayerPhase.setTarget` (D:2215), see `rule::strafe_begin`.
pub fn strafe_begin(g: &FlightNodes, crystals_alive: u16, at: Vec3, p: Vec3, rng: &mut LegacyRandom) -> (Option<Path>, Option<Vec3>) {
    rule::strafe_begin(&g.nodes, crystals_alive, at, p, rng)
}
