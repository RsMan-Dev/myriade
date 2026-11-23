//! Vanilla's A* pathfinder (`PathFinder`, `BinaryHeap`, `Node`, `Path`) with
//! the walking, swimming, flying and amphibious node evaluators
//! (`WalkNodeEvaluator`, `SwimNodeEvaluator`, `FlyNodeEvaluator`,
//! `AmphibiousNodeEvaluator`), ported rule for rule.
use super::{
    MobWorld,
    data::{self, PathComputationType},
};
use crate::blocks::Fluid;
use crate::fast_hash::FxHashMap;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::position::BlockPos;

/// The node kinds.
pub use myriade_types::PathType;
pub const PATH_TYPES: usize = PathType::COUNT;

thread_local! {
    /// The answer of a neighbour block of each state (`0` not asked, `1` none, `2 + type`), and the states asked.
    static ADJACENT: std::cell::RefCell<(Vec<u8>, Vec<u32>)> = const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
    /// A finder's query is running on this thread (tags and block data cannot change under it).
    static IN_QUERY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static REFERENCE_FINDER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Runs `f` with this thread's finders working as before the fast paths: the reference of the equality tests.
pub fn with_reference_finder<T>(f: impl FnOnce() -> T) -> T {
    let before = REFERENCE_FINDER.with(|r| r.replace(true));
    let out = f();
    REFERENCE_FINDER.with(|r| r.set(before));
    out
}

impl MobWorld<'_> {
    /// What a neighbour in `state` gives a node (`checkNeighbourBlocks`), when no mod part is asked: its default
    /// answer, else water's border, else the type its own path type maps to.
    pub(super) fn adjacent_of_state(&self, state: horizon_world::registry::BlockStateId) -> Option<PathType> {
        let facts = self.facts.get(state);
        let default = if facts.name.as_ref() == "minecraft:sweet_berry_bush" {
            Some(PathType::DamagingInNeighbor)
        } else if data::vanilla_burning_block(&facts.name, facts.emission > 0, self.has_tag("minecraft:fire", &facts.name)) {
            Some(PathType::FireInNeighbor)
        } else {
            None
        };
        default
            .or_else(|| matches!(facts.fluid, Fluid::Water | Fluid::Modded).then_some(PathType::WaterBorder))
            .or_else(|| data::vanilla_adjacent(self.data.path_type(state)))
    }
    /// `check_neighbours` inside a finder's query, with no mod hooks: a neighbour's answer depends on its state alone,
    /// so it is worked out once per state per query.
    fn check_neighbours_cached(&self, x: i32, y: i32, z: i32, t: PathType) -> PathType {
        ADJACENT.with(|cell| {
            let (table, touched) = &mut *cell.borrow_mut();
            // The answer code of a state (`1` none, `2 + type`), worked out once.
            let mut answer = |id: u32| -> u8 {
                let i = id as usize;
                if i >= table.len() {
                    table.resize(i + 1, 0);
                }
                if table[i] == 0 {
                    table[i] = self.adjacent_of_state(horizon_world::registry::BlockStateId(id)).map_or(1, |t| 2 + t as u8);
                    touched.push(id);
                }
                table[i]
            };
            if !self.neighbourhood_may_answer(x, y, z, &mut |id| answer(id) > 1) {
                return t;
            }
            let mut column: ((i32, i32), Option<&myriade_level::chunk::Chunk>) = ((i32::MIN, i32::MIN), None);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if dx == 0 && dz == 0 {
                            continue;
                        }
                        let (nx, ny, nz) = (x.saturating_add(dx), y.saturating_add(dy), z.saturating_add(dz));
                        // `World::block`, the column looked up once for the cells in the same chunk.
                        let key = (nx >> 4, nz >> 4);
                        if column.0 != key {
                            column = (key, self.world.chunks.get(&key).map(|c| &**c));
                        }
                        let state = column.1.map_or(self.world.air, |chunk| chunk.block(nx & 15, ny, nz & 15));
                        let code = answer(state.0);
                        if code > 1 {
                            return PathType::ALL[usize::from(code - 2)];
                        }
                    }
                }
            }
            t
        })
    }
    /// Whether any block of the 3 x 3 x 3 cells around (x, y, z) may give a neighbour answer, told from the
    /// palettes of the sections they lie in (a palette of the whole registry, or a section outside the column,
    /// may). False: none of them does, and none of the 26 blocks need be read.
    fn neighbourhood_may_answer(&self, x: i32, y: i32, z: i32, some: &mut impl FnMut(u32) -> bool) -> bool {
        let (x0, x1, y0, y1, z0, z1) = (x.saturating_sub(1), x.saturating_add(1), y.saturating_sub(1), y.saturating_add(1), z.saturating_sub(1), z.saturating_add(1));
        for cx in (x0 >> 4)..=(x1 >> 4) {
            for cz in (z0 >> 4)..=(z1 >> 4) {
                let Some(chunk) = self.world.chunks.get(&(cx, cz)) else {
                    if some(self.world.air.0) {
                        return true;
                    }
                    continue;
                };
                // Heights outside the column read as state 0.
                if (y0 < chunk.min_y() || y1 >= chunk.max_y()) && some(0) {
                    return true;
                }
                let (lo, hi) = (y0.max(chunk.min_y()), y1.min(chunk.max_y() - 1));
                if lo > hi {
                    continue;
                }
                for s in ((lo - chunk.min_y()) >> 4)..=((hi - chunk.min_y()) >> 4) {
                    let blocks = &chunk.sections[s as usize].blocks;
                    if blocks.is_global() || blocks.maybe_has(|v| some(v)) {
                        return true;
                    }
                }
            }
        }
        false
    }
    /// WalkNodeEvaluator.getFloorLevel(level, pos). Saturating, as the
    /// neighbour step is: a follower asking under a node at `i32::MIN` lands on
    /// the same block instead of overflowing (arismoko/horizon#4443).
    pub fn floor_level(&self, x: i32, y: i32, z: i32) -> f64 {
        let below = y.saturating_sub(1);
        f64::from(below) + f64::from(self.data.state(self.world.block(x, below, z)).top)
    }
}

/// The parts of a mob the node evaluators read (`Mob` accessors at prepare time).
#[derive(Clone, Debug)]
pub struct PathMob {
    pub position: Vec3,
    pub width: f32,
    pub height: f32,
    pub on_ground: bool,
    pub in_water: bool,
    pub max_up_step: f32,
    pub max_fall_distance: i32,
    pub malus: [Option<f32>; PATH_TYPES],
    pub can_float: bool,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    pub can_walk_over_fences: bool,
    /// Striders stand on lava (`canStandOnFluid`).
    pub stands_on_lava: bool,
}
impl PathMob {
    pub fn malus(&self, t: PathType) -> f32 {
        self.malus[t as usize].unwrap_or_else(|| t.malus())
    }
    pub fn bounds(&self) -> Aabb {
        let w = f64::from(self.width) / 2.0;
        Aabb::new(
            Vec3::new(self.position.x - w, self.position.y, self.position.z - w),
            Vec3::new(
                self.position.x + w,
                self.position.y + f64::from(self.height),
                self.position.z + w,
            ),
        )
    }
    fn block_position(&self) -> [i32; 3] {
        [
            self.position.x.floor() as i32,
            self.position.y.floor() as i32,
            self.position.z.floor() as i32,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evaluator {
    Walk,
    Swim { breach: bool },
    Fly,
    Amphibious { prefers_shallow: bool },
}

#[derive(Clone, Debug)]
pub struct Node {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    heap: i32,
    g: f32,
    h: f32,
    f: f32,
    came_from: Option<u32>,
    pub closed: bool,
    walked: f32,
    pub cost_malus: f32,
    pub kind: PathType,
}
impl Node {
    fn new(x: i32, y: i32, z: i32) -> Self {
        Self {
            x,
            y,
            z,
            heap: -1,
            g: 0.0,
            h: 0.0,
            f: 0.0,
            came_from: None,
            closed: false,
            walked: 0.0,
            cost_malus: 0.0,
            kind: PathType::Blocked,
        }
    }
    fn distance(&self, x: i32, y: i32, z: i32) -> f32 {
        let (dx, dy, dz) = (
            (x - self.x) as f32,
            (y - self.y) as f32,
            (z - self.z) as f32,
        );
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
    fn manhattan(&self, x: i32, y: i32, z: i32) -> f32 {
        ((x - self.x).abs() + (y - self.y).abs() + (z - self.z).abs()) as f32
    }
}

/// A computed path: node positions, the requested target and whether it was reached.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub nodes: Vec<PathNode>,
    pub next: usize,
    pub target: [i32; 3],
    pub dist_to_target: f32,
    pub reached: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathNode {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub kind: PathType,
}
impl Path {
    fn new(nodes: Vec<PathNode>, target: [i32; 3], reached: bool) -> Self {
        let dist_to_target = nodes.last().map_or(f32::MAX, |n| {
            ((n.x - target[0]).abs() + (n.y - target[1]).abs() + (n.z - target[2]).abs()) as f32
        });
        Self {
            nodes,
            next: 0,
            target,
            dist_to_target,
            reached,
        }
    }
    pub fn is_done(&self) -> bool {
        self.next >= self.nodes.len()
    }
    pub fn advance(&mut self) {
        self.next += 1;
    }
    pub fn end(&self) -> Option<PathNode> {
        self.nodes.last().copied()
    }
    pub fn next_node(&self) -> PathNode {
        self.nodes[self.next]
    }
    /// Path.getEntityPosAtNode: node corner plus half the mob's block width.
    pub fn entity_pos_at(&self, index: usize, width: f32) -> Vec3 {
        let n = self.nodes[index];
        let half = f64::from((width + 1.0) as i32) * 0.5;
        Vec3::new(f64::from(n.x) + half, f64::from(n.y), f64::from(n.z) + half)
    }
    pub fn same_as(&self, other: Option<&Path>) -> bool {
        other.is_some_and(|o| o.nodes == self.nodes)
    }
    /// `PathNavigation.trimPath`: a node standing on a cauldron
    /// (`BlockTags.CAULDRONS`) is raised onto its rim, and the node after it is
    /// raised to the rim's height too when it is not above the cauldron, so a
    /// route that ends on a cauldron (a leatherworker's job site) aims at the
    /// top face instead of into the block.
    pub fn raise_cauldron_nodes(&mut self, is_cauldron: impl Fn(i32, i32, i32) -> bool) {
        for i in 0..self.nodes.len() {
            let node = self.nodes[i];
            if !is_cauldron(node.x, node.y, node.z) {
                continue;
            }
            self.nodes[i].y = node.y.saturating_add(1);
            if let Some(&next) = self.nodes.get(i + 1)
                && node.y >= next.y
            {
                self.nodes[i + 1] = PathNode {
                    y: node.y.saturating_add(1),
                    ..next
                };
            }
        }
    }
}

struct Target {
    x: i32,
    y: i32,
    z: i32,
    best_h: f32,
    best: Option<u32>,
    reached: bool,
    pos: [i32; 3],
}

/// Reusable allocation buffers for pathfinding queries.
#[derive(Default)]
pub struct FinderBuffers {
    pub nodes: Vec<Node>,
    pub index: FxHashMap<(i32, i32, i32), u32>,
    pub heap: Vec<u32>,
    pub types: FxHashMap<(i32, i32, i32), PathType>,
    pub statics: std::cell::RefCell<FxHashMap<(i32, i32, i32), PathType>>,
    pub collisions: FxHashMap<[u64; 6], bool>,
}

impl FinderBuffers {
    pub fn new() -> Self {
        Self {
            nodes: Vec::with_capacity(256),
            index: FxHashMap::default(),
            heap: Vec::with_capacity(256),
            types: FxHashMap::default(),
            statics: std::cell::RefCell::new(FxHashMap::default()),
            collisions: FxHashMap::default(),
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.index.clear();
        self.heap.clear();
        self.types.clear();
        self.statics.borrow_mut().clear();
        self.collisions.clear();
    }
}

thread_local! {
    static TLS_BUFFERS: std::cell::RefCell<FinderBuffers> = std::cell::RefCell::new(FinderBuffers::new());
}

/// One pathfinding query: the node arena, open set and per-query caches.
pub struct Finder<'a, 'w> {
    w: &'a MobWorld<'w>,
    mob: &'a PathMob,
    eval: Evaluator,
    nodes: Vec<Node>,
    index: FxHashMap<(i32, i32, i32), u32>,
    heap: Vec<u32>,
    types: FxHashMap<(i32, i32, i32), PathType>,
    /// `path_type` of each position asked during this query: it reads up to
    /// 26 blocks, and overlapping mob boxes ask for the same positions.
    statics: std::cell::RefCell<FxHashMap<(i32, i32, i32), PathType>>,
    collisions: FxHashMap<[u64; 6], bool>,
    entity_width: i32,
    entity_height: i32,
    entity_depth: i32,
}

impl<'a, 'w> Drop for Finder<'a, 'w> {
    fn drop(&mut self) {
        IN_QUERY.with(|q| q.set(false));
        TLS_BUFFERS.with(|cell| {
            if let Ok(mut buf) = cell.try_borrow_mut() {
                self.nodes.clear();
                self.index.clear();
                self.heap.clear();
                self.types.clear();
                self.statics.borrow_mut().clear();
                self.collisions.clear();
                buf.nodes = std::mem::take(&mut self.nodes);
                buf.index = std::mem::take(&mut self.index);
                buf.heap = std::mem::take(&mut self.heap);
                buf.types = std::mem::take(&mut self.types);
                buf.statics = std::mem::take(&mut self.statics);
                buf.collisions = std::mem::take(&mut self.collisions);
            }
        });
    }
}

/// Classify one swim cell without reading the block below when it already
/// contains water. The below-block lookup is only needed for a breach.
#[inline]
fn swim_cell_type(
    fluid: Fluid,
    below_water: impl FnOnce() -> bool,
    is_air: impl FnOnce() -> bool,
) -> Option<PathType> {
    if fluid == Fluid::Water {
        return None;
    }
    if fluid == Fluid::None && below_water() && is_air() {
        Some(PathType::Breach)
    } else {
        Some(PathType::Blocked)
    }
}

impl<'a, 'w> Finder<'a, 'w> {
    pub fn new(w: &'a MobWorld<'w>, mob: &'a PathMob, eval: Evaluator) -> Self {
        ADJACENT.with(|cell| {
            let (table, touched) = &mut *cell.borrow_mut();
            for id in touched.drain(..) {
                table[id as usize] = 0;
            }
        });
        IN_QUERY.with(|q| q.set(true));
        let (nodes, index, heap, types, statics, collisions) = TLS_BUFFERS.with(|cell| {
            if let Ok(mut buf) = cell.try_borrow_mut() {
                buf.clear();
                (
                    std::mem::take(&mut buf.nodes),
                    std::mem::take(&mut buf.index),
                    std::mem::take(&mut buf.heap),
                    std::mem::take(&mut buf.types),
                    std::mem::take(&mut buf.statics),
                    std::mem::take(&mut buf.collisions),
                )
            } else {
                (
                    Vec::with_capacity(128),
                    FxHashMap::default(),
                    Vec::with_capacity(128),
                    FxHashMap::default(),
                    std::cell::RefCell::new(FxHashMap::default()),
                    FxHashMap::default(),
                )
            }
        });
        Self {
            w,
            mob,
            eval,
            nodes,
            index,
            heap,
            types,
            statics,
            collisions,
            entity_width: (mob.width + 1.0).floor() as i32,
            entity_height: (mob.height + 1.0).floor() as i32,
            entity_depth: (mob.width + 1.0).floor() as i32,
        }
    }
    /// Mob.getPathfindingMalus; AmphibiousNodeEvaluator.prepare overrides three
    /// maluses for the duration of the query.
    fn malus(&self, t: PathType) -> f32 {
        if let Evaluator::Amphibious { .. } = self.eval {
            match t {
                PathType::Water => return 0.0,
                PathType::Walkable => return 6.0,
                PathType::WaterBorder => return 4.0,
                _ => {}
            }
        }
        self.mob.malus(t)
    }
    // --- world access -------------------------------------------------
    fn state(&self, x: i32, y: i32, z: i32) -> data::StateData {
        self.w.data.state(self.w.world.block(x, y, z))
    }
    fn pathfindable(&self, x: i32, y: i32, z: i32, kind: PathComputationType) -> bool {
        self.w.data.pathfindable(self.w.world.block(x, y, z), kind)
    }
    fn fluid(&self, x: i32, y: i32, z: i32) -> Fluid {
        self.w.facts.get(self.w.world.block(x, y, z)).fluid
    }
    fn is_air(&self, x: i32, y: i32, z: i32) -> bool {
        self.w.facts.is_air(self.w.world.block(x, y, z))
    }
    /// WalkNodeEvaluator.getFloorLevel(level, pos).
    pub fn static_floor(&self, x: i32, y: i32, z: i32) -> f64 {
        let below = y.saturating_sub(1);
        f64::from(below) + f64::from(self.state(x, below, z).top)
    }
    fn floor_level(&self, x: i32, y: i32, z: i32) -> f64 {
        let amphibious = matches!(self.eval, Evaluator::Amphibious { .. });
        if (self.mob.can_float || amphibious) && self.fluid(x, y, z) == Fluid::Water {
            f64::from(y) + 0.5
        } else {
            self.static_floor(x, y, z)
        }
    }
    fn swim_path_type(&self, x: i32, y: i32, z: i32) -> PathType {
        let mut last = (x, y, z);
        for xx in x..x.saturating_add(self.entity_width) {
            for yy in y..y.saturating_add(self.entity_height) {
                for zz in z..z.saturating_add(self.entity_depth) {
                    let fluid = self.fluid(xx, yy, zz);
                    if let Some(t) = swim_cell_type(
                        fluid,
                        || {
                            self.pathfindable(
                                xx,
                                yy.saturating_sub(1),
                                zz,
                                PathComputationType::Water,
                            )
                        },
                        || self.is_air(xx, yy, zz),
                    ) {
                        return t;
                    }
                    last = (xx, yy, zz);
                }
            }
        }
        if self.pathfindable(last.0, last.1, last.2, PathComputationType::Water) {
            PathType::Water
        } else {
            PathType::Blocked
        }
    }
    // --- node arena -------------------------------------------------------
    fn node(&mut self, x: i32, y: i32, z: i32) -> u32 {
        if let Some(i) = self.index.get(&(x, y, z)) {
            return *i;
        }
        let i = self.nodes.len() as u32;
        self.nodes.push(Node::new(x, y, z));
        self.index.insert((x, y, z), i);
        i
    }
    // --- walking neighbours ----------------------------------------------
    fn node_update_max(&mut self, x: i32, y: i32, z: i32, t: PathType, cost: f32) -> u32 {
        let n = self.node(x, y, z);
        let node = &mut self.nodes[n as usize];
        node.kind = t;
        node.cost_malus = node.cost_malus.max(cost);
        n
    }
    fn blocked_node(&mut self, x: i32, y: i32, z: i32) -> u32 {
        let n = self.node(x, y, z);
        let node = &mut self.nodes[n as usize];
        node.kind = PathType::Blocked;
        node.cost_malus = -1.0;
        n
    }
    fn closed_node(&mut self, x: i32, y: i32, z: i32, t: PathType) -> u32 {
        let n = self.node(x, y, z);
        let node = &mut self.nodes[n as usize];
        node.closed = true;
        node.kind = t;
        node.cost_malus = t.malus();
        n
    }
    fn jump_height(&self) -> f64 {
        f64::from(self.mob.max_up_step).max(1.125)
    }
    fn partial_collision(t: PathType) -> bool {
        matches!(
            t,
            PathType::Fence | PathType::DoorWoodClosed | PathType::DoorIronClosed
        )
    }
    fn neighbor_valid(&self, n: Option<u32>, current: u32) -> bool {
        n.is_some_and(|n| {
            let node = &self.nodes[n as usize];
            !node.closed
                && (node.cost_malus >= 0.0 || self.nodes[current as usize].cost_malus < 0.0)
        })
    }
    fn diagonal_pair_valid(&self, pos: u32, ew: Option<u32>, ns: Option<u32>) -> bool {
        let (Some(ew), Some(ns)) = (ew, ns) else {
            return false;
        };
        let (p, e, n) = (
            &self.nodes[pos as usize],
            &self.nodes[ew as usize],
            &self.nodes[ns as usize],
        );
        if n.y > p.y || e.y > p.y {
            return false;
        }
        if e.kind == PathType::WalkableDoor || n.kind == PathType::WalkableDoor {
            return false;
        }
        if self.mob.width > 1.0 && (e.cost_malus > 0.0 || n.cost_malus > 0.0) {
            return false;
        }
        let between_posts = n.kind == PathType::Fence
            && e.kind == PathType::Fence
            && f64::from(self.mob.width) < 0.5;
        (n.y < p.y || n.cost_malus >= 0.0 || between_posts)
            && (e.y < p.y || e.cost_malus >= 0.0 || between_posts)
    }
    fn diagonal_valid(&self, d: Option<u32>) -> bool {
        d.is_some_and(|d| {
            let n = &self.nodes[d as usize];
            !n.closed && n.kind != PathType::WalkableDoor && n.cost_malus >= 0.0
        })
    }
    // --- binary heap (vanilla BinaryHeap) --------------------------------
    fn heap_insert(&mut self, n: u32) {
        let i = self.heap.len();
        self.heap.push(n);
        self.nodes[n as usize].heap = i as i32;
        self.up_heap(i);
    }
    fn heap_pop(&mut self) -> u32 {
        let popped = self.heap[0];
        let last = self.heap.pop().unwrap_or(popped);
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.nodes[last as usize].heap = 0;
            self.down_heap(0);
        }
        self.nodes[popped as usize].heap = -1;
        popped
    }
    fn change_cost(&mut self, n: u32, cost: f32) {
        let old = self.nodes[n as usize].f;
        self.nodes[n as usize].f = cost;
        let i = self.nodes[n as usize].heap as usize;
        if cost < old {
            self.up_heap(i);
        } else {
            self.down_heap(i);
        }
    }
    fn up_heap(&mut self, mut i: usize) {
        let node = self.heap[i];
        let cost = self.nodes[node as usize].f;
        while i > 0 {
            let parent_i = (i - 1) >> 1;
            let parent = self.heap[parent_i];
            if cost >= self.nodes[parent as usize].f {
                break;
            }
            self.heap[i] = parent;
            self.nodes[parent as usize].heap = i as i32;
            i = parent_i;
        }
        self.heap[i] = node;
        self.nodes[node as usize].heap = i as i32;
    }
    fn down_heap(&mut self, mut i: usize) {
        let node = self.heap[i];
        let cost = self.nodes[node as usize].f;
        let size = self.heap.len();
        loop {
            let left_i = 1 + (i << 1);
            let right_i = left_i + 1;
            if left_i >= size {
                break;
            }
            let left = self.heap[left_i];
            let left_cost = self.nodes[left as usize].f;
            let (right, right_cost) = if right_i >= size {
                (u32::MAX, f32::INFINITY)
            } else {
                (
                    self.heap[right_i],
                    self.nodes[self.heap[right_i] as usize].f,
                )
            };
            if left_cost < right_cost {
                if left_cost >= cost {
                    break;
                }
                self.heap[i] = left;
                self.nodes[left as usize].heap = i as i32;
                i = left_i;
            } else {
                if right_cost >= cost {
                    break;
                }
                self.heap[i] = right;
                self.nodes[right as usize].heap = i as i32;
                i = right_i;
            }
        }
        self.heap[i] = node;
        self.nodes[node as usize].heap = i as i32;
    }
    // --- PathFinder.findPath ------------------------------------------------
    fn best_h(&self, n: u32, targets: &mut [Target]) -> f32 {
        let mut best = f32::MAX;
        let node = &self.nodes[n as usize];
        for t in targets.iter_mut() {
            let h = node.distance(t.x, t.y, t.z);
            if h < t.best_h {
                t.best_h = h;
                t.best = Some(n);
            }
            best = best.min(h);
        }
        best
    }
}

// The smallest natural mob mean size (baby bee: 0.2666...) at minimum
// minecraft:scale (0.0625) still exceeds 1/64. Preserve vanilla subdivision
// counts above that conservative boundary, even for long airborne-start legs.
// Only smaller mod boxes need a work cap to avoid a saturated i32 loop.
fn collision_steps(length: f64, box_size: f64) -> i32 {
    if !box_size.is_finite() || box_size <= 0.0 {
        return 1;
    }
    let steps = (length / box_size).ceil().max(1.0);
    if box_size < 1.0 / 64.0 {
        steps.min(16_384.0) as i32
    } else {
        steps as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_subdivision_preserves_small_vanilla_boxes_and_bounds_pathological_ones() {
        // Minimum-scale silverfish, west candidate from (0.99, 70, 0.921591).
        let width = f64::from(0.4_f32 * 0.0625);
        let height = f64::from(0.3_f32 * 0.0625);
        let delta = Vec3::new(
            -1.0 - 0.99 + width / 2.0,
            height / 2.0,
            -0.921591 + width / 2.0,
        );
        let length = (delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt();
        let size = (2.0 * width + height) / 3.0;
        assert_eq!((length / size).ceil() as i32, 95);
        assert_eq!(collision_steps(length, size), 95);
        assert_eq!(collision_steps(1.0, 1e-30), 16_384);
        assert_eq!(collision_steps(1.0, f64::from(f32::from_bits(1))), 16_384);
        // Vanilla-size boxes keep the original formula, even above the work cap.
        let boundary = 1.0_f64 / 64.0;
        let below = f64::from_bits(boundary.to_bits() - 1);
        let above = f64::from_bits(boundary.to_bits() + 1);
        for size in [boundary, above] {
            assert_eq!(collision_steps(512.0, size), (512.0 / size).ceil() as i32);
        }
        assert_eq!(collision_steps(512.0, below), 16_384);
        // Pin both sides of the exact sub-vanilla work limit.
        assert_eq!(collision_steps(16_383.0 / 128.0, 1.0 / 128.0), 16_383);
        assert_eq!(collision_steps(16_383.5 / 128.0, 1.0 / 128.0), 16_384);
        assert_eq!(collision_steps(16_384.5 / 128.0, 1.0 / 128.0), 16_384);
        for size in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(collision_steps(1.0, size), 1);
        }
    }

    #[test]
    fn swim_cells_only_check_below_for_missing_fluid() {
        use std::cell::Cell;

        let below_reads = Cell::new(0);
        let below_water = || {
            below_reads.set(below_reads.get() + 1);
            true
        };
        assert_eq!(swim_cell_type(Fluid::Water, below_water, || false), None);
        assert_eq!(below_reads.get(), 0);

        assert_eq!(
            swim_cell_type(Fluid::None, below_water, || true),
            Some(PathType::Breach)
        );
        assert_eq!(below_reads.get(), 1);

        assert_eq!(
            swim_cell_type(Fluid::None, || false, || panic!("not queried")),
            Some(PathType::Blocked)
        );
        assert_eq!(
            swim_cell_type(
                Fluid::Lava,
                || panic!("not queried"),
                || panic!("not queried")
            ),
            Some(PathType::Blocked)
        );
        assert_eq!(below_reads.get(), 1);
    }

    fn node(x: i32, y: i32, z: i32) -> PathNode {
        PathNode {
            x,
            y,
            z,
            kind: PathType::Open,
        }
    }

    fn path(nodes: Vec<PathNode>) -> Path {
        Path {
            nodes,
            next: 0,
            target: [0, 0, 0],
            dist_to_target: 0.0,
            reached: true,
        }
    }

    /// `PathNavigation.trimPath`: the node standing on a cauldron moves onto its
    /// rim, and the node after it follows when it is not above the cauldron.
    #[test]
    fn a_node_on_a_cauldron_is_raised_onto_its_rim() {
        let mut p = path(vec![
            node(0, 63, 0),
            node(1, 63, 0),
            node(2, 63, 0),
            node(2, 66, 0),
        ]);
        p.raise_cauldron_nodes(|x, y, _| x == 2 && y == 63);
        assert_eq!(p.nodes[0], node(0, 63, 0), "a plain node keeps its height");
        assert_eq!(p.nodes[1], node(1, 63, 0));
        assert_eq!(
            p.nodes[2],
            node(2, 64, 0),
            "the cauldron node is raised one block"
        );
        assert_eq!(
            p.nodes[3],
            node(2, 66, 0),
            "a node above the cauldron is left alone"
        );
        assert_eq!(p.target, [0, 0, 0], "the requested target does not move");
    }

    /// The node after a cauldron is raised with it unless it is above it, so a
    /// route that steps off a cauldron does not drop back into the block.
    #[test]
    fn the_node_after_a_cauldron_follows_it_up() {
        let mut p = path(vec![node(0, 63, 0), node(1, 63, 0)]);
        p.raise_cauldron_nodes(|_, y, _| y == 63);
        // The closure still reports the original height, as the block under the
        // raised node is the cauldron itself.
        assert_eq!(p.nodes, vec![node(0, 64, 0), node(1, 64, 0)]);
    }

    /// A path with no cauldron under it is not touched.
    #[test]
    fn a_path_without_cauldrons_is_unchanged() {
        let nodes = vec![node(0, 64, 0), node(1, 64, 0), node(1, 65, 0)];
        let mut p = path(nodes.clone());
        p.raise_cauldron_nodes(|_, _, _| false);
        assert_eq!(p.nodes, nodes);
    }
}
