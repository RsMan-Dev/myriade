//! Occupancy masks and a table of shapes beside the world (suivi/ecs-noyau-collisions.md).
//!
//! Two levels, none invalidated:
//! - the SHAPE TABLE, per block state, permanent: the class of a state (empty, plain cube, a fixed shape, or "hard": a shape that
//!   depends on the entity, on the world, or a moving piston) and, for a fixed shape, the shape itself, computed the first time a
//!   state is met and kept;
//! - the MASKS, per 16 x 16 x 16 section and per row of 16 cells in x, four u16 words (plain cube, fixed shape, fixed shape that
//!   reaches out of its cell, hard). Built when a section is first read from the blocks the chunk holds; when a block changes,
//!   `changed` flips the bits of its cell (a few instructions), nothing is rebuilt. The state of a shaped cell is read from the chunk
//!   when its shape is needed.
//!
//! `resolve` is the block kernel's `Entity.collide` with the collection of colliders replaced: it walks the cells of the swept box
//! and its ring in the scan's order (z, then y, then x; the ring's cells only where the scan reads them) and feeds the same
//! `try_resolve_step_movement_in` with colliders that do the kernel's arithmetic: `collide_unit_cube` for a plain cube,
//! `collide_moved` for a fixed shape (the kernel's `BlockCollider::Moved`, without cloning the shape). A HARD cell (a shape that
//! depends on the entity, a block entity, a moving piston, a column offset) is asked of the kernel alone, for that one cell, with the
//! context the kernel's own scan would give it (`ask`), and its collider takes its place in the order: the cost follows the number of
//! hard cells met. The whole kernel runs only when the box is too big or not finite (`Err` from `collect`); nothing else is left to it
//! (a world border and entity colliders do not go through here).
mod fast;
use horizon_client_render::collisions::SceneCollisions;
use horizon_core::{position::BlockPos, random::position_seed};
use horizon_world::{collision_context::{CollisionContext, CollisionEntity, CollisionItem, EntityContextKind}, entity_obstruction::EntityIdentity};
use horizon_core::geometry::{try_resolve_step_movement_in, Aabb, Axis, Collider, ShapeOp, Vec3, VoxelShape};
use horizon_world::registry::BlockStateId;
use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
    sync::{
        atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering::Relaxed},
        Mutex,
    },
};

type Binding<'a> = horizon_world::block_collision::BlockCollisionRegistry<'a>;
type World = horizon_client_render::world::World;

pub struct Section {
    full: [u16; 256],
    /// Plain cubes, fixed shapes and hard cells: what the scan reads in the box's own cells.
    near: [u16; 256],
    shaped: [u16; 256],
    large: [u16; 256],
    hard: [u16; 256],
    /// Hard or liquid cells whose state is a moving piston (the scan keeps those on the second outer layer).
    piston: [u16; 256],
    /// Water and lava: empty for a body with no subject (`CollisionContext::position`), the kernel's business for one that stands on fluids.
    liquid: [u16; 256],
    any_cells: bool,
    any_hard: bool,
    any_liquid: bool,
    any_large: bool,
    any_piston: bool,
    any_shaped: bool,
    /// What the lock-free path asks of a section at a glance: 1 holds cells, 2 holds a shape that is not a cube or a hard cell, 4 holds a shape
    /// that reaches out of its cell or a moving piston, 8 holds a liquid. Only ever turned on.
    /// (Bit 16, set under the lock while other threads read: this section or one of its 26 neighbours holds a shape that reaches out of its cell
    /// or a moving piston, so the scan reads cells of the ring that lie in other sections.)
    flags: AtomicU8,
    /// The x columns (bits) in which a cell has ever held a shape that is not a cube or a hard cell (`dirty_x`), a liquid (`liquid_x`), a shape reaching out of
    /// its cell or a moving piston (`large_x`). Only ever turned on, so a superset of what the rows hold: a box away from those columns reads no row for them.
    dirty_x: AtomicU16,
    liquid_x: AtomicU16,
    large_x: AtomicU16,
    /// Which of the 26 neighbours hold such a shape (bit `rel` of the neighbour's offset: (dx + 1) * 9 + (dy + 1) * 3 + dz + 1).
    halo: AtomicU32,
}
impl Section {
    fn void() -> Self {
        Section { full: [0; 256], near: [0; 256], shaped: [0; 256], large: [0; 256], hard: [0; 256], piston: [0; 256], liquid: [0; 256], any_cells: false, any_hard: false, any_liquid: false, any_large: false, any_piston: false, any_shaped: false, flags: AtomicU8::new(0), dirty_x: AtomicU16::new(0), liquid_x: AtomicU16::new(0), large_x: AtomicU16::new(0), halo: AtomicU32::new(0) }
    }
    fn refresh_flags(&self) {
        let own = u8::from(self.any_cells) | u8::from(self.any_shaped || self.any_hard) << 1 | u8::from(self.any_large || self.any_piston) << 2 | u8::from(self.any_liquid) << 3;
        self.flags.fetch_and(16, Relaxed);
        self.flags.fetch_or(own, Relaxed);
    }
    fn set(&mut self, row: usize, bit: u16, class: Class) {
        self.set_bits(row, bit, class);
        self.refresh_flags();
    }
    fn set_bits(&mut self, row: usize, bit: u16, class: Class) {
        let clear = !bit;
        self.full[row] &= clear;
        self.near[row] &= clear;
        // A class a section has never held has no bit to clear (the flags only ever turn on).
        if self.any_shaped {
            self.shaped[row] &= clear;
        }
        if self.any_large {
            self.large[row] &= clear;
        }
        if self.any_hard {
            self.hard[row] &= clear;
        }
        if self.any_liquid {
            self.liquid[row] &= clear;
        }
        if self.any_piston {
            self.piston[row] &= clear;
        }
        match class {
            Class::Empty => {}
            Class::Cube => {
                self.full[row] |= bit;
                self.near[row] |= bit;
                self.any_cells = true;
            }
            Class::Shaped { .. } | Class::Column { .. } => {
                let large = matches!(class, Class::Shaped { large: true, .. });
                self.shaped[row] |= bit;
                self.any_shaped = true;
                self.near[row] |= bit;
                self.dirty_x.fetch_or(bit, Relaxed);
                if large {
                    self.large_x.fetch_or(bit, Relaxed);
                    self.large[row] |= bit;
                    self.any_large = true;
                }
                self.any_cells = true;
            }
            Class::Hard { large, piston } => {
                self.hard[row] |= bit;
                self.near[row] |= bit;
                self.any_hard = true;
                self.any_cells = true;
                self.dirty_x.fetch_or(bit, Relaxed);
                if large {
                    self.large_x.fetch_or(bit, Relaxed);
                    self.large[row] |= bit;
                    self.any_large = true;
                }
                if piston {
                    self.large_x.fetch_or(bit, Relaxed);
                    self.piston[row] |= bit;
                    self.any_piston = true;
                }
            }
            Class::Liquid { large } => {
                self.liquid[row] |= bit;
                self.any_liquid = true;
                self.any_cells = true;
                self.liquid_x.fetch_or(bit, Relaxed);
                if large {
                    self.large_x.fetch_or(bit, Relaxed);
                    self.large[row] |= bit;
                    self.any_large = true;
                }
            }
        }
    }
}
static VOID: Section = Section { full: [0; 256], near: [0; 256], shaped: [0; 256], large: [0; 256], hard: [0; 256], piston: [0; 256], liquid: [0; 256], any_cells: false, any_hard: false, any_liquid: false, any_large: false, any_piston: false, any_shaped: false, flags: AtomicU8::new(0), dirty_x: AtomicU16::new(0), liquid_x: AtomicU16::new(0), large_x: AtomicU16::new(0), halo: AtomicU32::new(0) };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Empty,
    Cube,
    /// A fixed shape (index in the table, from 0), large when it reaches out of its cell.
    Shaped { shape: u32, large: bool },
    /// A fixed shape moved by an offset that is a pure function of the column (x, z): bamboo, pointed dripstone, sulfur spike
    /// (`DynamicSupport::offset`). `slot` indexes the table's moved shapes of this state, 256 per slot (one per offset). The kernel flags these states large (no cached shape), but the moved shape never leaves its cell
    /// (a test checks every offset): such a shape cannot meet a box in a ring cell, so the ring is not read for it.
    Column { slot: u32 },
    /// The kernel is asked for the shape of this one cell (entity context, world, block entity, column offset); `large` and
    /// `piston`: the scan keeps it on the outer layers too (`CollisionBlock`).
    Hard { large: bool, piston: bool },
    /// Water or lava: no collision for a body with no subject.
    Liquid { large: bool },
}
fn encode(c: Class) -> u32 {
    match c {
        Class::Empty => 1,
        Class::Cube => 2,
        Class::Hard { large, piston } => 3 | u32::from(large) << 3 | u32::from(piston) << 4,
        Class::Liquid { large } => 4 | u32::from(large) << 3,
        Class::Shaped { shape, large } => 5 | u32::from(large) << 3 | shape << 5,
        Class::Column { slot } => 6 | slot << 5,
    }
}
fn decode(n: u32) -> Class {
    let (large, piston) = (n >> 3 & 1 != 0, n >> 4 & 1 != 0);
    match n & 7 {
        1 => Class::Empty,
        2 => Class::Cube,
        3 => Class::Hard { large, piston },
        4 => Class::Liquid { large },
        6 => Class::Column { slot: n >> 5 },
        _ => Class::Shaped { shape: n >> 5, large },
    }
}

#[derive(Default)]
struct KeyHash(u64);
impl Hasher for KeyHash {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = i.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}
type Map = HashMap<u64, Box<Section>, BuildHasherDefault<KeyHash>>;
/// A section's key: 21 bits per axis.
fn key(k: [i32; 3]) -> u64 {
    (k[0] as u64 & 0x1F_FFFF) | (k[1] as u64 & 0x1F_FFFF) << 21 | (k[2] as u64 & 0x1F_FFFF) << 42
}

/// A collider of the kernel's own arithmetic: the unit cube at an offset, or a stored shape at an offset (`BlockCollider::Moved`).
#[derive(Clone, Copy)]
enum Occ {
    Cube(Vec3),
    /// The address of a shape of the registry's cache (never freed or moved while the registry lives) and its offset.
    Shape(usize, Vec3),
    /// The address of a shape already moved (the kernel's `BlockCollider::Shape`): a shape that only grazes the box, joined with it.
    Joined(usize),
}
impl Occ {
    #[inline]
    fn shape(&self) -> &VoxelShape {
        match self {
            // SAFETY: the address of a shape of the registry's collision cache, which the world keeps for its whole life (or of `held`).
            Occ::Shape(p, _) => unsafe { &*(*p as *const VoxelShape) },
            Occ::Cube(_) => unreachable!(),
            Occ::Joined(p) => unsafe { &*(*p as *const VoxelShape) },
        }
    }
}
impl Collider for Occ {
    #[inline]
    fn collide(&self, axis: Axis, aabb: Aabb, amount: f64) -> f64 {
        match self {
            Occ::Cube(offset) => VoxelShape::collide_unit_cube(*offset, axis, aabb, amount),
            Occ::Shape(_, offset) => self.shape().collide_moved(*offset, axis, aabb, amount),
            Occ::Joined(_) => self.shape().collide(axis, aabb, amount),
        }
    }
    #[inline]
    fn y_len(&self) -> usize {
        match self {
            Occ::Cube(_) => 2,
            Occ::Shape(..) | Occ::Joined(_) => self.shape().coordinates(Axis::Y).len(),
        }
    }
    #[inline]
    fn y_at(&self, k: usize) -> f64 {
        match self {
            Occ::Cube(offset) => k as f64 + offset.y,
            Occ::Shape(_, offset) => self.shape().coordinates(Axis::Y)[k] + offset.y,
            Occ::Joined(_) => self.shape().coordinates(Axis::Y)[k],
        }
    }
}

/// The classes of the block states, filled on first sight, and the fixed shapes.
#[derive(Default)]
struct Table {
    /// Per state: 0 unknown, else `encode`.
    class: Vec<u32>,
    /// The distinct fixed shapes, by address in the registry's own cache (nothing is copied; the registry outlives the masks).
    shapes: Vec<usize>,
    shape_at: HashMap<usize, u32>,
    /// Per column-offset state (`Class::Column`): the shapes the kernel gave, moved, by offset key (an address in `column_shapes`, 0: not met).
    columns: Vec<Option<Box<[usize; 256]>>>,
    column_shapes: Vec<Box<VoxelShape>>,
    /// States classified (first encounters), for the measurements.
    met: u64,
}
impl Table {
    fn class_of(&mut self, binding: &Binding<'_>, s: BlockStateId) -> Class {
        let i = s.0 as usize;
        if i >= self.class.len() {
            self.class.resize(i + 1024, 0);
        }
        let c = self.class[i];
        if c == 0 {
            let c = self.classify(binding, s);
            self.class[i] = encode(c);
            self.met += 1;
            return c;
        }
        decode(c)
    }
    fn classify(&mut self, binding: &Binding<'_>, s: BlockStateId) -> Class {
        let Ok(block) = binding.collision_block(s) else { return Class::Hard { large: true, piston: true } };
        if !binding.may_collide(s) && !binding.reaches_out(s) {
            return Class::Empty;
        }
        if binding.is_plain_cube(s) {
            return Class::Cube;
        }
        let hard = Class::Hard { large: block.large_collision_shape, piston: block.moving_piston };
        // The shape of such a state is its cached one, whatever the entity: no contextual rule and no variants (flag 8), no
        // block entity or world to ask (a cached shape), no piston in motion.
        if binding.has_context_rule(s) {
            // `LiquidCollision`: water and lava (no variants) are empty unless the body stands on fluids or collides with all of them.
            let liquid = matches!(binding.registry().state(s).map(|st| st.name.as_str()), Some("minecraft:water" | "minecraft:lava")) && binding.cached_shapes().collision_variants(s).next().is_none();
            return if liquid { Class::Liquid { large: block.large_collision_shape } } else { hard };
        }
        if block.empty_shape || block.moving_piston {
            return hard;
        }
        // Bamboo and the pointed stalactites: no cached shape, a fixed shape moved by an offset of the column.
        if matches!(binding.cached_shapes().cached_shape(s), Ok(None))
            && matches!(binding.registry().state(s).map(|st| st.name.as_str()), Some("minecraft:bamboo" | "minecraft:pointed_dripstone" | "minecraft:sulfur_spike"))
        {
            self.columns.push(None);
            return Class::Column { slot: self.columns.len() as u32 - 1 };
        }
        let Ok(Some(shape)) = binding.cached_shapes().cached_shape(s) else { return hard };
        if shape.is_empty() || shape.is_block() || shape.bounds().is_none() {
            return hard;
        }
        let addr = shape as *const VoxelShape as usize;
        let n = *self.shape_at.entry(addr).or_insert_with(|| {
            self.shapes.push(addr);
            self.shapes.len() as u32 - 1
        });
        Class::Shaped { shape: n, large: block.large_collision_shape }
    }
}

const HELD: usize = 4096;
struct Inner {
    map: Map,
    /// The last sections looked up (direct-mapped by key): no hashing for the sections a walking body keeps asking for.
    recent: [(u64, usize); 64],
    colliders: Vec<Occ>,
    /// The shapes the kernel gave for hard cells during one call: `Occ::Shape` points into it (reserved once, never grown past\n    /// its capacity, so the addresses hold).
    held: Vec<VoxelShape>,
    table: Table,
}
impl Default for Inner {
    fn default() -> Self {
        Inner { map: Map::default(), recent: [(0, 0); 64], colliders: Vec::new(), held: Vec::with_capacity(HELD), table: Table::default() }
    }
}
/// The masks and the table of one world, and how often the fast path answered.
pub struct Occupancy {
    inner: Mutex<Inner>,
    pub fast: AtomicU64,
    pub fallback: AtomicU64,
    pub built: AtomicU64,
    /// Calls the lock-free path declined (they took the general one).
    pub declined: AtomicU64,
    /// Which instance this is (the per-thread caches of `fast` belong to one), and how many times the masks were dropped by `clear`.
    id: u64,
    generation: AtomicU64,
}
impl Default for Occupancy {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Occupancy { inner: Mutex::default(), fast: AtomicU64::default(), fallback: AtomicU64::default(), built: AtomicU64::default(), declined: AtomicU64::default(), id: NEXT.fetch_add(1, Relaxed), generation: AtomicU64::default() }
    }
}

impl Occupancy {
    /// The section `k` (key `key`) of the masks, built from the blocks on its first read; its address holds until `clear`.
    fn section(&self, world: &World, binding: &Binding<'_>, key: u64, k: [i32; 3]) -> *const Section {
        let mut g = self.inner.lock().unwrap();
        let Inner { map, table, .. } = &mut *g;
        if let Some(s) = map.get(&key) {
            return &**s as *const Section;
        }
        self.built.fetch_add(1, Relaxed);
        let s = get_or_build(map, table, world, binding, k);
        &**s as *const Section
    }
    /// The block at (x, y, z) has just been written: its cell takes the class of its new state (the bits of its row), if the
    /// section has a mask. Nothing else is touched. Whoever writes blocks calls this (and after replacing a chunk, `clear`).
    pub fn changed(&self, world: &World, binding: &Binding<'_>, x: i32, y: i32, z: i32) {
        let mut g = self.inner.lock().unwrap();
        let Inner { map, table, .. } = &mut *g;
        let Some(section) = map.get_mut(&key([x >> 4, y >> 4, z >> 4])) else { return };
        let class = table.class_of(binding, world.block(x, y, z));
        let before = section.flags.load(Relaxed) & 4;
        section.set((((y & 15) << 4) | (z & 15)) as usize, 1 << (x & 15), class);
        if before == 0 && section.flags.load(Relaxed) & 4 != 0 {
            mark_halo(map, [x >> 4, y >> 4, z >> 4]);
        }
    }
    /// Drops every mask (a chunk replaced, a world reloaded); the table stays.
    pub fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.map.clear();
        g.recent = [(0, 0); 64];
        self.generation.fetch_add(1, Relaxed);
    }
    /// The class of a state, by name, for the tests: "empty", "cube", "shaped", "large shaped" or "hard".
    pub fn class_name(&self, binding: &Binding<'_>, s: BlockStateId) -> &'static str {
        match self.inner.lock().unwrap().table.class_of(binding, s) {
            Class::Empty => "empty",
            Class::Cube => "cube",
            Class::Shaped { large: false, .. } => "shaped",
            Class::Shaped { large: true, .. } => "large shaped",
            Class::Column { .. } => "column",
            Class::Hard { .. } => "hard",
            Class::Liquid { .. } => "liquid",
        }
    }
    /// The class of a state as a number, for the measurements: 0 empty, 1 plain cube, 2 fixed shape, 3 hard, 4 liquid.
    pub fn probe_class(&self, binding: &Binding<'_>, s: BlockStateId) -> u8 {
        match self.inner.lock().unwrap().table.class_of(binding, s) {
            Class::Empty => 0,
            Class::Cube => 1,
            Class::Shaped { .. } | Class::Column { .. } => 2,
            Class::Hard { .. } => 3,
            Class::Liquid { .. } => 4,
        }
    }
    /// The memory of one section's masks, in bytes (the words and flags; the shapes are in the table, per state).
    pub const fn section_bytes() -> usize {
        std::mem::size_of::<Section>()
    }
    /// How many block states the table has classified, and how many shapes it holds.
    pub fn table_size(&self) -> (u64, usize) {
        let g = self.inner.lock().unwrap();
        (g.table.met, g.table.shapes.len())
    }

    /// `SceneCollisions::resolve_in_border_with(bounds, movement, step, on_ground, None, &[])`, the same result bit for bit.
    pub fn resolve(&self, mut scene: SceneCollisions<'_, '_>, bounds: Aabb, movement: Vec3, max_step: f32, on_ground: bool) -> Result<Vec3, Box<dyn std::error::Error>> {
        // Plain cubes and air only, no lock: the common case.
        if let Some(v) = fast::resolve(self, &scene, bounds, movement, max_step, on_ground) {
            self.fast.fetch_add(1, Relaxed);
            return Ok(v);
        }
        self.declined.fetch_add(1, Relaxed);
        {
            let mut g = self.inner.lock().unwrap();
            let Inner { map, recent, colliders, held, table } = &mut *g;
            colliders.clear();
            // The kernel's context of the whole call is the entity's, from the box the call began with (`y0`).
            let y0 = bounds.min_position().y;
            let r = try_resolve_step_movement_in(movement, bounds, max_step, on_ground, colliders, |b, out| {
                collect(map, recent, table, held, &self.built, &mut scene, (y0, true), b, out)
            });
            colliders.clear();
            if !held.is_empty() {
                held.clear();
            }
            if let Ok(v) = r {
                self.fast.fetch_add(1, Relaxed);
                return Ok(v);
            }
        }
        self.fallback.fetch_add(1, Relaxed);
        scene.resolve_in_border_with(bounds, movement, max_step, on_ground, None, &[])
    }

    /// `SceneCollisions::unobstructed(bounds)` (no block collides with the box), the same answer: a collider found means it
    /// collides; otherwise the kernel where the masks do not decide.
    pub fn no_collision(&self, mut scene: SceneCollisions<'_, '_>, bounds: Aabb) -> bool {
        {
            let mut g = self.inner.lock().unwrap();
            let Inner { map, recent, colliders, held, table } = &mut *g;
            colliders.clear();
            // `unobstructed`: the position context, whatever the body.
            let done = collect(map, recent, table, held, &self.built, &mut scene, (bounds.min_position().y, false), bounds, colliders).is_ok();
            let free = colliders.is_empty();
            colliders.clear();
            if !held.is_empty() {
                held.clear();
            }
            if done {
                self.fast.fetch_add(1, Relaxed);
                return free;
            }
        }
        self.fallback.fetch_add(1, Relaxed);
        scene.unobstructed(bounds).unwrap_or(false)
    }
}

/// `SceneCollisions::context`: the position context of a body with no subject, the living entity's otherwise.
fn context_of<'w>(scene: &SceneCollisions<'w, '_>, y: f64) -> CollisionContext<'w> {
    let Some(subject) = scene.subject else { return CollisionContext::position(y) };
    CollisionContext::for_entity(
        CollisionEntity { identity: EntityIdentity(0), kind: EntityContextKind::Living, y, descending: false, main_hand: Some(subject.main_hand.unwrap_or(CollisionItem::EMPTY)) },
        false,
    )
}

/// Bit of a neighbour's offset in `Section::halo`.
const fn rel(d: [i32; 3]) -> u32 {
    ((d[0] + 1) * 9 + (d[1] + 1) * 3 + d[2] + 1) as u32
}

/// The section `k` of the map, built from the blocks when it is not there, and the halos kept: a section that holds a shape reaching out of its
/// cell (or a moving piston) is noted in the 26 neighbours' `halo` (and `flags` bit 16 when any is), and a new one takes its own from the
/// neighbours it finds.
fn get_or_build<'m>(map: &'m mut Map, table: &mut Table, world: &World, binding: &Binding<'_>, k: [i32; 3]) -> &'m mut Box<Section> {
    let kk = key(k);
    if !map.contains_key(&kk) {
        let s = build(world, binding, table, k);
        let mut halo = 0u32;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if (dx, dy, dz) != (0, 0, 0) {
                        if let Some(n) = map.get(&key([k[0] + dx, k[1] + dy, k[2] + dz])) {
                            if n.flags.load(Relaxed) & 4 != 0 {
                                halo |= 1 << rel([dx, dy, dz]);
                            }
                        }
                    }
                }
            }
        }
        s.halo.store(halo, Relaxed);
        if halo != 0 {
            s.flags.fetch_or(16, Relaxed);
        }
        let reaches = s.flags.load(Relaxed) & 4 != 0;
        map.insert(kk, s);
        if reaches {
            mark_halo(map, k);
        }
    }
    map.get_mut(&kk).expect("just inserted")
}
/// The section at `k` now holds a shape that reaches out of its cell: its 26 neighbours that exist note it.
fn mark_halo(map: &Map, k: [i32; 3]) {
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if (dx, dy, dz) != (0, 0, 0) {
                    if let Some(n) = map.get(&key([k[0] + dx, k[1] + dy, k[2] + dz])) {
                        n.halo.fetch_or(1 << rel([-dx, -dy, -dz]), Relaxed);
                        n.flags.fetch_or(16, Relaxed);
                    }
                }
            }
        }
    }
}

fn build(world: &World, binding: &Binding<'_>, table: &mut Table, k: [i32; 3]) -> Box<Section> {
    let mut s = Box::new(Section::void());
    let Some(chunk) = world.chunks.get(&(k[0], k[2])) else { return s };
    let (c0, c1) = ([k[0] << 4, k[1] << 4, k[2] << 4], [(k[0] << 4) + 15, (k[1] << 4) + 15, (k[2] << 4) + 15]);
    if !world.may_hold(c0, c1, |s| binding.may_collide(s) || binding.reaches_out(s)) {
        return s;
    }
    for y in 0..16 {
        for z in 0..16 {
            let row = ((y << 4) | z) as usize;
            for x in 0..16 {
                let class = table.class_of(binding, chunk.block(c0[0] + x, c0[1] + y, c0[2] + z));
                if class != Class::Empty {
                    s.set(row, 1 << x, class);
                }
            }
        }
    }
    s
}

/// The sweep's `clearly_overlaps`: some box of the moved shape overlaps `bounds` by more than 1e-5 on every axis.
fn clearly_overlaps(shape: &VoxelShape, offset: Vec3, bounds: Aabb) -> bool {
    const MARGIN: f64 = 1.0e-5;
    shape.boxes().iter().any(|b| {
        let b = b.move_by(offset);
        Axis::ALL.into_iter().all(|a| b.max(a).min(bounds.max(a)) - b.min(a).max(bounds.min(a)) > MARGIN)
    })
}

/// The colliders of the scan of `b`, in the scan's order, or `Err` when the masks do not decide.
#[allow(clippy::too_many_arguments)]
fn collect(map: &mut Map, recent: &mut [(u64, usize); 64], table: &mut Table, held: &mut Vec<VoxelShape>, built: &AtomicU64, scene: &mut SceneCollisions<'_, '_>, context: (f64, bool), b: Aabb, out: &mut Vec<Occ>) -> Result<(), ()> {
    let (world, binding, subject) = (scene.world, scene.binding, scene.subject.is_some());
    let (min, max) = (b.min_position(), b.max_position());
    if !min.is_finite() || !max.is_finite() {
        return Err(());
    }
    let e = 1.0e-7;
    // `floor` without the library call: the truncation, one less for a negative fraction.
    let fl = |v: f64| {
        let t = v as i64 as f64;
        if t > v { t - 1.0 } else { t }
    };
    let (fx0, fy0, fz0, fx1, fy1, fz1) = (fl(min.x - e), fl(min.y - e), fl(min.z - e), fl(max.x + e), fl(max.y + e), fl(max.z + e));
    // Beyond a few cells the kernel's own errors and budget rules apply: not decided here.
    if fx1 - fx0 > 14.0 || fy1 - fy0 > 14.0 || fz1 - fz0 > 14.0 || fx0.abs() > 2.0e9 || fz0.abs() > 2.0e9 || fy0.abs() > 2.0e9 {
        return Err(());
    }
    let (x0, y0, z0, x1, y1, z1) = (fx0 as i32, fy0 as i32, fz0 as i32, fx1 as i32, fy1 as i32, fz1 as i32);
    // The sections of the padded box (the cells the scan walks: the box's, and one more on every side).
    let (kx0, kx1, ky0, ky1, kz0, kz1) = ((x0 - 1) >> 4, (x1 + 1) >> 4, (y0 - 1) >> 4, (y1 + 1) >> 4, (z0 - 1) >> 4, (z1 + 1) >> 4);
    let (ny, nz) = ((ky1 - ky0 + 1) as usize, (kz1 - kz0 + 1) as usize);
    // The boxes of the map's sections keep their address while the lock is held (nothing is removed during a call).
    let mut secs: [*const Section; 27] = [&VOID; 27];
    let (mut any_cells, mut any_hard, mut any_large, mut any_piston) = (false, false, false, false);
    for (i, kx) in (kx0..=kx1).enumerate() {
        for (j, ky) in (ky0..=ky1).enumerate() {
            for (l, kz) in (kz0..=kz1).enumerate() {
                let k = key([kx, ky, kz]);
                let slot = (k.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 58) as usize;
                let s: &Section = if recent[slot].1 != 0 && recent[slot].0 == k {
                    // SAFETY: the address of a box of the map, which keeps it until `clear` (that clears `recent`).
                    unsafe { &*(recent[slot].1 as *const Section) }
                } else {
                    let s: &Section = match map.get(&k) {
                        Some(s) => s,
                        None => {
                            built.fetch_add(1, Relaxed);
                            get_or_build(map, table, world, binding, [kx, ky, kz])
                        }
                    };
                    recent[slot] = (k, s as *const Section as usize);
                    s
                };
                any_cells |= s.any_cells;
                any_hard |= s.any_hard || (subject && s.any_liquid);
                any_large |= s.any_large;
                any_piston |= s.any_piston;
                secs[(i * ny + j) * nz + l] = s;
            }
        }
    }
    // SAFETY: see above; the pointers come from the map just read.
    let at = |kx: i32, ky: i32, kz: i32| unsafe { &*secs[(((kx - kx0) as usize * ny) + (ky - ky0) as usize) * nz + (kz - kz0) as usize] };
    // A word of the bits of x in [lo, hi] (block coordinates) within the section `kx`.
    let span = |kx: i32, lo: i32, hi: i32| -> u16 {
        let (a, c) = ((lo.max(kx << 4) - (kx << 4)) as i32, (hi.min((kx << 4) + 15) - (kx << 4)) as i32);
        if a > c { 0 } else { (((1u32 << (c - a + 1)) - 1) << a) as u16 }
    };
    if !any_cells {
        return Ok(());
    }
    // The masks of x per section of the row, once.
    // Without a shape that reaches out of its cell nearby, only the box's own cells are read (the ring is not).
    let pad = i32::from(any_large || any_piston);
    let mut inner_m = [0u16; 3];
    let mut edge_m = [0u16; 3];
    for (i, kx) in (kx0..=kx1).enumerate() {
        inner_m[i] = span(kx, x0, x1);
        edge_m[i] = if pad == 1 { span(kx, x0 - 1, x0 - 1) | span(kx, x1 + 1, x1 + 1) } else { 0 };
    }
    // What the scan does with the cells of one row's word of bits (in x order): the same for the lean and the general walk.
    macro_rules! emit {
        ($s:expr, $row:expr, $kx:expr, $y:expr, $z:expr, $bits:expr) => {{
            let (s, row, kx, y, z): (&Section, usize, i32, i32, i32) = ($s, $row, $kx, $y, $z);
            let mut bits: u16 = $bits;
            while bits != 0 {
                let bit = bits.trailing_zeros() as i32;
                bits &= bits - 1;
                let x = (kx << 4) + bit;
                let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                if s.full[row] >> bit & 1 != 0 {
                    let top = Vec3::new(offset.x + 1.0, offset.y + 1.0, offset.z + 1.0);
                    if b.intersects_corners(offset, top) {
                        out.push(Occ::Cube(offset));
                    }
                    continue;
                }
                if (s.hard[row] | s.liquid[row]) >> bit & 1 != 0 {
                    // A cell whose shape the kernel gives, with the entity's context, as it does in its scan.
                    if s.liquid[row] >> bit & 1 != 0 && !subject {
                        continue;
                    }
                    ask(scene, context, x, y, z, b, out, held)?;
                    continue;
                }
                let shape_ptr = match table.class_of(binding, world.block(x, y, z)) {
                    Class::Shaped { shape, .. } => table.shapes[shape as usize],
                    Class::Column { slot } => column_shape(table, scene, slot, x, y, z)?,
                    _ => return Err(()),
                };
                // SAFETY: as `Occ::shape`: the registry's cache or `column_shapes` (boxed, never freed or moved).
                let shape: &VoxelShape = unsafe { &*(shape_ptr as *const VoxelShape) };
                // `examine`: the shape's bounds, moved, must meet the box; and it must overlap it clearly (else the kernel joins).
                if !shape.bounds().is_some_and(|sb| sb.move_by(offset).intersects(b)) {
                    continue;
                }
                if !clearly_overlaps(shape, offset, b) {
                    graze(shape, offset, b, out, held)?;
                    continue;
                }
                out.push(Occ::Shape(shape as *const VoxelShape as usize, offset));
            }
        }};
    }
    if pad == 0 && !any_hard {
        // Plain cubes and fixed shapes only, nothing reaching out: the box's own cells.
        for z in z0..=z1 {
            for y in y0..=y1 {
                let row = (((y & 15) << 4) | (z & 15)) as usize;
                for (i, kx) in (kx0..=kx1).enumerate() {
                    let s = at(kx, y >> 4, z >> 4);
                    emit!(s, row, kx, y, z, s.near[row] & inner_m[i]);
                }
            }
        }
        return Ok(());
    }
    for z in z0 - pad..=z1 + pad {
        let zouter = pad == 1 && (z == z0 - 1 || z == z1 + 1);
        for y in y0 - pad..=y1 + pad {
            // The scan's kinds: how many of the cell's coordinates lie on the box's outer layer. Kind 1 counts only a state
            // with a large shape, kind 2 only a moving piston, kind 3 never.
            let base = usize::from(zouter) + usize::from(pad == 1 && (y == y0 - 1 || y == y1 + 1));
            if base == 2 && !any_piston {
                continue;
            }
            let row = (((y & 15) << 4) | (z & 15)) as usize;
            for (i, kx) in (kx0..=kx1).enumerate() {
                let s = at(kx, y >> 4, z >> 4);
                let bits = if base == 0 {
                    ((s.near[row] | if subject { s.liquid[row] } else { 0 }) & inner_m[i]) | (s.large[row] & edge_m[i])
                } else if base == 1 {
                    (s.large[row] & inner_m[i]) | (s.piston[row] & edge_m[i])
                } else {
                    s.piston[row] & inner_m[i]
                };
                emit!(s, row, kx, y, z, bits);
            }
        }
    }
    Ok(())
}

/// The offset key of a column: the two 4-bit values `DynamicSupport::offset` takes from `position_seed(x, 0, z)` (shifts 0 and 8);
/// bamboo and the stalactites are moved by a pure function of them.
#[inline]
fn column_key(x: i32, z: i32) -> usize {
    let seed = position_seed(BlockPos::new(x, 0, z));
    ((seed & 15) | (seed >> 8 & 15) << 4) as usize
}

/// The kernel's moved shape of a column-offset state at the offset of column (x, z): asked once per (state, offset), then kept.
#[inline]
fn column_shape(table: &mut Table, scene: &mut SceneCollisions<'_, '_>, slot: u32, x: i32, y: i32, z: i32) -> Result<usize, ()> {
    let key = column_key(x, z);
    if let Some(Some(a)) = table.columns.get(slot as usize) {
        if a[key] != 0 {
            return Ok(a[key]);
        }
    }
    column_shape_first(table, scene, slot, key, x, y, z)
}
#[cold]
#[inline(never)]
fn column_shape_first(table: &mut Table, scene: &mut SceneCollisions<'_, '_>, slot: u32, key: usize, x: i32, y: i32, z: i32) -> Result<usize, ()> {
    let (world, binding) = (scene.world, scene.binding);
    let state = world.block(x, y, z);
    // The shape depends on the column alone (no contextual rule, no block entity), so any context and this cell's own answer will do.
    let shape = binding.collision_shape_with_context(scene, state, BlockPos::new(x, y, z), &CollisionContext::position(f64::from(y))).map_err(|_| ())?;
    table.column_shapes.push(Box::new(shape));
    let p = table.column_shapes.last().map(|b| &**b as *const VoxelShape as usize).ok_or(())?;
    let a = table.columns.get_mut(slot as usize).ok_or(())?.get_or_insert_with(|| Box::new([0; 256]));
    a[key] = p;
    Ok(p)
}

/// A shape whose bounds meet the box without overlapping it clearly: the kernel moves it and keeps it only if its join with the
/// box (`Shapes.joinIsNotEmpty(AND)`) is not empty, as the moved shape itself.
#[inline(never)]
fn graze(shape: &VoxelShape, offset: Vec3, b: Aabb, out: &mut Vec<Occ>, held: &mut Vec<VoxelShape>) -> Result<(), ()> {
    if held.len() == held.capacity() {
        return Err(());
    }
    let moved = shape.moved(offset);
    let entity = VoxelShape::from_box(b).map_err(|_| ())?;
    if moved.is_empty() || !moved.join_is_not_empty(&entity, ShapeOp::AND).map_err(|_| ())? {
        return Ok(());
    }
    held.push(moved);
    out.push(Occ::Joined(held.last().expect("just pushed") as *const VoxelShape as usize));
    Ok(())
}

/// The collider of one hard cell, as the kernel's `examine` makes it: the shape the kernel gives for this block with the entity's
/// context (the world is asked for what it must: block entity, moving pistons), a unit cube as its offset, a shape as itself.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn ask(scene: &mut SceneCollisions<'_, '_>, (y0, entity_context): (f64, bool), x: i32, y: i32, z: i32, b: Aabb, out: &mut Vec<Occ>, held: &mut Vec<VoxelShape>) -> Result<(), ()> {
    let (world, binding) = (scene.world, scene.binding);
    let state = world.block(x, y, z);
    if binding.collision_block(state).map_err(|_| ())?.empty_shape {
        return Ok(());
    }
    let ctx = if entity_context { context_of(scene, y0) } else { CollisionContext::position(y0) };
    let shape = binding.collision_shape_with_context(scene, state, BlockPos::new(x, y, z), &ctx).map_err(|_| ())?;
    let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
    if shape.is_block() {
        let top = Vec3::new(offset.x + 1.0, offset.y + 1.0, offset.z + 1.0);
        if b.intersects_corners(offset, top) {
            out.push(Occ::Cube(offset));
        }
        return Ok(());
    }
    if !shape.bounds().is_some_and(|sb| sb.move_by(offset).intersects(b)) {
        return Ok(());
    }
    if held.len() == held.capacity() {
        return Err(());
    }
    if !clearly_overlaps(&shape, offset, b) {
        return graze(&shape, offset, b, out, held);
    }
    held.push(shape);
    out.push(Occ::Shape(held.last().expect("just pushed") as *const VoxelShape as usize, offset));
    Ok(())
}
