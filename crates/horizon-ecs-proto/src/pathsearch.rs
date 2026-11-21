//! Counting copy of Horizon's `Finder` (walk evaluator included), to be reduced. See suivi/ecs-portage/relais/chemin-partage.md.
use crate::IFloor as _;
use crate::rules::pathsearch::type_on_below;
use horizon_world::registry::BlockStateId;
use horizon_client_lib::mobs::{MobWorld, data::{self, PathComputationType}, path::{Evaluator, Path, PathMob, PathNode, PathType, PATH_TYPES}};
use horizon_client_lib::blocks::Fluid;
use horizon_client_lib::fast_hash::FxHashMap;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::position::BlockPos;


#[cfg(feature = "probes")]
macro_rules! probe { ($($s:tt)*) => { $($s)* }; }
#[cfg(not(feature = "probes"))]
macro_rules! probe { ($($s:tt)*) => {}; }
pub(crate) use probe;
/// Counters of the search (bench option): `ON` set by the benches with PATHCOUNT.
#[cfg(feature = "probes")]
pub static ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "probes")]
pub const NAMES: [&str; 24] = ["opened", "neighbours_pushed", "find_accepted", "try_jump", "first_ground", "node()", "node_new", "cached()", "cached_miss", "path_type()", "path_type_miss(statics)", "static()", "from_state", "from_state_miss(world cache)", "block_read_floor", "block_read_other", "block_read_neigh", "check_neigh", "check_neigh_scan", "within_bb(alloc)", "has_collisions", "has_collisions_miss", "heap_ops(ins/pop/chg)", "heap_steps"];
#[cfg(feature = "probes")]
pub static C: [std::sync::atomic::AtomicU64; 24] = [const { std::sync::atomic::AtomicU64::new(0) }; 24];
#[cfg(feature = "probes")]
pub const SEC: [&str; 16] = ["Finder::new", "Finder drop", "find_accepted", "walk_neighbors", "heap ops", "within_bb", "loop: neighbours", "heap_pop", "find (whole)", "static_floor", "cached()", "after loop", "start()", "node()", "path_type()", "best_h"];
#[cfg(feature = "probes")]
pub static ACC: [std::sync::atomic::AtomicU64; 16] = [const { std::sync::atomic::AtomicU64::new(0) }; 16];
#[cfg(feature = "probes")]
thread_local! { static CTR: Option<crate::instr::Counter> = crate::instr::Counter::new(); }
#[cfg(feature = "probes")]
pub struct Sec(usize, u64);
#[cfg(feature = "probes")]
pub static MASK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[cfg(feature = "probes")]
pub static CNT: [std::sync::atomic::AtomicU64; 16] = [const { std::sync::atomic::AtomicU64::new(0) }; 16];
/// Instructions between two readings of the counter around nothing: the guard's own cost, to take off per guard.
#[cfg(feature = "probes")]
pub fn calibrate(i: usize) -> u64 {
    let (a, n) = (ACC[i].load(std::sync::atomic::Ordering::Relaxed), CNT[i].load(std::sync::atomic::Ordering::Relaxed));
    for _ in 0..20000 {
        #[cfg(feature = "probes")]
        let _g = sec(i);
    }
    let o = (ACC[i].load(std::sync::atomic::Ordering::Relaxed) - a) / 20000;
    ACC[i].store(a, std::sync::atomic::Ordering::Relaxed);
    CNT[i].store(n, std::sync::atomic::Ordering::Relaxed);
    o
}
/// The counters and guards exist only in a build with `PATH_PROBE` set in the environment (they cost instructions on the hot path).
#[cfg(feature = "probes")]
const REP: u32 = match option_env!("PATH_REP") { Some(v) => (v.as_bytes()[0] - b'0') as u32, None => 0 };
#[cfg(feature = "probes")]
const PROBE: bool = option_env!("PATH_PROBE").is_some();
#[cfg(feature = "probes")]
#[inline(always)]
fn sec(i: usize) -> Option<Sec> {
    if !PROBE { return None; }
    if MASK.load(std::sync::atomic::Ordering::Relaxed) & (1 << i) == 0 { return None; }
    CTR.with(|c| c.as_ref().map(|c| Sec(i, c.read())))
}
#[cfg(feature = "probes")]
impl Drop for Sec {
    fn drop(&mut self) {
        CTR.with(|c| if let Some(c) = c { ACC[self.0].fetch_add(c.read() - self.1, std::sync::atomic::Ordering::Relaxed); CNT[self.0].fetch_add(1, std::sync::atomic::Ordering::Relaxed); });
    }
}
#[cfg(feature = "probes")]
#[inline(always)]
fn bump(i: usize) {
    if PROBE && ON.load(std::sync::atomic::Ordering::Relaxed) {
        C[i].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

thread_local! {
    static ADJACENT: std::cell::RefCell<(Vec<u8>, Vec<u32>)> = const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
}
/// What a neighbour in `state` gives a node (`checkNeighbourBlocks`), when no mod part is asked: its default
/// answer, else water's border, else the type its own path type maps to.
pub fn adjacent_of_state(w: &MobWorld, state: horizon_world::registry::BlockStateId) -> Option<PathType> {
    let facts = w.facts.get(state);
    let default = if facts.name.as_ref() == "minecraft:sweet_berry_bush" {
        Some(PathType::DamagingInNeighbor)
    } else if vanilla_burning_block(&facts.name, facts.emission > 0, w.has_tag("minecraft:fire", &facts.name)) {
        Some(PathType::FireInNeighbor)
    } else {
        None
    };
    default
        .or_else(|| matches!(facts.fluid, Fluid::Water | Fluid::Modded).then_some(PathType::WaterBorder))
        .or_else(|| vanilla_adjacent(w.data.path_type(state)))
}
/// A new query: the neighbour answers worked out so far (by state, valid for one registry and one set of tags) are forgotten.
pub fn adjacent_reset() {
    ADJACENT.with(|cell| {
        let (table, touched) = &mut *cell.borrow_mut();
        for id in touched.drain(..) {
            table[id as usize] = 0;
        }
    });
}
/// `check_neighbours` inside a finder's query, with no mod hooks: a neighbour's answer depends on its state alone,
/// so it is worked out once per state per query.
pub fn check_neighbours_cached(w: &MobWorld, x: i32, y: i32, z: i32, t: PathType) -> PathType {
    ADJACENT.with(|cell| {
        let (table, touched) = &mut *cell.borrow_mut();
        // The answer code of a state (`1` none, `2 + type`), worked out once.
        let mut answer = |id: u32| -> u8 {
            let i = id as usize;
            if i >= table.len() {
                table.resize(i + 1, 0);
            }
            if table[i] == 0 {
                table[i] = adjacent_of_state(w, horizon_world::registry::BlockStateId(id)).map_or(1, |t| 2 + t as u8);
                touched.push(id);
            }
            table[i]
        };
        if !neighbourhood_may_answer(w, x, y, z, &mut |id| answer(id) > 1) {
            return t;
        }
        probe!(bump(18));
        let mut column: ((i32, i32), Option<&horizon_client_render::chunk::Chunk>) = ((i32::MIN, i32::MIN), None);
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
                        column = (key, w.world.chunks.get(&key).map(|c| &**c));
                    }
                    probe!(bump(16));
                    let state = column.1.map_or(w.world.air, |chunk| chunk.block(nx & 15, ny, nz & 15));
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
pub fn neighbourhood_may_answer(w: &MobWorld, x: i32, y: i32, z: i32, some: &mut impl FnMut(u32) -> bool) -> bool {
    let (x0, x1, y0, y1, z0, z1) = (x.saturating_sub(1), x.saturating_add(1), y.saturating_sub(1), y.saturating_add(1), z.saturating_sub(1), z.saturating_add(1));
    for cx in (x0 >> 4)..=(x1 >> 4) {
        for cz in (z0 >> 4)..=(z1 >> 4) {
            let Some(chunk) = w.world.chunks.get(&(cx, cz)) else {
                if some(w.world.air.0) {
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
// ---- the block batch ----------------------------------------------------------------------------------------
/// While a `BlockBatch` guard lives, the block states the searches read are kept in a direct table (the world does not change inside a batch:
/// one decision of one creature runs many searches over the same cells). One read of the world is some 220 instructions, a hit about 15.
struct BatchTab {
    depth: u32,
    stamp: u32,
    tab: Vec<(u64, i32, u32, u32)>,
}
thread_local! {
    static BATCH: std::cell::UnsafeCell<BatchTab> = const { std::cell::UnsafeCell::new(BatchTab { depth: 0, stamp: 0, tab: Vec::new() }) };
}
const BATCH_BITS: u32 = 13;
/// Holds the block table open (see above); drop it before anything changes a block.
pub struct BlockBatch;
impl BlockBatch {
    pub fn begin() -> Self {
        BATCH.with(|b| {
            // SAFETY: thread local, no reference is kept across calls.
            let b = unsafe { &mut *b.get() };
            if b.depth == 0 {
                if b.tab.is_empty() {
                    b.tab = vec![(0, 0, 0, 0); 1 << BATCH_BITS];
                }
                // What the searches of the last batch kept is of another moment of the world.
                TLS_BUFFERS.with(|c| {
                    if let Ok(mut buf) = c.try_borrow_mut() {
                        buf.clear();
                    }
                });
                b.stamp = b.stamp.wrapping_add(1);
                if b.stamp == 0 {
                    b.tab.iter_mut().for_each(|e| e.2 = 0);
                    b.stamp = 1;
                }
            }
            b.depth += 1;
        });
        BlockBatch
    }
}
impl Drop for BlockBatch {
    fn drop(&mut self) {
        // SAFETY: as in `begin`.
        BATCH.with(|b| unsafe { (*b.get()).depth -= 1 });
    }
}
fn batch_open() -> bool {
    // SAFETY: as in `BlockBatch::begin`.
    BATCH.with(|b| unsafe { (*b.get()).depth > 0 })
}
/// `w.world.block(x, y, z)`, through the batch table when one is open.
#[inline(always)]
fn block_at(w: &MobWorld, x: i32, y: i32, z: i32) -> BlockStateId {
    BATCH.with(|b| {
        // SAFETY: as in `begin`.
        let b = unsafe { &mut *b.get() };
        if b.depth == 0 {
            return w.world.block(x, y, z);
        }
        let kxz = (x as u32 as u64) | ((z as u32 as u64) << 32);
        let i = ((kxz ^ (y as u32 as u64).wrapping_mul(0x9E37_79B9)).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - BATCH_BITS)) as usize;
        let e = &mut b.tab[i];
        if e.2 == b.stamp && e.0 == kxz && e.1 == y {
            return BlockStateId(e.3);
        }
        let id = w.world.block(x, y, z);
        *e = (kxz, y, b.stamp, id.0);
        id
    })
}

/// Whether a block state of this path type can hold a water fluid (waterlogged). Read off every state of the registry by the test
/// `water_fluid_states_by_path_type`: a cell of any other type has no water fluid, so its floor is the static one.
pub fn may_hold_water(t: PathType) -> bool {
    matches!(t, PathType::Blocked | PathType::DamageCautious | PathType::Fence | PathType::Fire | PathType::Leaves | PathType::Rail | PathType::Trapdoor | PathType::Water)
}
pub fn path_type_from_state(w: &MobWorld, x: i32, y: i32, z: i32) -> PathType {
    probe!(bump(12));
    w.world.cached_path_type(BlockPos::new(x, y, z), || {
        probe!(bump(13));
        let state = block_at(w, x, y, z);
        w.mob_hooks
            .and_then(|hooks| hooks.block_path_type(state, [x, y, z]))
            .unwrap_or_else(|| w.data.path_type(state))
    })
}
/// WalkNodeEvaluator.getPathTypeStatic.
/// A mod's supplied path is classified through here, so every block step
/// saturates: a finite coordinate at `i32::MIN` or `i32::MAX` is never
/// refused, and never overflows a checked build either (arismoko/horizon#4443).
pub fn path_type_static(w: &MobWorld, x: i32, y: i32, z: i32) -> PathType {
    probe!(bump(11));
    let t = path_type_from_state(w, x, y, z);
    if t == PathType::Open && y > w.world.bottom_y() {
        match type_on_below(path_type_from_state(w, x, y.saturating_sub(1), z)) {
            Some(t) => t,
            None => check_neighbours(w, x, y, z, PathType::Walkable),
        }
    } else {
        t
    }
}
/// WalkNodeEvaluator.checkNeighbourBlocks: the first neighbour that gives a node a type
/// (`getAdjacentBlockPathType`) decides it. The neighbour step saturates, so a
/// node at the edge of the `i32` range still reads its eight neighbours (arismoko/horizon#4443).
pub fn check_neighbours(w: &MobWorld, x: i32, y: i32, z: i32, t: PathType) -> PathType {
    probe!(bump(17));
    if w.mob_hooks.is_none() {
        return check_neighbours_cached(w, x, y, z, t);
    }
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let pos = [
                    x.saturating_add(dx),
                    y.saturating_add(dy),
                    z.saturating_add(dz),
                ];
                let state = w.world.block(pos[0], pos[1], pos[2]);
                let own = path_type_from_state(w, pos[0], pos[1], pos[2]);
                let facts = w.facts.get(state);
                let default = if facts.name.as_ref() == "minecraft:sweet_berry_bush" {
                    Some(PathType::DamagingInNeighbor)
                } else if vanilla_burning_block(
                    &facts.name,
                    facts.emission > 0,
                    w.has_tag("minecraft:fire", &facts.name),
                ) {
                    Some(PathType::FireInNeighbor)
                } else {
                    None
                };
                let adjacent = w.mob_hooks.map_or(default, |hooks| {
                    hooks.adjacent_block_path_type(state, pos, own, default)
                });
                if let Some(adjacent) = adjacent {
                    return adjacent;
                }
                // A declined block answer keeps this neighbour's fluid
                // contribution before the vanilla type mappings.
                if matches!(facts.fluid, Fluid::Water | Fluid::Modded) {
                    return PathType::WaterBorder;
                }
                if let Some(adjacent) = vanilla_adjacent(own) {
                    return adjacent;
                }
            }
        }
    }
    t
}
/// WalkNodeEvaluator.getFloorLevel(level, pos). Saturating, as the
/// neighbour step is: a follower asking under a node at `i32::MIN` lands on
/// the same block instead of overflowing (arismoko/horizon#4443).
pub fn floor_level(w: &MobWorld, x: i32, y: i32, z: i32) -> f64 {
    let below = y.saturating_sub(1);
    probe!(bump(14));
    f64::from(below) + f64::from(w.data.state(block_at(w, x, below, z)).top)
}

#[derive(Clone, Debug, PartialEq)]
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
pub struct FinderBuffers {
    pub nodes: Vec<Node>,
    pub index: FxHashMap<(i32, i32, i32), u32>,
    pub heap: Vec<(f32, u32)>,
    pub grid: Vec<u64>,
    pub stamp: u32,
    pub types: FxHashMap<(i32, i32, i32), PathType>,
    pub statics: std::cell::RefCell<FxHashMap<(i32, i32, i32), PathType>>,
    pub collisions: FxHashMap<[u64; 6], bool>,
    /// What the kept `types`/`statics`/`collisions` were made for (a block batch keeps them from one search to the next; 0: nothing kept).
    pub sig: u64,
}

impl FinderBuffers {
    pub fn new() -> Self {
        Self {
            nodes: Vec::with_capacity(256),
            index: FxHashMap::default(),
            heap: Vec::with_capacity(256),
            grid: Vec::new(),
            stamp: 0,
            types: FxHashMap::default(),
            statics: std::cell::RefCell::new(FxHashMap::default()),
            collisions: FxHashMap::default(),
            sig: 0,
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.index.clear();
        self.heap.clear();
        self.types.clear();
        self.statics.borrow_mut().clear();
        self.collisions.clear();
        self.sig = 0;
    }
}

thread_local! {
    /// The maluses of the last profile seen (the same few species search over and over).
    static MALUS_CACHE: std::cell::RefCell<Option<ProfileCache>> = const { std::cell::RefCell::new(None) };
    static TLS_BUFFERS: std::cell::RefCell<FinderBuffers> = std::cell::RefCell::new(FinderBuffers::new());
}

/// The last mob profile seen: what defines it (width, height, doors, maluses), its hash and the maluses by type.
struct ProfileCache {
    key: (u32, u32, bool, bool, [Option<f32>; PathType::COUNT]),
    hash: u64,
    mt: [f32; PathType::COUNT],
}
/// The node table is a direct grid GW x GW x GH around the mob (a stamp per search, no clearing); the sections table TAB cells.
const GW: usize = 97;
const GH: usize = 17;
const TAB: usize = 9 * 9 * 3;
const NONE: u32 = u32::MAX;

/// One pathfinding query: the node arena, open set and per-query caches.
pub struct Finder<'a, 'w> {
    /// Set by the caller of one `find_accepted` that reads only whether the node found is Water: the body-box check of `try_jump_on`
    /// (which turns a node into none) then changes nothing it reads, unless the node is Water. Cleared by `try_jump_on` before it recurses.
    lazy_box: bool,
    w: &'a MobWorld<'w>,
    mob: &'a PathMob,
    eval: Evaluator,
    nodes: Vec<Node>,
    index: FxHashMap<(i32, i32, i32), u32>,
    heap: Vec<(f32, u32)>,
    grid: Vec<u64>,
    /// `grid`'s buffer, for the reads through `&self`.
    gp: *mut u64,
    jh: f64,
    use_grid: bool,
    stamp: u32,
    gox: i32,
    goy: i32,
    goz: i32,
    mt: [f32; PathType::COUNT],
    sx0: i32,
    sy0: i32,
    sz0: i32,
    st: [std::cell::Cell<*mut Sect>; TAB],
    pt: [std::cell::Cell<*mut [u8; 4096]>; TAB],
    types: FxHashMap<(i32, i32, i32), PathType>,
    /// `path_type` of each position asked during this query: it reads up to
    /// 26 blocks, and overlapping mob boxes ask for the same positions.
    statics: std::cell::RefCell<FxHashMap<(i32, i32, i32), PathType>>,
    collisions: FxHashMap<[u64; 6], bool>,
    use_layer: bool,
    /// First grid stamp of the run of searches whose cell bytes stay valid.
    kstamp: u16,
    /// Set in the layer's section key of the amphibious evaluator (its static types differ from the walking ones).
    lbit: u64,
    occ: Option<&'static crate::occupancy::Occupancy>,
    visited: i32,
    sig: u64,
    saw_rail: std::cell::Cell<bool>,
    profile: u64,
    entity_width: i32,
    entity_height: i32,
    entity_depth: i32,
}

impl<'a, 'w> Drop for Finder<'a, 'w> {
    fn drop(&mut self) {
        #[cfg(feature = "probes")]
        let _g = sec(1);
        TLS_BUFFERS.with(|cell| {
            if let Ok(mut buf) = cell.try_borrow_mut() {
                self.nodes.clear();
                self.index.clear();
                self.heap.clear();
                if self.sig == 0 {
                    self.types.clear();
                    self.statics.borrow_mut().clear();
                    self.collisions.clear();
                }
                buf.sig = self.sig;
                buf.nodes = std::mem::take(&mut self.nodes);
                buf.index = std::mem::take(&mut self.index);
                buf.heap = std::mem::take(&mut self.heap);
                buf.grid = std::mem::take(&mut self.grid);
                buf.stamp = self.stamp;
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
    /// Collision checks go through the occupancy masks (the same answers as `MobWorld::no_collision`).
    pub fn with_occ(mut self, occ: &'static crate::occupancy::Occupancy) -> Self {
        self.occ = Some(occ);
        self
    }
    pub fn new(w: &'a MobWorld<'w>, mob: &'a PathMob, eval: Evaluator) -> Self {
        #[cfg(feature = "probes")]
        let _g = sec(0);
        adjacent_reset();
        LAYER.with(|l| {
            let e = LAYER_EPOCH.load(std::sync::atomic::Ordering::Acquire);
            let mut l = l.borrow_mut();
            if l.epoch != e {
                l.map.clear();
                l.pmap.clear();
                l.epoch = e;
            }
        });
        let batch = batch_open();
        let (nodes, index, heap, mut grid, mut stamp, mut types, mut statics, mut collisions, kept_sig) = TLS_BUFFERS.with(|cell| {
            if let Ok(mut buf) = cell.try_borrow_mut() {
                if batch {
                    buf.nodes.clear();
                    buf.index.clear();
                    buf.heap.clear();
                } else {
                    buf.clear();
                }
                (
                    std::mem::take(&mut buf.nodes),
                    std::mem::take(&mut buf.index),
                    std::mem::take(&mut buf.heap),
                    std::mem::take(&mut buf.grid),
                    buf.stamp,
                    std::mem::take(&mut buf.types),
                    std::mem::take(&mut buf.statics),
                    std::mem::take(&mut buf.collisions),
                    buf.sig,
                )
            } else {
                (
                    Vec::with_capacity(128),
                    FxHashMap::default(),
                    Vec::with_capacity(128),
                    Vec::new(),
                    0,
                    FxHashMap::default(),
                    std::cell::RefCell::new(FxHashMap::default()),
                    FxHashMap::default(),
                    0,
                )
            }
        });
        if grid.is_empty() && TLS_BUFFERS.with(|c| c.try_borrow_mut().is_ok()) {
            grid = vec![0u64; GW * GW * GH];
        }
        if !grid.is_empty() {
            stamp += 1;
            if stamp > 0xFFFF {
                grid.iter_mut().for_each(|e| *e = 0);
                stamp = 1;
            }
        }
        let use_layer = layer_on() && matches!(eval, Evaluator::Walk | Evaluator::Amphibious { .. }) && w.mob_hooks.is_none() && (mob.width + 1.0).floor() <= 5.0 && (mob.height + 1.0).floor() <= 5.0;
        if use_layer && stamp & 63 == 0 {
            layer_evict();
        }
        let mp = block_pos(mob);
        let amph = matches!(eval, Evaluator::Amphibious { .. });
        let key = (mob.width.to_bits(), mob.height.to_bits(), mob.can_open_doors, mob.can_pass_doors);
        let mut mt = [0.0f32; PathType::COUNT];
        let mut profile = 0;
        let hit = !amph && MALUS_CACHE.with(|c| match &*c.borrow() {
            Some(pc) if pc.key.0 == key.0 && pc.key.1 == key.1 && pc.key.2 == key.2 && pc.key.3 == key.3 && pc.key.4 == mob.malus => {
                mt = pc.mt;
                profile = pc.hash;
                true
            }
            _ => false,
        });
        if !hit {
            profile = profile_of(mob);
            for t in PathType::ALL {
                mt[t as usize] = match (eval, t) {
                    (Evaluator::Amphibious { .. }, PathType::Water) => 0.0,
                    (Evaluator::Amphibious { .. }, PathType::Walkable) => 6.0,
                    (Evaluator::Amphibious { .. }, PathType::WaterBorder) => 4.0,
                    _ => mob.malus(t),
                };
            }
            if !amph {
                MALUS_CACHE.with(|c| *c.borrow_mut() = Some(ProfileCache { key: (key.0, key.1, key.2, key.3, mob.malus), hash: profile, mt }));
            }
        }
        // In a batch the answers of the search before are kept when it was made for the same mob at the same place.
        let sig = if batch {
            let q = mob.position;
            profile ^ u64::from(amph).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ q.x.to_bits().rotate_left(7) ^ q.y.to_bits().rotate_left(19) ^ q.z.to_bits().rotate_left(31) ^ u64::from(mob.in_water) << 1 | 1
        } else {
            0
        };
        let kstamp = KSTAMP.with(|k| {
            if sig == 0 || sig != kept_sig {
                k.set(stamp as u16);
            }
            k.get()
        });
        if sig == 0 || sig != kept_sig {
            types.clear();
            statics.get_mut().clear();
            collisions.clear();
        }
        Self {
            sig,
            w,
            mob,
            eval,
            nodes,
            index,
            heap,
            use_grid: !grid.is_empty(),
            gp: grid.as_mut_ptr(),
            jh: f64::from(mob.max_up_step).max(1.125),
            grid,
            stamp,
            gox: mp[0].wrapping_sub(GW as i32 / 2),
            goy: mp[1].wrapping_sub(GH as i32 / 2),
            goz: mp[2].wrapping_sub(GW as i32 / 2),
            mt,
            sx0: (mp[0] >> 4) - 4,
            sy0: (mp[1] >> 4) - 1,
            sz0: (mp[2] >> 4) - 4,
            st: [const { std::cell::Cell::new(std::ptr::null_mut()) }; TAB],
            pt: [const { std::cell::Cell::new(std::ptr::null_mut()) }; TAB],
            types,
            statics,
            collisions,
            use_layer,
            kstamp,
            lazy_box: false,
            occ: None,
            visited: 0,
            saw_rail: std::cell::Cell::new(false),
            profile: if amph { profile ^ AMPH_MIX } else { profile },
            lbit: if amph { AMPH_BIT } else { 0 },
            entity_width: (mob.width + 1.0).ifloor(),
            entity_height: (mob.height + 1.0).ifloor(),
            entity_depth: (mob.width + 1.0).ifloor(),
        }
    }
    /// Mob.getPathfindingMalus; AmphibiousNodeEvaluator.prepare overrides three
    /// maluses for the duration of the query.
    #[inline(always)]
    fn malus(&self, t: PathType) -> f32 {
        self.mt[t as usize]
    }
    /// The static `PathType` of a position under this finder's evaluator,
    /// through the world's cached classification. One classification per
    /// position: a mod's supplied path is classified here on submit.
    pub fn path_type_at(&self, x: i32, y: i32, z: i32) -> PathType {
        self.path_type(x, y, z)
    }
    // --- world access -------------------------------------------------
    fn state(&self, x: i32, y: i32, z: i32) -> data::StateData {
        probe!(bump(15));
        self.w.data.state(block_at(self.w, x, y, z))
    }
    fn pathfindable(&self, x: i32, y: i32, z: i32, kind: PathComputationType) -> bool {
        probe!(bump(15));
        self.w.data.pathfindable(block_at(self.w, x, y, z), kind)
    }
    fn type_from_state(&self, x: i32, y: i32, z: i32) -> PathType {
        path_type_from_state(self.w, x, y, z)
    }
    fn fluid(&self, x: i32, y: i32, z: i32) -> Fluid {
        probe!(bump(15));
        self.w.facts.get(block_at(self.w, x, y, z)).fluid
    }
    fn is_air(&self, x: i32, y: i32, z: i32) -> bool {
        probe!(bump(15));
        self.w.facts.is_air(block_at(self.w, x, y, z))
    }
    /// The layer's section of a cell: from the per-search table when it is within reach of the mob, else from the layer.
    #[inline(always)]
    fn sect(&self, x: i32, y: i32, z: i32) -> *mut Sect {
        let (a, b, c) = (((x >> 4) - self.sx0) as u32, ((z >> 4) - self.sz0) as u32, ((y >> 4) - self.sy0) as u32);
        if a < 9 && b < 9 && c < 3 {
            let cell = &self.st[((a * 9 + b) * 3 + c) as usize];
            let p = cell.get();
            if !p.is_null() {
                return p;
            }
            let p = LAYER.with(|l| l.borrow_mut().section(layer_key(x, y, z) | self.lbit));
            cell.set(p);
            return p;
        }
        LAYER.with(|l| l.borrow_mut().section(layer_key(x, y, z) | self.lbit))
    }
    #[inline(always)]
    fn psect(&self, x: i32, y: i32, z: i32) -> *mut [u8; 4096] {
        let (a, b, c) = (((x >> 4) - self.sx0) as u32, ((z >> 4) - self.sz0) as u32, ((y >> 4) - self.sy0) as u32);
        if a < 9 && b < 9 && c < 3 {
            let cell = &self.pt[((a * 9 + b) * 3 + c) as usize];
            let p = cell.get();
            if !p.is_null() {
                return p;
            }
            let p = LAYER.with(|l| l.borrow_mut().profile_section((self.profile, self.entity_width, self.entity_height), layer_key(x, y, z)));
            cell.set(p);
            return p;
        }
        LAYER.with(|l| l.borrow_mut().profile_section((self.profile, self.entity_width, self.entity_height), layer_key(x, y, z)))
    }
    /// WalkNodeEvaluator.getFloorLevel(level, pos).
    pub fn static_floor(&self, x: i32, y: i32, z: i32) -> f64 {
        if self.use_layer {
            let slot = self.slot(x, y, z);
            return self.static_floor_at(slot, x, y, z);
        }
        f64::from(y.saturating_sub(1)) + f64::from(self.state(x, y.saturating_sub(1), z).top)
    }
    /// `static_floor` of a cell whose grid cell (`slot`) is known and reset, with the layer on.
    #[inline(always)]
    fn static_floor_at(&self, slot: Option<*mut u64>, x: i32, y: i32, z: i32) -> f64 {
        let below = y.saturating_sub(1);
        {
            if let Some(p) = slot {
                // SAFETY: a grid cell.
                let c = unsafe { (*p >> 56) as u8 };
                if c < 0xFE {
                    return f64::from(below) + f64::from(c) / 16.0;
                }
            }
            let idx = layer_idx(x, below, z);
            let sect = self.sect(x, below, z);
            // SAFETY: as in `path_type`.
            let cell = unsafe { &mut (*sect).top[idx] };
            if *cell == 0xFF {
                let top = self.state(x, below, z).top;
                let q = top * 16.0;
                *cell = if q >= 0.0 && q < 250.0 && q.fract() == 0.0 { q as u8 } else { 0xFE };
            }
            let c = *cell;
            if let Some(p) = slot {
                // SAFETY: `p` comes from `slot()`, a cell of the Finder's grid (index checked there); no reference to it is alive.
                unsafe { *p = (*p & !(0xFFu64 << 56)) | (u64::from(c) << 56) };
            }
            if c < 0xFE { f64::from(below) + f64::from(c) / 16.0 } else { f64::from(below) + f64::from(self.state(x, below, z).top) }
        }
    }
    fn floor_level(&self, x: i32, y: i32, z: i32) -> f64 {
        let amphibious = matches!(self.eval, Evaluator::Amphibious { .. });
        if (self.mob.can_float || amphibious) && self.fluid(x, y, z) == Fluid::Water {
            f64::from(y) + 0.5
        } else {
            self.static_floor(x, y, z)
        }
    }
    fn has_collisions(&mut self, b: Aabb) -> bool {
        #[cfg(feature = "probes")]
        let _g = sec(6);
        probe!(bump(20));
        let (lo, hi) = (b.min_position(), b.max_position());
        let key = [
            lo.x.to_bits(),
            lo.y.to_bits(),
            lo.z.to_bits(),
            hi.x.to_bits(),
            hi.y.to_bits(),
            hi.z.to_bits(),
        ];
        if let Some(v) = self.collisions.get(&key) {
            return *v;
        }
        probe!(bump(21));
        let v = match self.occ {
            Some(o) => !o.no_collision(horizon_client_render::collisions::SceneCollisions { world: self.w.world, binding: self.w.collision, subject: None }, b),
            None => !self.w.no_collision(b),
        };
        self.collisions.insert(key, v);
        v
    }
    // --- path types -----------------------------------------------------
    /// WalkNodeEvaluator.getPathTypeStatic (+ the fly/amphibious overrides).
    /// The world doesn't change during a query, so each position's type is
    /// worked out once.
    fn path_type(&self, x: i32, y: i32, z: i32) -> PathType {
        #[cfg(feature = "probes")]
        let _g = sec(14);
        probe!(bump(9));
        if self.use_layer {
            let idx = layer_idx(x, y, z);
            let sect = self.sect(x, y, z);
            // SAFETY: the box lives in the layer map and is neither moved nor dropped while a search runs (`changed`/`clear` are called between searches).
            let cell = unsafe { &mut (*sect).stat[idx] };
            if *cell != 0 {
                return PathType::ALL[usize::from(*cell - 1)];
            }
            probe!(bump(10));
            let t = self.compute_path_type(x, y, z);
            *cell = t as u8 + 1;
            return t;
        }
        if let Some(t) = self.statics.borrow().get(&(x, y, z)) {
            return *t;
        }
        probe!(bump(10));
        let t = self.compute_path_type(x, y, z);
        self.statics.borrow_mut().insert((x, y, z), t);
        t
    }
    fn compute_path_type(&self, x: i32, y: i32, z: i32) -> PathType {
        #[cfg(feature = "probes")]
        let _g = sec(1);
        match self.eval {
            Evaluator::Fly => self.fly_path_type(x, y, z),
            Evaluator::Amphibious { .. } => {
                let t = self.type_from_state(x, y, z);
                if t == PathType::Water {
                    for (dx, dy, dz) in [
                        (0, -1, 0),
                        (0, 1, 0),
                        (0, 0, -1),
                        (0, 0, 1),
                        (-1, 0, 0),
                        (1, 0, 0),
                    ] {
                        if self.type_from_state(
                            x.saturating_add(dx),
                            y.saturating_add(dy),
                            z.saturating_add(dz),
                        ) == PathType::Blocked
                        {
                            return PathType::WaterBorder;
                        }
                    }
                    PathType::Water
                } else {
                    self.walk_path_type(x, y, z)
                }
            }
            _ => self.walk_path_type(x, y, z),
        }
    }
    pub fn walk_path_type(&self, x: i32, y: i32, z: i32) -> PathType {
        path_type_static(self.w, x, y, z)
    }
    fn check_neighbours(&self, x: i32, y: i32, z: i32, t: PathType) -> PathType {
        check_neighbours(self.w, x, y, z, t)
    }
    fn fly_path_type(&self, x: i32, y: i32, z: i32) -> PathType {
        let mut t = self.type_from_state(x, y, z);
        if t == PathType::Open && y > self.w.world.bottom_y() {
            let below_y = y.saturating_sub(1);
            let below = self.type_from_state(x, below_y, z);
            t = match below {
                PathType::Fire | PathType::Lava => PathType::Fire,
                PathType::Damaging => PathType::Damaging,
                PathType::Cocoa => PathType::Cocoa,
                PathType::Fence => {
                    if [x, below_y, z] != block_pos(self.mob) {
                        PathType::Fence
                    } else {
                        t
                    }
                }
                PathType::Walkable | PathType::Open | PathType::Water => PathType::Open,
                _ => PathType::Walkable,
            };
        }
        if t == PathType::Walkable || t == PathType::Open {
            t = self.check_neighbours(x, y, z, t);
        }
        t
    }
    fn within_bb(&self, x: i32, y: i32, z: i32) -> Small {
        #[cfg(feature = "probes")]
        let _g = sec(5);
        probe!(bump(19));
        let mut out = Small::new();
        let mp = block_pos(self.mob);
        for dx in 0..self.entity_width {
            for dy in 0..self.entity_height {
                for dz in 0..self.entity_depth {
                    let mut t = self.path_type(
                        x.saturating_add(dx),
                        y.saturating_add(dy),
                        z.saturating_add(dz),
                    );
                    if t == PathType::DoorWoodClosed
                        && self.mob.can_open_doors
                        && self.mob.can_pass_doors
                    {
                        t = PathType::WalkableDoor;
                    }
                    if t == PathType::DoorOpen && !self.mob.can_pass_doors {
                        t = PathType::Blocked;
                    }
                    if t == PathType::Rail
                        && { self.saw_rail.set(true); true }
                        && self.path_type(mp[0], mp[1], mp[2]) != PathType::Rail
                        && self.path_type(mp[0], mp[1].saturating_sub(1), mp[2]) != PathType::Rail
                    {
                        t = PathType::UnpassableRail;
                    }
                    out.add(t);
                }
            }
        }
        out
    }
    /// WalkNodeEvaluator.getPathTypeOfMob.
    fn path_type_of_mob(&self, x: i32, y: i32, z: i32) -> PathType {
        #[cfg(feature = "probes")]
        let _g = sec(0);
        if let Evaluator::Swim { .. } = self.eval {
            return self.swim_path_type(x, y, z);
        }
        let mut types = self.within_bb(x, y, z);
        // EnumSet iteration follows declaration order.
        types.sort();
        let types = types.as_slice();
        if types.len() == 1 {
            return types[0];
        }
        if types.contains(&PathType::Fence) {
            return PathType::Fence;
        }
        if types.contains(&PathType::UnpassableRail) {
            return PathType::UnpassableRail;
        }
        let mut highest = PathType::Blocked;
        let mut highest_malus = self.malus(highest);
        for t in types {
            let m = self.malus(*t);
            if m < 0.0 {
                return *t;
            }
            if m >= highest_malus {
                highest_malus = m;
                highest = *t;
            }
        }
        let current = self.path_type(x, y, z);
        if self.entity_width > 1 {
            let cheaper = self.malus(current) < highest_malus;
            if cheaper && self.malus(PathType::BigMobsCloseToDanger) < highest_malus {
                PathType::BigMobsCloseToDanger
            } else {
                highest
            }
        } else if current == PathType::Open && highest != PathType::Open && highest_malus == 0.0 {
            PathType::Open
        } else {
            highest
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
    fn cached(&mut self, x: i32, y: i32, z: i32) -> PathType {
        #[cfg(feature = "probes")]
        let _g = sec(10);
        probe!(bump(7));
        if self.use_layer {
            let slot = self.slot(x, y, z);
            return self.cached_at(slot, x, y, z);
        }
        self.cached_off_layer(x, y, z)
    }
    /// `cached` of a cell whose grid cell (`slot`) is known and reset.
    #[inline(always)]
    fn cached_at(&mut self, slot: Option<*mut u64>, x: i32, y: i32, z: i32) -> PathType {
        {
            if let Some(p) = slot {
                // SAFETY: a grid cell.
                let v = unsafe { (*p >> 48) as u8 };
                if v != 0 {
                    return PathType::ALL[usize::from(v - 1)];
                }
            }
            let idx = layer_idx(x, y, z);
            let sect = self.psect(x, y, z);
            // SAFETY: as in `path_type`; the boxes of the profile map are never moved.
            let v = unsafe { (*sect)[idx] };
            if v != 0 {
                if let Some(p) = slot {
                    // SAFETY: `p` comes from `slot()`, a cell of the Finder's grid; only the type byte is written.
                    unsafe { *p = (*p & !(0xFFu64 << 48)) | (u64::from(v) << 48) };
                }
                return PathType::ALL[usize::from(v - 1)];
            }
            probe!(bump(8));
            self.saw_rail.set(false);
            let t = self.path_type_of_mob(x, y, z);
            if !self.saw_rail.get() {
                // SAFETY: as above.
                unsafe { (*sect)[idx] = t as u8 + 1 };
                if let Some(p) = slot {
                    // SAFETY: as above (`p` from `slot()`, type byte only).
                    unsafe { *p = (*p & !(0xFFu64 << 48)) | (u64::from(t as u8 + 1) << 48) };
                }
            }
            return t;
        }
    }
    fn cached_off_layer(&mut self, x: i32, y: i32, z: i32) -> PathType {
        if let Some(t) = self.types.get(&(x, y, z)) {
            return *t;
        }
        probe!(bump(8));
        let t = self.path_type_of_mob(x, y, z);
        self.types.insert((x, y, z), t);
        t
    }
    // --- node arena -------------------------------------------------------
    /// The grid cell of (x, y, z) for this search, reset when it is from an older one; None outside the grid.
    /// A cell: node index (low 32 bits, NONE if none), search stamp (16), type + 1 (8, 0 unread), floor code (8, 0xFF unread).
    #[inline(always)]
    fn slot(&self, x: i32, y: i32, z: i32) -> Option<*mut u64> {
        let (dx, dy, dz) = (x.wrapping_sub(self.gox) as u32 as usize, y.wrapping_sub(self.goy) as u32 as usize, z.wrapping_sub(self.goz) as u32 as usize);
        if !self.use_grid || dx >= GW || dz >= GW || dy >= GH {
            return None;
        }
        // SAFETY: in range of the grid, whose buffer lives as long as `self`.
        unsafe {
            let p = self.gp.add((dy * GW + dz) * GW + dx);
            self.fresh(p);
            Some(p)
        }
    }
    /// Resets a grid cell from an older search.
    ///
    /// # Safety
    ///
    /// `p` must be a cell of `self.grid` (as given by `slot` or `centre`), and no reference to that cell may be alive.
    #[inline(always)]
    unsafe fn fresh(&self, p: *mut u64) {
        // SAFETY: the caller gives a cell of the grid.
        unsafe {
            let old = (*p >> 32) as u16;
            if old != self.stamp as u16 {
                // The type and floor bytes of a search of the same batch, mob and place stay true (the world does not change inside a batch).
                let keep = if old >= self.kstamp && old != 0 { *p & (!0u64 << 48) } else { 0xFFu64 << 56 };
                *p = keep | (u64::from(self.stamp as u16) << 32) | u64::from(NONE);
            }
        }
    }
    /// The grid cell of (x, y, z) when it and its 8 horizontal neighbours and the cell above are all in the grid, without resetting it.
    #[inline(always)]
    fn centre(&self, x: i32, y: i32, z: i32) -> Option<*mut u64> {
        let (dx, dy, dz) = (x.wrapping_sub(self.gox) as u32 as usize, y.wrapping_sub(self.goy) as u32 as usize, z.wrapping_sub(self.goz) as u32 as usize);
        if !self.use_grid || dx == 0 || dz == 0 || dx >= GW - 1 || dz >= GW - 1 || dy == 0 || dy >= GH - 1 {
            return None;
        }
        // SAFETY: in range of the grid.
        Some(unsafe { self.gp.add((dy * GW + dz) * GW + dx) })
    }
    fn node(&mut self, x: i32, y: i32, z: i32) -> u32 {
        probe!(bump(5));
        if let Some(p) = self.slot(x, y, z) {
            // SAFETY: `slot` gives a cell of the grid.
            let e = unsafe { *p };
            if e as u32 != NONE {
                return e as u32;
            }
            let i = self.nodes.len() as u32;
            probe!(bump(6));
            self.nodes.push(Node::new(x, y, z));
            // SAFETY: as above, `p` from `slot()`; the grid is not reallocated by the push (it is a different Vec).
            unsafe { *p = (e & !0xFFFF_FFFF) | u64::from(i) };
            return i;
        }
        if let Some(i) = self.index.get(&(x, y, z)) {
            return *i;
        }
        let i = self.nodes.len() as u32;
        probe!(bump(6));
        self.nodes.push(Node::new(x, y, z));
        self.index.insert((x, y, z), i);
        i
    }
    fn start_node(&mut self, x: i32, y: i32, z: i32) -> u32 {
        let n = self.node(x, y, z);
        let t = self.cached(x, y, z);
        let m = self.malus(t);
        let node = &mut self.nodes[n as usize];
        node.kind = t;
        node.cost_malus = m;
        n
    }
    fn can_start_at(&mut self, x: i32, y: i32, z: i32) -> bool {
        let t = self.cached(x, y, z);
        if self.eval == Evaluator::Fly {
            return self.malus(t) >= 0.0;
        }
        t != PathType::Open && self.malus(t) >= 0.0
    }
    fn start(&mut self) -> Option<u32> {
        let m = self.mob;
        let p = m.position;
        let b = m.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        match self.eval {
            Evaluator::Swim { .. } => {
                return Some(self.node(
                    lo.x.ifloor(),
                    (lo.y + 0.5).ifloor(),
                    lo.z.ifloor(),
                ));
            }
            Evaluator::Amphibious { .. } if m.in_water => {
                return Some(self.start_node(
                    lo.x.ifloor(),
                    (lo.y + 0.5).ifloor(),
                    lo.z.ifloor(),
                ));
            }
            Evaluator::Fly => {
                let (bx, bz) = (p.x.ifloor(), p.z.ifloor());
                let mut start_y;
                if m.can_float && m.in_water {
                    start_y = p.y.ifloor();
                    while &*self.w.facts.get(self.w.world.block(bx, start_y, bz)).name
                        == "minecraft:water"
                    {
                        start_y += 1;
                    }
                } else {
                    start_y = (p.y + 0.5).ifloor();
                }
                if !self.can_start_at(bx, start_y, bz) {
                    let by = p.y.ifloor();
                    for (cx, cz) in [(lo.x, lo.z), (lo.x, hi.z), (hi.x, lo.z), (hi.x, hi.z)] {
                        let (cx, cz) = (cx.ifloor(), cz.ifloor());
                        if self.can_start_at(cx, by, cz) {
                            return Some(self.start_node(cx, by, cz));
                        }
                    }
                }
                return Some(self.start_node(bx, start_y, bz));
            }
            _ => {}
        }
        // WalkNodeEvaluator.getStart
        let (bx, bz) = (p.x.ifloor(), p.z.ifloor());
        let mut start_y = p.y.ifloor();
        let fluid_at = |s: &Self, y: i32| s.fluid(bx, y, bz);
        if !(m.stands_on_lava && fluid_at(self, start_y) == Fluid::Lava) {
            if m.can_float && fluid_at(self, start_y) == Fluid::Water {
                while fluid_at(self, start_y) == Fluid::Water {
                    start_y += 1;
                }
                start_y -= 1;
            } else if m.on_ground {
                start_y = (p.y + 0.5).ifloor();
            } else {
                let mut y = (p.y + 1.0).ifloor();
                while y > self.w.world.bottom_y() {
                    start_y = y;
                    y -= 1;
                    let s = self.w.world.block(bx, y, bz);
                    if !self.w.facts.is_air(s)
                        && !self.w.data.pathfindable(s, PathComputationType::Land)
                    {
                        break;
                    }
                }
            }
        } else {
            while m.stands_on_lava && fluid_at(self, start_y) == Fluid::Lava {
                start_y += 1;
            }
            start_y -= 1;
        }
        if !self.can_start_at(bx, start_y, bz) {
            for (cx, cz) in [(lo.x, lo.z), (lo.x, hi.z), (hi.x, lo.z), (hi.x, hi.z)] {
                let (cx, cz) = (cx.ifloor(), cz.ifloor());
                if self.can_start_at(cx, start_y, cz) {
                    return Some(self.start_node(cx, start_y, cz));
                }
            }
        }
        Some(self.start_node(bx, start_y, bz))
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
    fn can_reach_without_collision(&mut self, n: u32) -> bool {
        #[cfg(feature = "probes")]
        let _g = sec(7);
        let (nx, ny, nz) = {
            let node = &self.nodes[n as usize];
            (node.x, node.y, node.z)
        };
        let mut bb = self.mob.bounds();
        let size = {
            let (lo, hi) = (bb.min_position(), bb.max_position());
            ((hi.x - lo.x) + (hi.y - lo.y) + (hi.z - lo.z)) / 3.0
        };
        let (lo, hi) = (bb.min_position(), bb.max_position());
        let p = self.mob.position;
        let mut delta = Vec3::new(
            f64::from(nx) - p.x + (hi.x - lo.x) / 2.0,
            f64::from(ny) - p.y + (hi.y - lo.y) / 2.0,
            f64::from(nz) - p.z + (hi.z - lo.z) / 2.0,
        );
        let len = (delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt();
        let steps = collision_steps(len, size);
        delta = Vec3::new(
            delta.x / f64::from(steps),
            delta.y / f64::from(steps),
            delta.z / f64::from(steps),
        );
        for _ in 1..=steps {
            bb = bb.move_by(delta);
            if self.has_collisions(bb) {
                return false;
            }
        }
        true
    }
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn find_accepted(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        jump: i32,
        node_height: f64,
        dir: (i32, i32),
        current: PathType,
    ) -> Option<u32> {
        if self.use_layer && !Self::partial_collision(current) && !(self.lbit != 0 && no_amph_fast()) {
            if let Some(p) = self.slot(x, y, z) {
                return self.find_accepted_p(p, x, y, z, jump, node_height, dir, current);
            }
        }
        self.find_accepted_slow(x, y, z, jump, node_height, dir, current)
    }
    /// `find_accepted` of the cell whose grid cell is `p` (already reset); `current` is not a partial-collision type.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    fn find_accepted_p(&mut self, p: *mut u64, x: i32, y: i32, z: i32, jump: i32, node_height: f64, dir: (i32, i32), current: PathType) -> Option<u32> {
        // Common case, all read from the grid cell (filled from the layer the first time): solid floor, walkable cell.
        // SAFETY: a grid cell; `cached`/`static_floor`/`node_update_max` only fill fields of it.
        let mut e = unsafe { *p };
        if (e >> 48) as u8 == 0 {
            self.cached_at(Some(p), x, y, z);
            // SAFETY: `p` is still a grid cell; the calls above only fill its fields.
            e = unsafe { *p };
        }
        if (e >> 56) as u8 == 0xFF {
            self.static_floor_at(Some(p), x, y, z);
            // SAFETY: as above.
            e = unsafe { *p };
        }
        let (ty, c, idx) = ((e >> 48) as u8, (e >> 56) as u8, e as u32);
        if c < 0xFE && ty == PathType::Walkable as u8 + 1 {
            if f64::from(y.saturating_sub(1)) + f64::from(c) / 16.0 - node_height > self.jh {
                return None;
            }
            let cost = self.mt[PathType::Walkable as usize];
            if cost < 0.0 {
                return None;
            }
            if idx == NONE {
                return Some(self.node_update_max(x, y, z, PathType::Walkable, cost));
            }
            let node = &mut self.nodes[idx as usize];
            node.kind = PathType::Walkable;
            node.cost_malus = node.cost_malus.max(cost);
            return Some(idx);
        }
        let t = PathType::ALL[usize::from(ty) - 1];
        if c < 0xFE && !may_hold_water(t) {
            // The floor of a cell that cannot hold water is the static one (`floor_level`).
            if f64::from(y.saturating_sub(1)) + f64::from(c) / 16.0 - node_height > self.jh {
                return None;
            }
            let cost = self.mt[t as usize];
            if cost >= 0.0 {
                // `find_accepted_slow` returns this node, or the first ground below an Open cell.
                let n = self.node_update_max(x, y, z, t, cost);
                return Some(if t == PathType::Open { self.first_ground_below(x, y, z) } else { n });
            }
            if jump <= 0 && t != PathType::Open && !Self::partial_collision(t) {
                return None;
            }
        }
        self.find_accepted_slow(x, y, z, jump, node_height, dir, current)
    }
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn find_accepted_slow(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        jump: i32,
        node_height: f64,
        dir: (i32, i32),
        current: PathType,
    ) -> Option<u32> {
        #[cfg(feature = "probes")]
        let _g = sec(2);
        let amphibious = matches!(self.eval, Evaluator::Amphibious { .. });
        probe!(bump(2));
        let max_y_target = self.floor_level(x, y, z);
        if max_y_target - node_height > self.jump_height() {
            return None;
        }
        let t = self.cached(x, y, z);
        let cost = self.malus(t);
        let mut best = None;
        if cost >= 0.0 {
            best = Some(self.node_update_max(x, y, z, t, cost));
        }
        if Self::partial_collision(current)
            && let Some(b) = best
            && self.nodes[b as usize].cost_malus >= 0.0
            && !self.can_reach_without_collision(b)
        {
            best = None;
        }
        if t == PathType::Walkable || (amphibious && t == PathType::Water) {
            return best;
        }
        let best_cost_negative = best.is_none_or(|b| self.nodes[b as usize].cost_malus < 0.0);
        if best_cost_negative
            && jump > 0
            && (t != PathType::Fence || self.mob.can_walk_over_fences)
            && t != PathType::UnpassableRail
            && t != PathType::Trapdoor
            && t != PathType::PowderSnow
        {
            best = self.try_jump_on(x, y, z, jump, node_height, dir, current);
        } else if !amphibious && t == PathType::Water && !self.mob.can_float {
            best = self.first_non_water_below(x, y, z, best);
        } else if t == PathType::Open {
            best = Some(self.first_ground_below(x, y, z));
        } else if Self::partial_collision(t) && best.is_none() {
            best = Some(self.closed_node(x, y, z, t));
        }
        best
    }
    #[allow(clippy::too_many_arguments)]
    fn try_jump_on(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        jump: i32,
        node_height: f64,
        dir: (i32, i32),
        current: PathType,
    ) -> Option<u32> {
        probe!(bump(3));
        let lazy = std::mem::take(&mut self.lazy_box);
        let above = self.find_accepted(x, y + 1, z, jump - 1, node_height, dir, current)?;
        if self.mob.width >= 1.0 || lazy && self.nodes[above as usize].kind != PathType::Water {
            return Some(above);
        }
        let (ak, ax, ay, az) = {
            let n = &self.nodes[above as usize];
            (n.kind, n.x, n.y, n.z)
        };
        if ak != PathType::Open && ak != PathType::Walkable {
            return Some(above);
        }
        let cx = f64::from(x - dir.0) + 0.5;
        let cz = f64::from(z - dir.1) + 0.5;
        let half = f64::from(self.mob.width) / 2.0;
        let grow = Aabb::new(
            Vec3::new(
                cx - half,
                self.floor_level(cx.ifloor(), y + 1, cz.ifloor()) + 0.001,
                cz - half,
            ),
            Vec3::new(
                cx + half,
                f64::from(self.mob.height) + self.floor_level(ax, ay, az) - 0.002,
                cz + half,
            ),
        );
        if self.has_collisions(grow) {
            None
        } else {
            Some(above)
        }
    }
    fn first_non_water_below(
        &mut self,
        x: i32,
        mut y: i32,
        z: i32,
        mut best: Option<u32>,
    ) -> Option<u32> {
        y -= 1;
        while y > self.w.world.bottom_y() {
            let t = self.cached(x, y, z);
            if t != PathType::Water {
                return best;
            }
            let m = self.malus(t);
            best = Some(self.node_update_max(x, y, z, t, m));
            y -= 1;
        }
        best
    }
    fn first_ground_below(&mut self, x: i32, y: i32, z: i32) -> u32 {
        probe!(bump(4));
        let mut cy = y - 1;
        while cy >= self.w.world.bottom_y() {
            if y - cy > self.mob.max_fall_distance {
                return self.blocked_node(x, cy, z);
            }
            let t = self.cached(x, cy, z);
            let cost = self.malus(t);
            if t != PathType::Open {
                if cost >= 0.0 {
                    return self.node_update_max(x, cy, z, t, cost);
                }
                return self.blocked_node(x, cy, z);
            }
            cy -= 1;
        }
        self.blocked_node(x, y, z)
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
    fn walk_neighbors(&mut self, pos: u32, out: &mut Vec<u32>) {
        #[cfg(feature = "probes")]
        let _g = sec(3);
        let (px, py, pz) = {
            let n = &self.nodes[pos as usize];
            (n.x, n.y, n.z)
        };
        let above = self.cached(px, py + 1, pz);
        let current = self.cached(px, py, pz);
        let mut jump = 0;
        if self.malus(above) >= 0.0 && current != PathType::StickyHoney {
            jump = self.mob.max_up_step.max(1.0).ifloor();
        }
        let height = self.floor_level(px, py, pz);
        // Direction.Plane.HORIZONTAL: north, east, south, west; 2D index S=0 W=1 N=2 E=3.
        const DIRS: [(i32, i32, usize); 4] = [(0, -1, 2), (1, 0, 3), (0, 1, 0), (-1, 0, 1)];
        let mut by2d: [Option<u32>; 4] = [None; 4];
        for (dx, dz, i2) in DIRS {
            let n = self.find_accepted(px + dx, py, pz + dz, jump, height, (dx, dz), current);
            by2d[i2] = n;
            if self.neighbor_valid(n, pos) {
                out.push(n.unwrap_or_default());
            }
        }
        // Clockwise of north is east, of east south, of south west, of west north.
        let clockwise = |i2: usize| match i2 {
            2 => 3,
            3 => 0,
            0 => 1,
            _ => 2,
        };
        for (dx, dz, i2) in DIRS {
            let c = clockwise(i2);
            let (cdx, cdz) = match c {
                0 => (0, 1),
                1 => (-1, 0),
                2 => (0, -1),
                _ => (1, 0),
            };
            if self.diagonal_pair_valid(pos, by2d[i2], by2d[c]) {
                let d = self.find_accepted(
                    px + dx + cdx,
                    py,
                    pz + dz + cdz,
                    jump,
                    height,
                    (dx, dz),
                    current,
                );
                if self.diagonal_valid(d) {
                    out.push(d.unwrap_or_default());
                }
            }
        }
        if let Evaluator::Amphibious { prefers_shallow } = self.eval {
            let up = self.find_accepted(px, py + 1, pz, (jump - 1).max(0), height, (0, 0), current);
            self.lazy_box = true;
            let down = self.find_accepted(px, py - 1, pz, jump, height, (0, 0), current);
            self.lazy_box = false;
            if self.neighbor_valid(up, pos)
                && up.is_some_and(|u| self.nodes[u as usize].kind == PathType::Water)
            {
                out.push(up.unwrap_or_default());
            }
            if self.neighbor_valid(down, pos)
                && down.is_some_and(|d| self.nodes[d as usize].kind == PathType::Water)
                && current != PathType::Trapdoor
            {
                out.push(down.unwrap_or_default());
            }
            if prefers_shallow {
                for &n in out.iter() {
                    let node = &mut self.nodes[n as usize];
                    if node.kind == PathType::Water && node.y < 63 - 10 {
                        node.cost_malus += 1.0;
                    }
                }
            }
        }
    }
    /// walk_neighbors and the relaxation of the neighbours, in one pass and in place (Walk only): the neighbours are worked out in the
    /// order of `walk_neighbors` and relaxed at once; relaxing changes nothing that finding the next ones reads.
    fn expand_walk(&mut self, pos: u32, targets: &mut [Target], max_len: f32) {
        let (px, py, pz, cg, cw) = {
            let n = &self.nodes[pos as usize];
            (n.x, n.y, n.z, n.g, n.walked)
        };
        // One grid access for the 3x3 cells around the node: the neighbours are the centre cell +- constants.
        let cp = self.centre(px, py, pz);
        let (above, current, height) = match cp {
            Some(cp) => {
                // SAFETY: `centre` keeps the cell above and the 3x3 cells inside the grid.
                let up = unsafe { cp.add(GW * GW) };
                // SAFETY: `cp` and `up` are grid cells, as above.
                unsafe {
                    self.fresh(cp);
                    self.fresh(up);
                }
                // SAFETY: `up` and `cp` are grid cells, as above.
                let (eu, ec) = unsafe { (*up, *cp) };
                let above = match (eu >> 48) as u8 {
                    0 => self.cached(px, py + 1, pz),
                    v => PathType::ALL[usize::from(v - 1)],
                };
                let current = match (ec >> 48) as u8 {
                    0 => self.cached(px, py, pz),
                    v => PathType::ALL[usize::from(v - 1)],
                };
                // SAFETY: `cp` is a grid cell, as above.
                let c = (unsafe { *cp } >> 56) as u8;
                let height = if c < 0xFE && !self.mob.can_float {
                    f64::from(py - 1) + f64::from(c) / 16.0
                } else {
                    self.floor_level(px, py, pz)
                };
                (above, current, height)
            }
            None => (self.cached(px, py + 1, pz), self.cached(px, py, pz), self.floor_level(px, py, pz)),
        };
        let mut jump = 0;
        if self.malus(above) >= 0.0 && current != PathType::StickyHoney {
            jump = self.mob.max_up_step.max(1.0).ifloor();
        }
        const DIRS: [(i32, i32, usize); 4] = [(0, -1, 2), (1, 0, 3), (0, 1, 0), (-1, 0, 1)];
        let pos_malus_neg = self.nodes[pos as usize].cost_malus < 0.0;
        let centre = if Self::partial_collision(current) { None } else { cp };
        let mut by2d: [Option<u32>; 4] = [None; 4];
        for (dx, dz, i2) in DIRS {
            let n = match centre {
                Some(cp) => {
                    // SAFETY: `centre` keeps the 3x3 cells inside the grid.
                    let q = unsafe { cp.offset(dx as isize + dz as isize * GW as isize) };
                    // SAFETY: `q` is one of those cells, as above.
                    unsafe { self.fresh(q) };
                    self.find_accepted_p(q, px + dx, py, pz + dz, jump, height, (dx, dz), current)
                }
                None => self.find_accepted(px + dx, py, pz + dz, jump, height, (dx, dz), current),
            };
            by2d[i2] = n;
            if let Some(n) = n {
                let node = &self.nodes[n as usize];
                if !node.closed && (node.cost_malus >= 0.0 || pos_malus_neg) {
                    self.relax(pos, (px, py, pz, cg, cw), n, targets, max_len);
                }
            }
        }
        for (dx, dz, i2) in DIRS {
            let (c, cdx, cdz) = match i2 {
                2 => (3, 1, 0),
                3 => (0, 0, 1),
                0 => (1, -1, 0),
                _ => (2, 0, -1),
            };
            if self.diagonal_pair_valid(pos, by2d[i2], by2d[c]) {
                let (qx, qz) = (dx + cdx, dz + cdz);
                let d = match centre {
                    Some(cp) => {
                        // SAFETY: as above.
                        let q = unsafe { cp.offset(qx as isize + qz as isize * GW as isize) };
                        // SAFETY: as above.
                        unsafe { self.fresh(q) };
                        self.find_accepted_p(q, px + qx, py, pz + qz, jump, height, (dx, dz), current)
                    }
                    None => self.find_accepted(px + qx, py, pz + qz, jump, height, (dx, dz), current),
                };
                if self.diagonal_valid(d) {
                    self.relax(pos, (px, py, pz, cg, cw), d.unwrap_or_default(), targets, max_len);
                }
            }
        }
    }
    #[inline(always)]
    fn relax(&mut self, current: u32, c: (i32, i32, i32, f32, f32), neighbor: u32, targets: &mut [Target], max_len: f32) {
        let (cx, cy, cz, cg, cw) = c;
        let nn = &mut self.nodes[neighbor as usize];
        let distance = nn.distance(cx, cy, cz);
        let walked = cw + distance;
        nn.walked = walked;
        let tentative = cg + distance + nn.cost_malus;
        let in_open = nn.heap >= 0;
        if walked < max_len && (!in_open || tentative < nn.g) {
            nn.came_from = Some(current);
            nn.g = tentative;
            let h = self.best_h(neighbor, targets) * 1.5;
            self.nodes[neighbor as usize].h = h;
            if in_open {
                self.change_cost(neighbor, tentative + h);
            } else {
                self.nodes[neighbor as usize].f = tentative + h;
                self.heap_insert(neighbor);
            }
        }
    }
    // --- swimming / flying neighbours ------------------------------------
    fn swim_accepted(&mut self, x: i32, y: i32, z: i32) -> Option<u32> {
        let breach = matches!(self.eval, Evaluator::Swim { breach: true });
        let t = self.cached(x, y, z);
        if (breach && t == PathType::Breach) || t == PathType::Water {
            let cost = self.malus(t);
            if cost >= 0.0 {
                let n = self.node_update_max(x, y, z, t, cost);
                if self.fluid(x, y, z) == Fluid::None {
                    self.nodes[n as usize].cost_malus += 8.0;
                }
                return Some(n);
            }
        }
        None
    }
    fn swim_neighbors(&mut self, pos: u32, out: &mut Vec<u32>) {
        let (px, py, pz) = {
            let n = &self.nodes[pos as usize];
            (n.x, n.y, n.z)
        };
        let open = |s: &Self, n: Option<u32>| n.is_some_and(|n| !s.nodes[n as usize].closed);
        let malus =
            |s: &Self, n: Option<u32>| n.is_some_and(|n| s.nodes[n as usize].cost_malus >= 0.0);
        // Direction.values(): down, up, north, south, west, east.
        let dirs = [
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
            (-1, 0, 0),
            (1, 0, 0),
        ];
        let mut by_dir: [Option<u32>; 6] = [None; 6];
        for (slot, (dx, dy, dz)) in by_dir.iter_mut().zip(dirs) {
            let n = self.swim_accepted(px + dx, py + dy, pz + dz);
            *slot = n;
            if open(self, n) {
                out.push(n.unwrap_or_default());
            }
        }
        for (a, b, dx, dz, cx, cz) in [
            (2usize, 5usize, 0i32, -1i32, 1i32, 0i32),
            (5usize, 3usize, 1i32, 0i32, 0i32, 1i32),
            (3usize, 4usize, 0i32, 1i32, -1i32, 0i32),
            (4usize, 2usize, -1i32, 0i32, 0i32, -1i32),
        ] {
            if malus(self, by_dir[a]) && malus(self, by_dir[b]) {
                let d = self.swim_accepted(px + dx + cx, py, pz + dz + cz);
                if open(self, d) {
                    out.push(d.unwrap_or_default());
                }
            }
        }
    }
    fn fly_accepted(&mut self, x: i32, y: i32, z: i32) -> Option<u32> {
        let t = self.cached(x, y, z);
        let cost = self.malus(t);
        if cost >= 0.0 {
            let n = self.node_update_max(x, y, z, t, cost);
            if t == PathType::Walkable {
                self.nodes[n as usize].cost_malus += 1.0;
            }
            return Some(n);
        }
        None
    }
    fn fly_neighbors(&mut self, pos: u32, out: &mut Vec<u32>) {
        let (px, py, pz) = {
            let n = &self.nodes[pos as usize];
            (n.x, n.y, n.z)
        };
        let f = |s: &mut Self, dx: i32, dy: i32, dz: i32| s.fly_accepted(px + dx, py + dy, pz + dz);
        let open = |s: &Self, n: Option<u32>| n.is_some_and(|n| !s.nodes[n as usize].closed);
        let m = |s: &Self, n: Option<u32>| n.is_some_and(|n| s.nodes[n as usize].cost_malus >= 0.0);
        let south = f(self, 0, 0, 1);
        let west = f(self, -1, 0, 0);
        let east = f(self, 1, 0, 0);
        let north = f(self, 0, 0, -1);
        let up = f(self, 0, 1, 0);
        let down = f(self, 0, -1, 0);
        for n in [south, west, east, north, up, down] {
            if open(self, n) {
                out.push(n.unwrap_or_default());
            }
        }
        let push = |s: &mut Self, n: Option<u32>, ok: bool, out: &mut Vec<u32>| {
            if open(s, n) && ok {
                out.push(n.unwrap_or_default());
            }
        };
        let south_up = f(self, 0, 1, 1);
        let ok = m(self, south) && m(self, up);
        push(self, south_up, ok, out);
        let west_up = f(self, -1, 1, 0);
        let ok = m(self, west) && m(self, up);
        push(self, west_up, ok, out);
        let east_up = f(self, 1, 1, 0);
        let ok = m(self, east) && m(self, up);
        push(self, east_up, ok, out);
        let north_up = f(self, 0, 1, -1);
        let ok = m(self, north) && m(self, up);
        push(self, north_up, ok, out);
        let south_down = f(self, 0, -1, 1);
        let ok = m(self, south) && m(self, down);
        push(self, south_down, ok, out);
        let west_down = f(self, -1, -1, 0);
        let ok = m(self, west) && m(self, down);
        push(self, west_down, ok, out);
        let east_down = f(self, 1, -1, 0);
        let ok = m(self, east) && m(self, down);
        push(self, east_down, ok, out);
        let north_down = f(self, 0, -1, -1);
        let ok = m(self, north) && m(self, down);
        push(self, north_down, ok, out);
        let north_east = f(self, 1, 0, -1);
        let ok = m(self, north) && m(self, east);
        push(self, north_east, ok, out);
        let south_east = f(self, 1, 0, 1);
        let ok = m(self, south) && m(self, east);
        push(self, south_east, ok, out);
        let north_west = f(self, -1, 0, -1);
        let ok = m(self, north) && m(self, west);
        push(self, north_west, ok, out);
        let south_west = f(self, -1, 0, 1);
        let ok = m(self, south) && m(self, west);
        push(self, south_west, ok, out);
        let neu = f(self, 1, 1, -1);
        let ok = m(self, north_east)
            && m(self, north)
            && m(self, east)
            && m(self, up)
            && m(self, north_up)
            && m(self, east_up);
        push(self, neu, ok, out);
        let seu = f(self, 1, 1, 1);
        let ok = m(self, south_east)
            && m(self, south)
            && m(self, east)
            && m(self, up)
            && m(self, south_up)
            && m(self, east_up);
        push(self, seu, ok, out);
        let nwu = f(self, -1, 1, -1);
        let ok = m(self, north_west)
            && m(self, north)
            && m(self, west)
            && m(self, up)
            && m(self, north_up)
            && m(self, west_up);
        push(self, nwu, ok, out);
        let swu = f(self, -1, 1, 1);
        let ok = m(self, south_west)
            && m(self, south)
            && m(self, west)
            && m(self, up)
            && m(self, south_up)
            && m(self, west_up);
        push(self, swu, ok, out);
        let ned = f(self, 1, -1, -1);
        let ok = m(self, north_east)
            && m(self, north)
            && m(self, east)
            && m(self, down)
            && m(self, north_down)
            && m(self, east_down);
        push(self, ned, ok, out);
        let sed = f(self, 1, -1, 1);
        let ok = m(self, south_east)
            && m(self, south)
            && m(self, east)
            && m(self, down)
            && m(self, south_down)
            && m(self, east_down);
        push(self, sed, ok, out);
        let nwd = f(self, -1, -1, -1);
        let ok = m(self, north_west)
            && m(self, north)
            && m(self, west)
            && m(self, down)
            && m(self, north_down)
            && m(self, west_down);
        push(self, nwd, ok, out);
        let swd = f(self, -1, -1, 1);
        let ok = m(self, south_west)
            && m(self, south)
            && m(self, west)
            && m(self, down)
            && m(self, south_down)
            && m(self, west_down);
        push(self, swd, ok, out);
    }
    // --- binary heap (vanilla BinaryHeap) --------------------------------
    fn heap_insert(&mut self, n: u32) {
        probe!(bump(22));
        let i = self.heap.len();
        self.heap.push((self.nodes[n as usize].f, n));
        self.nodes[n as usize].heap = i as i32;
        self.up_heap(i);
    }
    fn heap_pop(&mut self) -> u32 {
        probe!(bump(22));
        let popped = self.heap[0].1;
        let last = self.heap.pop().unwrap_or((0.0, popped));
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.nodes[last.1 as usize].heap = 0;
            self.down_heap(0);
        }
        self.nodes[popped as usize].heap = -1;
        popped
    }
    fn change_cost(&mut self, n: u32, cost: f32) {
        probe!(bump(22));
        let old = self.nodes[n as usize].f;
        self.nodes[n as usize].f = cost;
        let i = self.nodes[n as usize].heap as usize;
        self.heap[i].0 = cost;
        if cost < old {
            self.up_heap(i);
        } else {
            self.down_heap(i);
        }
    }
    fn up_heap(&mut self, mut i: usize) {
        let entry = self.heap[i];
        while i > 0 {
            probe!(bump(23));
            let parent_i = (i - 1) >> 1;
            let parent = self.heap[parent_i];
            if entry.0 >= parent.0 {
                break;
            }
            self.heap[i] = parent;
            self.nodes[parent.1 as usize].heap = i as i32;
            i = parent_i;
        }
        self.heap[i] = entry;
        self.nodes[entry.1 as usize].heap = i as i32;
    }
    fn down_heap(&mut self, mut i: usize) {
        let entry = self.heap[i];
        let size = self.heap.len();
        loop {
            probe!(bump(23));
            let left_i = 1 + (i << 1);
            let right_i = left_i + 1;
            if left_i >= size {
                break;
            }
            let left = self.heap[left_i];
            let right = if right_i >= size { (f32::INFINITY, u32::MAX) } else { self.heap[right_i] };
            if left.0 < right.0 {
                if left.0 >= entry.0 {
                    break;
                }
                self.heap[i] = left;
                self.nodes[left.1 as usize].heap = i as i32;
                i = left_i;
            } else {
                if right.0 >= entry.0 {
                    break;
                }
                self.heap[i] = right;
                self.nodes[right.1 as usize].heap = i as i32;
                i = right_i;
            }
        }
        self.heap[i] = entry;
        self.nodes[entry.1 as usize].heap = i as i32;
    }
    // --- PathFinder.findPath ------------------------------------------------
    #[inline(always)]
    fn best_h(&self, n: u32, targets: &mut [Target]) -> f32 {
        #[cfg(feature = "probes")]
        let _g = sec(15);
        if targets.is_empty() {
            return 0.0;
        }
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
    /// PathFinder.findPath over several target positions.
    pub fn find(
        mut self,
        positions: &[[i32; 3]],
        max_path_length: f32,
        reach_range: i32,
        max_visited: i32,
    ) -> Option<Path> {
        self.find_in(positions, max_path_length, reach_range, max_visited)
    }
    /// Every cell a search from the start expands within `max_path_length` (no target: a flood), with whether it ran to its end
    /// (`false`: it hit `max_visited`, the answer then depends on the target and the caller searches per target).
    /// `find` of `[target]` (reach 0) that also adds to `seen` the cells it closed: each is reached by a path of the search (blocked cells, which
    /// the node table marks closed without ever opening them, have no `came_from` and are left out). Gives whether the target was reached.
    pub fn find_reached(mut self, target: [i32; 3], max_len: f32, cap: i32, seen: &mut std::collections::HashSet<[i32; 3]>) -> bool {
        let r = self.find_in(&[target], max_len, 0, cap);
        seen.extend(self.nodes.iter().filter(|n| n.closed && n.came_from.is_some()).map(|n| [n.x, n.y, n.z]));
        r.is_some_and(|p| p.reached)
    }
    pub fn flood(mut self, max_path_length: f32, max_visited: i32) -> Option<(Vec<[i32; 3]>, bool)> {
        self.find_in(&[], max_path_length, 0, max_visited);
        let complete = self.visited < max_visited;
        let cells = self.nodes.iter().filter(|n| n.closed).map(|n| [n.x, n.y, n.z]).collect();
        Some((cells, complete))
    }
    fn find_in(
        &mut self,
        positions: &[[i32; 3]],
        max_path_length: f32,
        reach_range: i32,
        max_visited: i32,
    ) -> Option<Path> {
        #[cfg(feature = "probes")]
        let _g = sec(8);
        #[cfg(feature = "probes")]
        let g12 = sec(12);
        let from = self.start()?;
        probe!(drop(g12));
        let mut targets: Vec<Target> = positions
            .iter()
            .map(|p| {
                // AmphibiousNodeEvaluator.getTarget floors y + 0.5 (the same block).
                let n = self.node(p[0], p[1], p[2]);
                let node = &self.nodes[n as usize];
                Target {
                    x: node.x,
                    y: node.y,
                    z: node.z,
                    best_h: f32::MAX,
                    best: None,
                    reached: false,
                    pos: *p,
                }
            })
            .collect();
        self.nodes[from as usize].g = 0.0;
        let h = self.best_h(from, &mut targets);
        self.nodes[from as usize].h = h;
        self.nodes[from as usize].f = h;
        self.heap.clear();
        self.heap_insert(from);
        let mut count = 0;
        let walk_fast = matches!(self.eval, Evaluator::Walk) && self.use_layer && self.use_grid;
        let mut neighbors = Vec::new();
        let (fx, fy, fz) = {
            let n = &self.nodes[from as usize];
            (n.x, n.y, n.z)
        };
        while !self.heap.is_empty() {
            count += 1;
            self.visited = count;
            if count >= max_visited {
                break;
            }
            probe!(bump(0));
            #[cfg(feature = "probes")]
            let g7 = sec(7);
            let current = self.heap_pop();
            probe!(drop(g7));
            self.nodes[current as usize].closed = true;
            let mut any = false;
            for t in targets.iter_mut() {
                if self.nodes[current as usize].manhattan(t.x, t.y, t.z) <= reach_range as f32 {
                    t.reached = true;
                    any = true;
                }
            }
            if any {
                break;
            }
            if self.nodes[current as usize].distance(fx, fy, fz) >= max_path_length {
                continue;
            }
            if walk_fast {
                probe! { if REP > 0 { self.expand_walk(current, &mut targets, max_path_length); } }
                self.expand_walk(current, &mut targets, max_path_length);
                continue;
            }
            neighbors.clear();
            match self.eval {
                Evaluator::Walk | Evaluator::Amphibious { .. } => {
                    self.walk_neighbors(current, &mut neighbors)
                }
                Evaluator::Swim { .. } => self.swim_neighbors(current, &mut neighbors),
                Evaluator::Fly => self.fly_neighbors(current, &mut neighbors),
            }
            #[cfg(feature = "probes")]
            let g6 = sec(6);
            let (cx, cy, cz, cg, cw) = {
                let c = &self.nodes[current as usize];
                (c.x, c.y, c.z, c.g, c.walked)
            };
            for &neighbor in &neighbors {
                probe!(bump(1));
                let nn = &mut self.nodes[neighbor as usize];
                let distance = nn.distance(cx, cy, cz);
                let walked = cw + distance;
                nn.walked = walked;
                let tentative = cg + distance + nn.cost_malus;
                let in_open = nn.heap >= 0;
                if walked < max_path_length && (!in_open || tentative < nn.g) {
                    nn.came_from = Some(current);
                    nn.g = tentative;
                    let h = self.best_h(neighbor, &mut targets) * 1.5;
                    self.nodes[neighbor as usize].h = h;
                    if in_open {
                        self.change_cost(neighbor, tentative + h);
                    } else {
                        self.nodes[neighbor as usize].f = tentative + h;
                        self.heap_insert(neighbor);
                    }
                }
            }
            probe!(drop(g6));
        }
        probe!(LAST_VISITED.store(count, std::sync::atomic::Ordering::Relaxed));
        #[cfg(feature = "probes")]
        let g11 = sec(11);
        let reached: Vec<&Target> = targets.iter().filter(|t| t.reached).collect();
        let build = |s: &Self, t: &Target, reached: bool| -> Option<Path> {
            let mut n = t.best?;
            let mut nodes = Vec::with_capacity(48);
            loop {
                let node = &s.nodes[n as usize];
                nodes.push(PathNode {
                    x: node.x,
                    y: node.y,
                    z: node.z,
                    kind: node.kind,
                });
                match node.came_from {
                    Some(c) => n = c,
                    None => break,
                }
            }
            nodes.reverse();
            Some(make_path(nodes, t.pos, reached))
        };
        let out = if !reached.is_empty() {
            reached
                .iter()
                .filter_map(|t| build(&self, t, true))
                .min_by_key(|p| p.nodes.len())
        } else {
            targets
                .iter()
                .filter_map(|t| build(&self, t, false))
                .min_by(|a, b| {
                    a.dist_to_target
                        .total_cmp(&b.dist_to_target)
                        .then(a.nodes.len().cmp(&b.nodes.len()))
                })
        };
        probe!(drop(g11));
        out
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

struct Target {
    x: i32,
    y: i32,
    z: i32,
    best_h: f32,
    best: Option<u32>,
    reached: bool,
    pos: [i32; 3],
}
fn vanilla_burning_block(block: &str, emits_light: bool, in_fire_tag: bool) -> bool {
    in_fire_tag || matches!(block, "minecraft:lava" | "minecraft:magma_block" | "minecraft:lava_cauldron") || matches!(block, "minecraft:campfire" | "minecraft:soul_campfire") && emits_light
}
fn vanilla_adjacent(t: PathType) -> Option<PathType> {
    match t {
        PathType::Damaging => Some(PathType::DamagingInNeighbor),
        PathType::Fire | PathType::Lava => Some(PathType::FireInNeighbor),
        PathType::Water => Some(PathType::WaterBorder),
        PathType::DamageCautious => Some(PathType::DamageCautious),
        _ => None,
    }
}

fn block_pos(m: &PathMob) -> [i32; 3] {
    [m.position.x.ifloor(), m.position.y.ifloor(), m.position.z.ifloor()]
}
fn make_path(nodes: Vec<PathNode>, target: [i32; 3], reached: bool) -> Path {
    let dist_to_target = nodes.last().map_or(f32::MAX, |n| {
        ((n.x - target[0]).abs() + (n.y - target[1]).abs() + (n.z - target[2]).abs()) as f32
    });
    Path { nodes, next: 0, target, dist_to_target, reached }
}

/// The distinct path types of a box, in a fixed array (the box of a mob is a few cells).
struct Small {
    a: [PathType; 24],
    n: usize,
}
impl Small {
    fn new() -> Self {
        Self { a: [PathType::Blocked; 24], n: 0 }
    }
    fn add(&mut self, t: PathType) {
        if !self.a[..self.n].contains(&t) {
            // A box of more than 24 distinct types does not exist (the enum has fewer than 24 values that a cell gives).
            self.a[self.n.min(23)] = t;
            self.n = (self.n + 1).min(24);
        }
    }
    fn sort(&mut self) {
        self.a[..self.n].sort_unstable_by_key(|t| *t as u8);
    }
    fn as_slice(&self) -> &[PathType] {
        &self.a[..self.n]
    }
}

// --- the shared layer of walk path types, per 16 x 16 x 16 section ------------------------------
/// `path_type_static` of a cell does not depend on the mob: one byte per block (0 unknown, else 1 + the type), filled the first
/// time a search asks, shared by every search, dropped around a change (`layer_changed`). 4096 bytes a section.
/// The herd's navigation uses this search instead of Horizon's `Finder` (bench option `OWN`).
#[cfg(feature = "probes")]
pub static USE_OWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
#[cfg(feature = "probes")]
pub static LAYER_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// The bench switches, their default without the `probes` feature.
#[cfg(feature = "probes")]
pub fn use_own() -> bool { USE_OWN.load(std::sync::atomic::Ordering::Relaxed) }
#[cfg(not(feature = "probes"))]
pub fn use_own() -> bool { true }
#[cfg(feature = "probes")]
pub fn layer_on() -> bool { LAYER_ON.load(std::sync::atomic::Ordering::Relaxed) }
#[cfg(not(feature = "probes"))]
pub fn layer_on() -> bool { true }
/// `stat`: 0 unknown, else 1 + the type; `top`: 0xFF unknown, else the collision top of the block in sixteenths (0xFE: not a multiple of 1/16, read the block).
pub struct Sect {
    stat: [u8; 4096],
    top: [u8; 4096],
}
/// Bumped at every block change: each thread's layer forgets what it knew when it sees a newer epoch (the layer is per thread, a change is seen by all).
pub static LAYER_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn layer_bump() {
    LAYER_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Release);
}
pub struct Layer {
    epoch: u64,
    map: FxHashMap<u64, Box<Sect>>,
    /// By (profile, section): the type a mob of that profile gives a cell (`path_type_of_mob`), one byte a block like `stat`.
    pmap: FxHashMap<(u64, u64), Box<[u8; 4096]>>,
    /// The profiles that have a section in `pmap`.
    profiles: Vec<(u64, i32, i32)>,
}
thread_local! {
    static LAYER: std::cell::RefCell<Layer> = std::cell::RefCell::new(Layer { epoch: 0, map: FxHashMap::default(), pmap: FxHashMap::default(), profiles: Vec::new() });
}
thread_local! {
    static KSTAMP: std::cell::Cell<u16> = const { std::cell::Cell::new(0) };
}
/// Bench only: switches the amphibious fast path off, to measure its gain in one binary.
#[cfg(feature = "probes")]
pub static NO_AMPH_FAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "probes")]
pub fn no_amph_fast() -> bool { NO_AMPH_FAST.load(std::sync::atomic::Ordering::Relaxed) }
#[cfg(not(feature = "probes"))]
pub fn no_amph_fast() -> bool { false }
/// Bench only: nodes opened by the last search.
#[cfg(feature = "probes")]
pub static LAST_VISITED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
const AMPH_BIT: u64 = 1 << 63;
const AMPH_MIX: u64 = 0x9E37_79B9_7F4A_7C15;
fn layer_key(x: i32, y: i32, z: i32) -> u64 {
    (((x >> 4) as u64) & 0x1F_FFFF) | ((((z >> 4) as u64) & 0x1F_FFFF) << 21) | ((((y >> 4) as u64) & 0x1F_FFFF) << 42)
}
fn layer_idx(x: i32, y: i32, z: i32) -> usize {
    (((y & 15) << 8) | ((z & 15) << 4) | (x & 15)) as usize
}
impl Layer {
    fn profile_section(&mut self, pr: (u64, i32, i32), key: u64) -> *mut [u8; 4096] {
        let profile = pr.0;
        if !self.profiles.iter().any(|q| q.0 == profile) {
            self.profiles.push(pr);
        }
        &mut **self.pmap.entry((profile, key)).or_insert_with(|| Box::new([0u8; 4096])) as *mut [u8; 4096]
    }
    fn section(&mut self, key: u64) -> *mut Sect {
        &mut **self.map.entry(key).or_insert_with(|| Box::new(Sect { stat: [0; 4096], top: [0xFF; 4096] })) as *mut Sect
    }
}
/// The block at (x, y, z) changed: the types of the cells that read it (itself, its 26 neighbours, the cell above) are forgotten,
/// and a mob's types of the cells whose box (up to 5 wide and 5 tall) or neighbourhood reaches it.
pub fn layer_changed(x: i32, y: i32, z: i32) {
    LAYER.with(|l| {
        let mut l = l.borrow_mut();
        let l = &mut *l;
        // Visits the cells of the box, section by section: `f(key, section, cells)` with the ranges clipped to the section.
        fn boxes(lo: [i32; 3], hi: [i32; 3], mut f: impl FnMut(u64, [i32; 3], [i32; 3])) {
            for sx in (lo[0] >> 4)..=(hi[0] >> 4) {
                for sy in (lo[1] >> 4)..=(hi[1] >> 4) {
                    for sz in (lo[2] >> 4)..=(hi[2] >> 4) {
                        let (bx, by, bz) = (sx << 4, sy << 4, sz << 4);
                        f(
                            layer_key(bx, by, bz),
                            [lo[0].max(bx), lo[1].max(by), lo[2].max(bz)],
                            [hi[0].min(bx + 15), hi[1].min(by + 15), hi[2].min(bz + 15)],
                        );
                    }
                }
            }
        }
        // Rows of the box inside one section: `g(start, end)` over the cell indices of each row.
        fn each(a: [i32; 3], b: [i32; 3], mut g: impl FnMut(usize, usize)) {
            for yy in a[1]..=b[1] {
                for zz in a[2]..=b[2] {
                    let base = (((yy & 15) << 8) | ((zz & 15) << 4)) as usize;
                    g(base | (a[0] & 15) as usize, (base | (b[0] & 15) as usize) + 1);
                }
            }
        }
        // A cell's type of a mob `ew` wide and `eh` tall reads the blocks within `ew + 1` below/left of it and 1 right, 3 above.
        for i in 0..l.profiles.len() {
            let (p, ew, eh) = l.profiles[i];
            boxes([x - ew - 1, y - eh - 1, z - ew - 1], [x + 1, y + 3, z + 1], |key, a, b| {
                if let Some(v) = l.pmap.get_mut(&(p, key)) {
                    each(a, b, |i, j| v[i..j].fill(0));
                }
            });
        }
        boxes([x - 1, y - 1, z - 1], [x + 1, y + 2, z + 1], |key, a, b| {
            for k in [key, key | AMPH_BIT] {
                if let Some(sct) = l.map.get_mut(&k) {
                    each(a, b, |i, j| {
                        sct.stat[i..j].fill(0);
                        sct.top[i..j].fill(0xFF);
                    });
                }
            }
        });
    });
}
/// Drops the whole layer (a chunk replaced, a world reloaded).
pub fn layer_clear() {
    LAYER.with(|l| {
        let mut l = l.borrow_mut();
        l.map.clear();
        l.pmap.clear();
        l.profiles.clear();
    });
}
/// The most bytes the layer keeps (per thread) before the next search starts by dropping the types per profile (they are re-derived
/// from `stat`), then the sections if that is not enough.
pub static LAYER_CAP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(256 << 20);
fn layer_evict() {
    LAYER.with(|l| {
        let mut l = l.borrow_mut();
        let cap = LAYER_CAP.load(std::sync::atomic::Ordering::Relaxed);
        if l.map.len() * 8192 + l.pmap.len() * 4096 > cap {
            l.pmap.clear();
            l.profiles.clear();
            if l.map.len() * 8192 > cap {
                l.map.clear();
            }
        }
    });
}
/// (sections held, bytes).
pub fn layer_size() -> (usize, usize) {
    LAYER.with(|l| (l.borrow().map.len(), l.borrow().map.len() * 8192 + l.borrow().pmap.len() * 4096))
}

#[cfg(feature = "probes")]
static OVH: [std::sync::atomic::AtomicU64; 16] = [const { std::sync::atomic::AtomicU64::new(0) }; 16];
#[cfg(feature = "probes")]
pub fn set_ovh(i: usize) {
    OVH[i].store(calibrate(i), std::sync::atomic::Ordering::Relaxed);
}
#[cfg(feature = "probes")]
pub fn ovh(i: usize) -> u64 {
    OVH[i].load(std::sync::atomic::Ordering::Relaxed)
}

/// What a mob's search reads of it: the box, the doors, the maluses. Two mobs of the same profile give a cell the same type.
fn profile_of(mob: &PathMob) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h = (h ^ v).wrapping_mul(0x0000_0100_0000_01B3);
    };
    mix(u64::from(mob.width.to_bits()));
    mix(u64::from(mob.height.to_bits()));
    mix(u64::from(mob.can_open_doors));
    mix(u64::from(mob.can_pass_doors));
    for m in mob.malus.iter() {
        mix(m.map_or(u64::MAX, |v| u64::from(v.to_bits())));
    }
    h
}
