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
//! `collide_moved` for a fixed shape (the kernel's `BlockCollider::Moved`, without cloning the shape). As soon as a hard cell lies in
//! the box or its ring, a shape overlaps the box only by a hair (the kernel then joins shapes), or the box is too big or not finite,
//! nothing is decided here: the kernel runs unchanged.
use horizon_client_render::collisions::SceneCollisions;
use horizon_core::geometry::{try_resolve_step_movement_in, Aabb, Axis, Collider, Vec3, VoxelShape};
use horizon_world::registry::BlockStateId;
use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
    sync::{
        atomic::{AtomicU64, Ordering::Relaxed},
        Mutex,
    },
};

type Binding<'a> = horizon_world::block_collision::BlockCollisionRegistry<'a>;
type World = horizon_client_render::world::World;

pub struct Section {
    full: [u16; 256],
    shaped: [u16; 256],
    large: [u16; 256],
    hard: [u16; 256],
    /// Water and lava: empty for a body with no subject (`CollisionContext::position`), the kernel's business for one that stands on fluids.
    liquid: [u16; 256],
    any_cells: bool,
    any_hard: bool,
    any_large: bool,
}
impl Section {
    fn void() -> Self {
        Section { full: [0; 256], shaped: [0; 256], large: [0; 256], hard: [0; 256], liquid: [0; 256], any_cells: false, any_hard: false, any_large: false }
    }
    fn set(&mut self, row: usize, bit: u16, class: Class) {
        let clear = !bit;
        self.full[row] &= clear;
        self.shaped[row] &= clear;
        self.large[row] &= clear;
        self.hard[row] &= clear;
        self.liquid[row] &= clear;
        match class {
            Class::Empty => {}
            Class::Cube => {
                self.full[row] |= bit;
                self.any_cells = true;
            }
            Class::Shaped { large, .. } => {
                self.shaped[row] |= bit;
                if large {
                    self.large[row] |= bit;
                    self.any_large = true;
                }
                self.any_cells = true;
            }
            Class::Hard => {
                self.hard[row] |= bit;
                self.any_hard = true;
            }
            Class::Liquid => {
                self.liquid[row] |= bit;
                self.any_hard = true;
            }
        }
    }
}
static VOID: Section = Section { full: [0; 256], shaped: [0; 256], large: [0; 256], hard: [0; 256], liquid: [0; 256], any_cells: false, any_hard: false, any_large: false };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Empty,
    Cube,
    /// A fixed shape (index in the table, from 0), large when it reaches out of its cell.
    Shaped { shape: u32, large: bool },
    Hard,
    /// Water or lava: no collision for a body with no subject.
    Liquid,
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
}
impl Occ {
    #[inline]
    fn shape(&self) -> &VoxelShape {
        match self {
            // SAFETY: the address of a shape of the registry's collision cache, which the world keeps for its whole life.
            Occ::Shape(p, _) => unsafe { &*(*p as *const VoxelShape) },
            Occ::Cube(_) => unreachable!(),
        }
    }
}
impl Collider for Occ {
    #[inline]
    fn collide(&self, axis: Axis, aabb: Aabb, amount: f64) -> f64 {
        match self {
            Occ::Cube(offset) => VoxelShape::collide_unit_cube(*offset, axis, aabb, amount),
            Occ::Shape(_, offset) => self.shape().collide_moved(*offset, axis, aabb, amount),
        }
    }
    #[inline]
    fn y_len(&self) -> usize {
        match self {
            Occ::Cube(_) => 2,
            Occ::Shape(..) => self.shape().coordinates(Axis::Y).len(),
        }
    }
    #[inline]
    fn y_at(&self, k: usize) -> f64 {
        match self {
            Occ::Cube(offset) => k as f64 + offset.y,
            Occ::Shape(_, offset) => self.shape().coordinates(Axis::Y)[k] + offset.y,
        }
    }
}

/// The classes of the block states, filled on first sight, and the fixed shapes.
#[derive(Default)]
struct Table {
    /// Per state: 0 unknown, 1 empty, 2 cube, 3 hard, 4 liquid, 8 + 2 * shape (+ 1 when large).
    class: Vec<u32>,
    /// The distinct fixed shapes, by address in the registry's own cache (nothing is copied; the registry outlives the masks).
    shapes: Vec<usize>,
    shape_at: HashMap<usize, u32>,
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
            let (k, c) = self.classify(binding, s);
            self.class[i] = k;
            self.met += 1;
            return c;
        }
        match c {
            1 => Class::Empty,
            2 => Class::Cube,
            3 => Class::Hard,
            4 => Class::Liquid,
            n => Class::Shaped { shape: (n - 8) / 2, large: (n - 8) % 2 == 1 },
        }
    }
    fn classify(&mut self, binding: &Binding<'_>, s: BlockStateId) -> (u32, Class) {
        let Ok(block) = binding.collision_block(s) else { return (3, Class::Hard) };
        if !binding.may_collide(s) && !binding.reaches_out(s) {
            return (1, Class::Empty);
        }
        if binding.is_plain_cube(s) {
            return (2, Class::Cube);
        }
        // The shape of such a state is its cached one, whatever the entity: no contextual rule and no variants (flag 8), no
        // block entity or world to ask (a cached shape), no piston in motion.
        if binding.has_context_rule(s) {
            // `LiquidCollision`: water and lava (no variants) are empty unless the body stands on fluids or collides with all of them.
            let liquid = matches!(binding.registry().state(s).map(|st| st.name.as_str()), Some("minecraft:water" | "minecraft:lava")) && binding.cached_shapes().collision_variants(s).next().is_none();
            return if liquid { (4, Class::Liquid) } else { (3, Class::Hard) };
        }
        if block.empty_shape || block.moving_piston {
            return (3, Class::Hard);
        }
        let Ok(Some(shape)) = binding.cached_shapes().cached_shape(s) else { return (3, Class::Hard) };
        if shape.is_empty() || shape.is_block() || shape.bounds().is_none() {
            return (3, Class::Hard);
        }
        let addr = shape as *const VoxelShape as usize;
        let n = *self.shape_at.entry(addr).or_insert_with(|| {
            self.shapes.push(addr);
            self.shapes.len() as u32 - 1
        });
        let large = block.large_collision_shape;
        (8 + 2 * n + u32::from(large), Class::Shaped { shape: n, large })
    }
}

struct Inner {
    map: Map,
    /// The last sections looked up (direct-mapped by key): no hashing for the sections a walking body keeps asking for.
    recent: [(u64, usize); 64],
    colliders: Vec<Occ>,
    table: Table,
}
impl Default for Inner {
    fn default() -> Self {
        Inner { map: Map::default(), recent: [(0, 0); 64], colliders: Vec::new(), table: Table::default() }
    }
}
/// The masks and the table of one world, and how often the fast path answered.
#[derive(Default)]
pub struct Occupancy {
    inner: Mutex<Inner>,
    pub fast: AtomicU64,
    pub fallback: AtomicU64,
    pub built: AtomicU64,
}

impl Occupancy {
    /// The block at (x, y, z) has just been written: its cell takes the class of its new state (the bits of its row), if the
    /// section has a mask. Nothing else is touched. Whoever writes blocks calls this (and after replacing a chunk, `clear`).
    pub fn changed(&self, world: &World, binding: &Binding<'_>, x: i32, y: i32, z: i32) {
        let mut g = self.inner.lock().unwrap();
        let Inner { map, table, .. } = &mut *g;
        let Some(section) = map.get_mut(&key([x >> 4, y >> 4, z >> 4])) else { return };
        let class = table.class_of(binding, world.block(x, y, z));
        section.set((((y & 15) << 4) | (z & 15)) as usize, 1 << (x & 15), class);
    }
    /// Drops every mask (a chunk replaced, a world reloaded); the table stays.
    pub fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.map.clear();
        g.recent = [(0, 0); 64];
    }
    /// The class of a state, by name, for the tests: "empty", "cube", "shaped", "large shaped" or "hard".
    pub fn class_name(&self, binding: &Binding<'_>, s: BlockStateId) -> &'static str {
        match self.inner.lock().unwrap().table.class_of(binding, s) {
            Class::Empty => "empty",
            Class::Cube => "cube",
            Class::Shaped { large: false, .. } => "shaped",
            Class::Shaped { large: true, .. } => "large shaped",
            Class::Hard => "hard",
            Class::Liquid => "liquid",
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
    pub fn resolve(&self, scene: SceneCollisions<'_, '_>, bounds: Aabb, movement: Vec3, max_step: f32, on_ground: bool) -> Result<Vec3, Box<dyn std::error::Error>> {
        {
            let mut g = self.inner.lock().unwrap();
            let Inner { map, recent, colliders, table } = &mut *g;
            colliders.clear();
            let (world, binding, subject) = (scene.world, scene.binding, scene.subject.is_some());
            let r = try_resolve_step_movement_in(movement, bounds, max_step, on_ground, colliders, |b, out| {
                collect(map, recent, table, &self.built, world, binding, subject, b, out)
            });
            colliders.clear();
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
    pub fn no_collision(&self, scene: SceneCollisions<'_, '_>, bounds: Aabb) -> bool {
        {
            let mut g = self.inner.lock().unwrap();
            let Inner { map, recent, colliders, table } = &mut *g;
            colliders.clear();
            if collect(map, recent, table, &self.built, scene.world, scene.binding, scene.subject.is_some(), bounds, colliders).is_ok() {
                let free = colliders.is_empty();
                colliders.clear();
                self.fast.fetch_add(1, Relaxed);
                return free;
            }
        }
        self.fallback.fetch_add(1, Relaxed);
        let mut scene = scene;
        scene.unobstructed(bounds).unwrap_or(false)
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
fn collect(map: &mut Map, recent: &mut [(u64, usize); 64], table: &mut Table, built: &AtomicU64, world: &World, binding: &Binding<'_>, subject: bool, b: Aabb, out: &mut Vec<Occ>) -> Result<(), ()> {
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
    let (mut any_cells, mut any_hard, mut any_large) = (false, false, false);
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
                            map.entry(k).or_insert_with(|| build(world, binding, table, [kx, ky, kz]))
                        }
                    };
                    recent[slot] = (k, s as *const Section as usize);
                    s
                };
                any_cells |= s.any_cells;
                any_hard |= s.any_hard;
                any_large |= s.any_large;
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
    if any_hard {
        for z in z0 - 1..=z1 + 1 {
            for y in y0 - 1..=y1 + 1 {
                let row = (((y & 15) << 4) | (z & 15)) as usize;
                for kx in kx0..=kx1 {
                    let s = at(kx, y >> 4, z >> 4);
                    if (s.hard[row] | if subject { s.liquid[row] } else { 0 }) & span(kx, x0 - 1, x1 + 1) != 0 {
                        return Err(());
                    }
                }
            }
        }
    }
    if !any_cells {
        return Ok(());
    }
    // The masks of x per section of the row, once.
    let mut inner_m = [0u16; 3];
    let mut edge_m = [0u16; 3];
    for (i, kx) in (kx0..=kx1).enumerate() {
        inner_m[i] = span(kx, x0, x1);
        edge_m[i] = if any_large { span(kx, x0 - 1, x0 - 1) | span(kx, x1 + 1, x1 + 1) } else { 0 };
    }
    // Without a shape that reaches out of its cell nearby, only the box's own cells are read (the ring is not).
    let pad = i32::from(any_large);
    for z in z0 - pad..=z1 + pad {
        let zouter = z == z0 - 1 || z == z1 + 1;
        for y in y0 - pad..=y1 + pad {
            // The scan's kinds: a cell on one outer layer is read only for a shape that reaches out of its cell, on two never.
            let base = if any_large { usize::from(zouter) + usize::from(y == y0 - 1 || y == y1 + 1) } else { 0 };
            if base == 2 {
                continue;
            }
            let row = (((y & 15) << 4) | (z & 15)) as usize;
            for (i, kx) in (kx0..=kx1).enumerate() {
                let s = at(kx, y >> 4, z >> 4);
                let mut bits = if base == 0 { ((s.full[row] | s.shaped[row]) & inner_m[i]) | (s.large[row] & edge_m[i]) } else { s.large[row] & inner_m[i] };
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
                    let Class::Shaped { shape, .. } = table.class_of(binding, world.block(x, y, z)) else { return Err(()) };
                    // SAFETY: as `Occ::shape`.
                    let shape: &VoxelShape = unsafe { &*(table.shapes[shape as usize] as *const VoxelShape) };
                    // `examine`: the shape's bounds, moved, must meet the box; and it must overlap it clearly (else the kernel joins).
                    if !shape.bounds().is_some_and(|sb| sb.move_by(offset).intersects(b)) {
                        continue;
                    }
                    if !clearly_overlaps(shape, offset, b) {
                        return Err(());
                    }
                    out.push(Occ::Shape(shape as *const VoxelShape as usize, offset));
                }
            }
        }
    }
    Ok(())
}
