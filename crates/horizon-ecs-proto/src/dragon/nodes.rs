//! The flight graph of the dragon (`D:2054-2244`): 24 nodes built once per fight, the closest node, the A* over the adjacency masks,
//! the holding ring and the strafe's first leg. Pure: the ground height and the crystal count come from the caller. The layout is
//! configuration: `LAYOUT` is the End's, and nothing here names the dragon.
use super::data::ADJACENCY;
use bevy_ecs::prelude::Resource;
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};
use std::f32::consts::PI;

/// `n` nodes on a circle of `radius`, `step` radians apart from `-PI` (each angle doubled), `lift` above the ground.
pub struct Ring {
    pub n: usize,
    pub radius: f32,
    pub step: f32,
    pub lift: i32,
}
/// The rings in node order, and the lowest height a node takes.
pub struct Layout {
    pub rings: &'static [Ring],
    pub floor: i32,
}
/// `dragon_nodes` (D:2054): 12 nodes on the outer ring, 8 on the middle, 4 on the inner.
pub const LAYOUT: Layout = Layout {
    rings: &[
        Ring { n: 12, radius: 60.0, step: PI / 12.0, lift: 5 },
        Ring { n: 8, radius: 40.0, step: PI / 8.0, lift: 15 },
        Ring { n: 4, radius: 20.0, step: PI / 4.0, lift: 5 },
    ],
    floor: 73,
};

/// The nodes of one fight, and the top of the pillar (`podium_top`, the landing's point).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct FlightNodes {
    pub nodes: [[i32; 3]; 24],
    pub pillar_top: i32,
}

fn dist(a: [i32; 3], b: [i32; 3]) -> f32 {
    (((a[0] - b[0]).pow(2) + (a[1] - b[1]).pow(2) + (a[2] - b[2]).pow(2)) as f32).sqrt()
}

impl FlightNodes {
    /// `ground(x, z)` is `MOTION_BLOCKING_NO_LEAVES` at that column.
    pub fn build(layout: &Layout, ground: impl Fn(i32, i32) -> i32) -> Self {
        let mut nodes = [[0; 3]; 24];
        let mut i = 0;
        for ring in layout.rings {
            for base in 0..ring.n {
                let a = 2.0 * (-PI + ring.step * base as f32);
                let (x, z) = ((ring.radius * a.cos()).floor() as i32, (ring.radius * a.sin()).floor() as i32);
                nodes[i] = [x, (ground(x, z) + ring.lift).max(layout.floor), z];
                i += 1;
            }
        }
        debug_assert_eq!(i, 24);
        Self { nodes, pillar_top: ground(0, 0) }
    }

    /// `podium_top`: the centre of the pillar's top.
    pub fn podium(&self) -> Vec3 {
        Vec3::new(0.5, f64::from(self.pillar_top), 0.5)
    }

    /// `closest_node`: the nearest node to `p` (the first on ties). Without crystals the inner ring is out (start at 12).
    pub fn closest(&self, crystals_alive: u16, p: Vec3) -> usize {
        let start = if crystals_alive == 0 { 12 } else { 0 };
        let q = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
        let mut best = (10_000.0f32, 0);
        for (i, n) in self.nodes.iter().enumerate().skip(start) {
            let d = ((n[0] - q[0]).pow(2) + (n[1] - q[1]).pow(2) + (n[2] - q[2]).pow(2)) as f32;
            if d < best.0 {
                best = (d, i);
            }
        }
        best.1
    }
}

/// A walk over nodes, then the point `last` if any (a strafe ends above its player). `next` is the walk's index.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub nodes: Vec<usize>,
    pub last: Option<[i32; 3]>,
    pub next: usize,
}

impl Path {
    pub fn total(&self) -> usize {
        self.nodes.len() + usize::from(self.last.is_some())
    }
    pub fn done(&self) -> bool {
        self.next >= self.total()
    }
    pub fn advance(&mut self) {
        self.next += 1;
    }
    pub fn next_pos(&self, g: &FlightNodes) -> [i32; 3] {
        match self.nodes.get(self.next) {
            Some(&i) => g.nodes[i],
            None => self.last.expect("path end"),
        }
    }
}

/// `findPath` (D:2111): A* over the adjacency masks from `from` to `to`. Unreachable, the path ends at the node nearest `to`. Without
/// crystals the inner ring is out. `last` is appended to the path.
pub fn find_path(g: &FlightNodes, crystals_alive: u16, from: usize, to: usize, last: Option<[i32; 3]>) -> Option<Path> {
    let min = if crystals_alive == 0 { 12 } else { 0 };
    let n = &g.nodes;
    let mut cost = [f32::MAX; 24];
    let mut came = [usize::MAX; 24];
    let mut closed = [false; 24];
    let mut open = vec![from];
    cost[from] = 0.0;
    let mut closest = from;
    while !open.is_empty() {
        let (k, &cur) = open
            .iter()
            .enumerate()
            .min_by(|a, b| (cost[*a.1] + dist(n[*a.1], n[to])).total_cmp(&(cost[*b.1] + dist(n[*b.1], n[to]))))
            .expect("open set");
        open.swap_remove(k);
        if cur == to {
            closest = to;
            break;
        }
        if dist(n[cur], n[to]) < dist(n[closest], n[to]) {
            closest = cur;
        }
        closed[cur] = true;
        for i in min..24 {
            if ADJACENCY[cur] & (1 << i) != 0 && !closed[i] {
                let t = cost[cur] + dist(n[cur], n[i]);
                if t < cost[i] {
                    cost[i] = t;
                    came[i] = cur;
                    if !open.contains(&i) {
                        open.push(i);
                    }
                }
            }
        }
    }
    if closest == from && from != to {
        return None;
    }
    let mut nodes = vec![closest];
    let mut c = closest;
    while came[c] != usize::MAX {
        c = came[c];
        nodes.insert(0, c);
    }
    Some(Path { nodes, last, next: 0 })
}

/// `ring_path` (D:2192): one node on from `current` around the pillar, `clockwise` kept, or six across when `flip` (the caller draws
/// it: `next_int_bound(8) == 0`). The path is advanced past its first node.
pub fn ring_path(g: &FlightNodes, crystals_alive: u16, current: usize, flip: bool, clockwise: &mut bool) -> Option<Path> {
    let mut target = current as i32;
    if flip {
        *clockwise = !*clockwise;
        target += 6;
    }
    target += if *clockwise { 1 } else { -1 };
    target = if crystals_alive > 0 { target.rem_euclid(12) } else { ((target - 12) & 7) + 12 };
    let mut path = find_path(g, crystals_alive, current, target as usize, None)?;
    path.advance();
    Some(path)
}

/// `next_target` (D:2175): the point of the path's next node, at a height drawn up to 20 blocks above it; advances the path.
pub fn next_target(path: &mut Option<Path>, g: &FlightNodes, rng: &mut LegacyRandom) -> Option<Vec3> {
    let p = path.as_mut()?;
    if p.done() {
        return None;
    }
    let c = p.next_pos(g);
    p.advance();
    let y = loop {
        let y = f64::from(c[1]) + f64::from(rng.next_float() * 20.0);
        if y >= f64::from(c[1]) {
            break y;
        }
    };
    Some(Vec3::new(f64::from(c[0]), y, f64::from(c[2])))
}

/// `DragonStrafePlayerPhase.setTarget` (D:2215): the path from `at` to the node by the player at `p`, ending above the player at a
/// height that grows with the distance, advanced once; and the first point to fly to.
pub fn strafe_begin(g: &FlightNodes, crystals_alive: u16, at: Vec3, p: Vec3, rng: &mut LegacyRandom) -> (Option<Path>, Option<Vec3>) {
    let current = g.closest(crystals_alive, at);
    let target = g.closest(crystals_alive, p);
    let (fx, fz) = (p.x.floor() as i32, p.z.floor() as i32);
    let sd = (f64::from(fx) - at.x).hypot(f64::from(fz) - at.z);
    let ho = (f64::from(0.4_f32) + sd / 80.0 - 1.0).min(10.0);
    let last = [fx, (p.y + ho).floor() as i32, fz];
    let mut path = find_path(g, crystals_alive, current, target, Some(last)).map(|mut q| {
        q.advance();
        q
    });
    let first = next_target(&mut path, g, rng);
    (path, first)
}
