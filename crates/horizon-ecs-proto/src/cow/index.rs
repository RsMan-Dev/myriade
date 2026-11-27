//! The spatial index of the herd and its watchers.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use myriade_engine_lib::mobs::{
    MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use myriade_level::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

/// The widening of a slot's extent (blocks): covers the `f32` rounding of the relative extent.
pub(crate) const SLOT_MARGIN: f64 = 1e-3;

/// The spatial index: the entities by column of 1 block, kept per loaded chunk (`chunks.rs`), a resource. It is changed only when a
/// box changes column (`reindex`, after the move): a cow that does not move costs it nothing. An entity in a chunk that is not loaded
/// is not indexed (`Cell` holds `u32::MAX`).
#[derive(Resource, Default)]
pub struct Index {
    pub(crate) chunks: Chunks,
    /// The entities whose cell changed since the last `player_near` (`reindex` writes them).
    pub(crate) crossed: Vec<(Entity, u32)>,
    /// The simulated chunks (`zone.rs`): on in the constructors of a server (`Herd::new`, `with_slab`...), off only in their `_awake` variants (benches and tests that want every creature ticked).
    pub zone: crate::zone::Zone,
    /// The watched columns of each consumer of `CellMoves` that looks at the NEIGHBOURS of a mover (see `Watch`).
    pub(crate) watch: [Watch; WATCHERS],
    /// By entity index: the consumers (bit `WATCH_*`) the entity is a watcher of.
    pub(crate) watching: Vec<u8>,
    /// The entities that came into the index while their chunk had no record (`enter_index`): `adopt_unplaced` lists them when it is made.
    pub(crate) unplaced: Vec<Entity>,
}

/// The consumers of the moves of columns that wake the watchers around a mover. A consumer registers its watchers (an entity
/// carrying its component, see `watch_hooks!`); a mover that enters a column far from every watcher of the consumer costs it nothing.
pub const WATCH_FOLLOW_PARENT: usize = 0;

pub const WATCH_AVOID: usize = 1;

pub const WATCH_FOLLOW_MOB: usize = 2;

pub const WATCH_PUFFER: usize = 3;

pub(crate) const WATCHERS: usize = 4;

/// The watchers of one consumer, counted by bucket of 2 x 2 chunks (32 x 32 columns, a watcher within 32 columns of a mover is in the
/// 3 x 3 buckets around its column); only the occupied buckets are listed. `total` is zero while nobody watches (the whole cost then).
#[derive(Default)]
pub(crate) struct Watch {
    pub(crate) total: u32,
    pub(crate) counts: horizon_core::fast_hash::FxHashMap<(i32, i32), u32>,
}

impl Index {
    /// An index with no chunk: they come with `chunk_created`.
    pub fn new() -> Self {
        Self::default()
    }
    /// An index with the chunks that cover the square of blocks `-half..half` (a bounded slab, for benches and tests).
    pub fn square(half: i32) -> Self {
        let mut idx = Self::default();
        for cz in (-half >> 4)..=((half + 1) >> 4) {
            for cx in (-half >> 4)..=((half + 1) >> 4) {
                idx.chunk_created(cx, cz);
            }
        }
        idx
    }
    /// Chunk `(cx, cz)` is loaded: its record is made (the one it has when it is made already) and its count of players is worked out.
    pub fn chunk_created(&mut self, cx: i32, cz: i32) -> u32 {
        let known = self.chunks.find(cx, cz);
        if known != NO_SLOT {
            return known;
        }
        let slot = self.chunks.create(cx, cz);
        let n = self.zone.covering(cx, cz);
        let r = &mut self.chunks.recs[slot as usize];
        (r.count, r.pre) = (n, n);
        slot
    }
    /// Chunk `(cx, cz)` is unloaded: its record is freed. The entities that were still listed in it are returned, no longer indexed (the
    /// caller puts them to sleep or takes them out: their `Cell` is set to `u32::MAX`).
    pub fn chunk_unloaded(&mut self, cx: i32, cz: i32) -> Vec<Entity> {
        let slot = self.chunks.find(cx, cz);
        if slot == NO_SLOT {
            return Vec::new();
        }
        let left = self.chunks.free(slot);
        let bucket = (cx >> 1, cz >> 1);
        for &e in &left {
            let m = self.watching.get(e.index().index() as usize).copied().unwrap_or(0);
            for k in (0..WATCHERS).filter(|k| m >> k & 1 == 1) {
                self.watch_bucket(k, bucket, -1);
            }
        }
        self.crossed.retain(|&(_, c)| c == u32::MAX || chunk_slot_of(c) != slot as usize);
        self.zone.touched.retain(|&k| k != slot);
        left
    }
    /// Whether chunk `(cx, cz)` is loaded.
    pub fn chunk_loaded(&self, cx: i32, cz: i32) -> bool {
        self.chunks.find(cx, cz) != NO_SLOT
    }
    #[inline]
    fn bucket_of(&self, cell: u32) -> (i32, i32) {
        let r = &self.chunks.recs[chunk_slot_of(cell)];
        (r.cx >> 1, r.cz >> 1)
    }
    fn watch_bucket(&mut self, k: usize, b: (i32, i32), delta: i32) {
        let w = &mut self.watch[k];
        let c = w.counts.entry(b).or_insert(0);
        *c = (*c as i64 + delta as i64) as u32;
        if *c == 0 {
            w.counts.remove(&b);
        }
        w.total = (w.total as i64 + delta as i64) as u32;
    }
    /// `e`, in column `cell`, watches for consumer `k` (adds it to the count of its bucket).
    pub(crate) fn watch_count(&mut self, k: usize, cell: u32, delta: i32) {
        let b = self.bucket_of(cell);
        self.watch_bucket(k, b, delta);
    }
    /// `e` became a watcher of consumer `k` (its component was inserted); `cell` is its column (`u32::MAX`: not placed yet,
    /// `move_watcher` counts it at its first column).
    pub fn watch_add(&mut self, e: Entity, k: usize, cell: u32) {
        let i = e.index().index() as usize;
        if self.watching.len() <= i {
            self.watching.resize(i + 1, 0);
        }
        self.watching[i] |= 1 << k;
        if cell != u32::MAX {
            self.watch_count(k, cell, 1);
        }
    }
    pub fn watch_remove(&mut self, e: Entity, k: usize, cell: u32) {
        if let Some(m) = self.watching.get_mut(e.index().index() as usize) {
            *m &= !(1 << k);
        }
        if cell != u32::MAX {
            self.watch_count(k, cell, -1);
        }
    }
    /// `reindex`: the entity went from column `old` to `new`; its counts follow when it watches and the bucket changed.
    #[inline]
    pub(crate) fn move_watcher(&mut self, e: Entity, old: u32, new: u32) {
        let m = self.watching.get(e.index().index() as usize).copied().unwrap_or(0);
        if m == 0 || (old != u32::MAX && new != u32::MAX && self.bucket_of(old) == self.bucket_of(new)) {
            return;
        }
        for k in 0..WATCHERS {
            if m >> k & 1 == 1 {
                if old != u32::MAX {
                    self.watch_count(k, old, -1);
                }
                if new != u32::MAX {
                    self.watch_count(k, new, 1);
                }
            }
        }
    }
    /// Does consumer `k` have a watcher within 32 columns of column `cell`? (zero cost while it has none.)
    #[inline]
    pub fn watched_near(&self, k: usize, cell: u32) -> bool {
        let w = &self.watch[k];
        if w.total == 0 || cell == u32::MAX {
            return false;
        }
        let (bx, bz) = self.bucket_of(cell);
        (bz - 1..=bz + 1).any(|z| (bx - 1..=bx + 1).any(|x| w.counts.contains_key(&(x, z))))
    }
    /// The cell of block column `(bx, bz)` seen from chunk slot `home` (`NO_SLOT`: no start), `u32::MAX` when its chunk is not loaded.
    #[inline]
    pub(crate) fn cell_in(&self, home: u32, bx: i32, bz: i32) -> u32 {
        let s = self.chunks.rel(home, bx >> 4, bz >> 4);
        if s == NO_SLOT { u32::MAX } else { s << 8 | ((bz & 15) << 4 | (bx & 15)) as u32 }
    }
    /// The cell of the column at `(x, z)`, `u32::MAX` when its chunk is not loaded.
    pub fn cell_of(&self, x: f64, z: f64) -> u32 {
        self.cell_in(NO_SLOT, x.ifloor(), z.ifloor())
    }
    /// `cell_of` for a position near the entity whose cell is `from` (the links of its chunk: no hash for a step into the next chunk).
    #[inline]
    pub fn cell_near(&self, from: u32, x: f64, z: f64) -> u32 {
        self.cell_in(if from == u32::MAX { NO_SLOT } else { (from >> 8) as u32 }, x.ifloor(), z.ifloor())
    }
    /// The slot of the chunk that holds `(x, z)`.
    #[inline]
    fn home(&self, x: f64, z: f64) -> u32 {
        self.chunks.slot_at(x.ifloor() >> 4, z.ifloor() >> 4)
    }
    pub fn insert(&mut self, e: Entity, cell: u32) {
        self.chunks.push(cell, e);
    }
    pub fn remove(&mut self, e: Entity, cell: u32) {
        self.chunks.take(cell, e);
    }
    /// The block column `(x, z)` of a cell.
    #[inline]
    pub(crate) fn corner(&self, cell: u32) -> (f64, f64) {
        let r = &self.chunks.recs[chunk_slot_of(cell)];
        let col = col_of(cell) as i32;
        (f64::from(r.cx * 16 + (col & 15)), f64::from(r.cz * 16 + (col >> 4)))
    }
    /// Records the XZ extent of the box of `e`, which is in `cell` (see `Cols::slots`).
    pub(crate) fn set_box(&mut self, e: Entity, cell: u32, mn: (f64, f64), mx: (f64, f64)) {
        if cell == u32::MAX {
            return;
        }
        let (x0, z0) = self.corner(cell);
        let col = col_of(cell);
        let Some(c) = self.chunks.recs[chunk_slot_of(cell)].cols.as_mut() else { return };
        let Some(i) = c.cells[col].iter().rposition(|&o| o == e) else { return };
        c.slots[col][i] = [(mn.0 - x0 - SLOT_MARGIN) as f32, (mx.0 - x0 + SLOT_MARGIN) as f32, (mn.1 - z0 - SLOT_MARGIN) as f32, (mx.1 - z0 + SLOT_MARGIN) as f32];
    }
    /// Whether the chunk of a cell ticks (always when the zone is off; never for an entity that is not indexed).
    pub fn simulated_cell(&self, cell: u32) -> bool {
        !self.zone.on || (cell != u32::MAX && self.chunks.recs[chunk_slot_of(cell)].count > 0)
    }
    /// Whether the chunk at block coordinates ticks (a chunk that is not loaded does not).
    pub fn simulated_at(&self, x: f64, z: f64) -> bool {
        !self.zone.on || self.chunk_simulated(x.ifloor() >> 4, z.ifloor() >> 4)
    }
    /// Whether chunk `(cx, cz)` ticks.
    pub fn chunk_simulated(&self, cx: i32, cz: i32) -> bool {
        !self.zone.on || self.chunks.recs.get(self.chunks.slot_at(cx, cz) as usize).is_some_and(|r| r.count > 0)
    }
    /// How many chunks tick.
    pub fn simulated_chunks(&self) -> usize {
        self.chunks.recs.iter().filter(|r| r.live && r.count > 0).count()
    }
    /// Calls `g` for each column of row `z` (a block coordinate) between `lo` and `hi` that holds entities, in increasing x, with its x,
    /// its entities and their extents. The chunks are reached from `home` by the links of the neighbours; a chunk that is not loaded is
    /// empty.
    #[inline]
    fn row(&self, home: u32, z: i32, lo: i32, hi: i32, mut g: impl FnMut(i32, &[Entity], &[[f32; 4]])) {
        if lo > hi {
            return;
        }
        let (cz, lz) = (z >> 4, (z & 15) as usize);
        for cx in (lo >> 4)..=(hi >> 4) {
            let s = self.chunks.rel(home, cx, cz);
            if s == NO_SLOT {
                continue;
            }
            let r = &self.chunks.recs[s as usize];
            let Some(c) = r.cols.as_deref() else { continue };
            let mut bits = u32::from(r.rows[lz]);
            if bits == 0 {
                continue;
            }
            let (a, b) = ((lo - cx * 16).max(0), (hi - cx * 16).min(15));
            bits &= (0xffff_u32 << a) & (0xffff_u32 >> (15 - b));
            while bits != 0 {
                let i = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                g(cx * 16 + i as i32, &c.cells[lz * 16 + i], &c.slots[lz * 16 + i]);
            }
        }
    }
    /// Calls `f` for the entities in the columns within `r` blocks of `(x, z)` (a box is at most 2 blocks wide).
    /// The chunks hold a bit per occupied column of each row, so the cost follows the occupied columns, not the area.
    /// The visit order is the plain row-major order of the columns (rows outer, columns inner), as a loop over every column would give.
    pub fn for_radius(&self, x: f64, z: f64, r: f64, mut f: impl FnMut(Entity)) {
        let home = self.home(x, z);
        let (x0, x1) = ((x - r - 1.0).ifloor(), (x + r + 1.0).ifloor());
        for zz in (z - r - 1.0).ifloor()..=(z + r + 1.0).ifloor() {
            self.row(home, zz, x0, x1, |_, cells, _| cells.iter().for_each(|&e| f(e)));
        }
    }
    /// Calls `f` for the entities whose column may lie in the disk of radius `r` around `old` and not in the one around `new` (a
    /// superset: the crescent a player's step sweeps, or a whole disk when `new` is `None`), row by row: the cost follows the length
    /// of the crescent, not the area of the disk.
    pub fn crescent(&self, old: (f64, f64), new: Option<(f64, f64)>, r: f64, mut f: impl FnMut(Entity)) {
        let home = self.home(old.0, old.1);
        let (r, ro) = (r + 1.0, r - 1.0);
        for zz in (old.1 - r).ifloor()..=(old.1 + r).ifloor() {
            let (z0, z1) = (f64::from(zz), f64::from(zz + 1));
            let near = (old.1.clamp(z0, z1) - old.1).abs();
            if near >= r {
                continue;
            }
            let half = (r * r - near * near).sqrt();
            let (mut a, b) = ((old.0 - half).ifloor(), (old.0 + half).ifloor());
            // The columns surely inside the new disk (its chord at the far edge of the row) are left out: two runs.
            let skip = new.and_then(|n| {
                let far = (n.1 - z0).abs().max((n.1 - z1).abs());
                (far < ro).then(|| ((ro * ro - far * far).sqrt(), n.0))
            });
            let run = |lo: i32, hi: i32, f: &mut dyn FnMut(Entity)| self.row(home, zz, lo, hi, |_, cells, _| cells.iter().for_each(|&e| f(e)));
            if let Some((h, nx)) = skip {
                let (s0, s1) = ((nx - h + 1.0).ifloor(), (nx + h - 1.0).ifloor());
                run(a, b.min(s0 - 1), &mut f);
                a = a.max(s1 + 1);
            }
            run(a, b, &mut f);
        }
    }
    /// `for_near` for the entities whose box may touch the XZ extent `mn..mx` (a superset: the caller tests the real boxes).
    pub fn for_near_touching(&self, x: f64, z: f64, mn: (f64, f64), mx: (f64, f64), mut f: impl FnMut(Entity)) {
        let (home, bx, bz) = (self.home(x, z), x.ifloor(), z.ifloor());
        for zz in bz - 1..=bz + 1 {
            self.row(home, zz, bx - 1, bx + 1, |xx, cells, slots| {
                let (x0, z0) = (f64::from(xx), f64::from(zz));
                let (ax, bx, az, bz) = ((mn.0 - x0) as f32, (mx.0 - x0) as f32, (mn.1 - z0) as f32, (mx.1 - z0) as f32);
                for (&e, s) in cells.iter().zip(slots) {
                    if s[1] >= ax && s[0] <= bx && s[3] >= az && s[2] <= bz {
                        f(e);
                    }
                }
            });
        }
    }
    /// Calls `f` for the entities in the columns around `(x, z)` (3 x 3, a box is at most 2 blocks wide).
    pub fn for_near(&self, x: f64, z: f64, mut f: impl FnMut(Entity)) {
        let (home, bx, bz) = (self.home(x, z), x.ifloor(), z.ifloor());
        for zz in bz - 1..=bz + 1 {
            self.row(home, zz, bx - 1, bx + 1, |_, cells, _| cells.iter().for_each(|&e| f(e)));
        }
    }
    /// The entities listed in column `cell`.
    #[inline]
    pub(crate) fn list(&self, cell: u32) -> &[Entity] {
        self.chunks.list(cell)
    }
}


/// A new animal is in the spatial index from its creation (not after its first tick: the pushes and the searches of that
/// tick see it).
pub fn enter_index(world: &mut World, e: Entity) {
    let Some(b) = world.get::<Bounds>(e).copied() else { return };
    let (mn, mx) = (b.0.min_position(), b.0.max_position());
    let old = world.get::<Cell>(e).unwrap().0;
    let cell = world.resource::<Index>().cell_near(old, (mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
    {
        let mut idx = world.resource_mut::<Index>();
        // No record for its chunk yet (generation, a summon beside a chunk still loading): it is listed when the record is made.
        if cell == u32::MAX && !idx.unplaced.contains(&e) {
            idx.unplaced.push(e);
        }
        if old != u32::MAX {
            idx.remove(e, old);
        }
        if cell != u32::MAX {
            idx.insert(e, cell);
        }
        idx.move_watcher(e, old, cell);
        // An entity that comes into a column is looked at as one that crossed into it (`player_near`, the zone): born beside a
        // player that stands still, it is active at once.
        idx.crossed.push((e, cell));
    }
    world.get_mut::<Cell>(e).unwrap().0 = cell;
    world.resource_mut::<crate::goals::CellMoves>().0.push(e);
}

/// The index follows the boxes that changed (the cows that moved): only a change of column touches it; a change of
/// the hazards under the box adds or takes off the `Hazard` module.
#[allow(clippy::type_complexity)]
pub(crate) fn reindex(
    mut idx: ResMut<Index>,
    mut moves: ResMut<crate::goals::CellMoves>,
    hazards: Res<Hazards>,
    mut events: ResMut<crate::goals::Events>,
    mut q: Query<(Entity, &Bounds, &mut Cell, Option<&Hazard>, Has<InFluid>, Has<super::fluids::FluidNew>, &mut FluidState, Has<crate::bridge::ids::NetId>), Changed<Bounds>>,
    // A followed entity that moved is sent again (the tracker reads `Modified`); absent when no player ever followed one.
    mut sent: Option<ResMut<crate::bridge::view::Modified>>,
    // Read only when the entity enters or leaves a fluid (rare): not part of the query that runs over every moved entity.
    all_modes: Query<&crate::locomotion::Modes>,
    mut commands: Commands,
) {
    for (e, b, mut c, hz, in_fluid, fresh, mut fs, followed) in &mut q {
        if followed && let Some(m) = sent.as_mut() {
            m.push_motion(e);
        }
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let cell = idx.cell_near(c.0, (mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        if cell != c.0 {
            if c.0 != u32::MAX {
                idx.remove(e, c.0);
            }
            if cell != u32::MAX {
                idx.insert(e, cell);
            }
            idx.move_watcher(e, c.0, cell);
            c.0 = cell;
            moves.0.push(e);
            idx.crossed.push((e, cell));
        }
        idx.set_box(e, cell, (mn.x, mn.z), (mx.x, mx.z));
        let bits = hazards.of_box_kept(&idx.chunks, &b.0, c.bypass_change_detection(), hz.map_or(0, |h| h.0));
        let fluid = bits & (HAZ_WATER | HAZ_LAVA) != 0;
        if fresh {
            commands.entity(e).remove::<super::fluids::FluidNew>();
        }
        if fluid && (!in_fluid || fresh) {
            // A cow that enters a fluid (placed water, a push) needs its physics at once.
            commands.entity(e).insert((InFluid, Moving));
            events.entered_fluid.push(e);
            // The mode of the water, if the entity has one, becomes the active one: an event, not a test each tick.
            if let Ok(m) = all_modes.get(e) {
                crate::locomotion::fluid_changed(&mut commands, e, m, bits & HAZ_WATER != 0, bits & HAZ_LAVA != 0);
            }
        } else if !fluid && in_fluid {
            commands.entity(e).remove::<InFluid>();
            events.left_fluid.push(e);
            if let Ok(m) = all_modes.get(e) {
                crate::locomotion::fluid_changed(&mut commands, e, m, false, false);
            }
            *fs = FluidState::default();
        }
        let was = hz.is_some_and(|h| h.0 & HAZ_SNOW != 0);
        let is = bits & HAZ_SNOW != 0;
        if is && !was {
            events.entered_powder_snow.push(e);
        } else if was && !is {
            events.left_powder_snow.push(e);
        }
        match (bits, hz) {
            (0, Some(_)) => {
                commands.entity(e).remove::<Hazard>();
            }
            (0, None) => {}
            (bits, Some(h)) if h.0 == bits => {}
            (bits, _) => {
                commands.entity(e).insert(Hazard(bits));
            }
        }
    }
}

/// The hook of `Cell`: an entity that is despawned leaves its column, whoever despawns it (no caller has to remember). Nothing while the
/// `Index` is out of the world (`zone_step` holds it and removes by hand).
pub fn cell_removed(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(c) = world.get::<Cell>(ctx.entity).map(|c| c.0).filter(|&c| c != u32::MAX) else { return };
    if let Some(mut idx) = world.get_resource_mut::<Index>() {
        idx.remove(ctx.entity, c);
    }
}

/// The hook of `Moving`: an item or an orb that starts to move is listed for its system (nothing for the others).
pub fn moving_added(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    if world.get::<crate::item::Item>(ctx.entity).is_some() {
        world.commands().entity(ctx.entity).insert(crate::item::ItemAwake);
    } else if world.get::<crate::orb::Orb>(ctx.entity).is_some() {
        world.commands().entity(ctx.entity).insert(crate::orb::OrbAwake);
    }
}

/// The hooks that register the carriers of a watcher component with the `Index`: `on_insert = watch_in::<K>, on_discard = watch_out::<K>`.
pub(crate) fn watch_in<const K: usize>(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let cell = world.get::<Cell>(ctx.entity).map_or(u32::MAX, |c| c.0);
    if let Some(mut idx) = world.get_resource_mut::<Index>() {
        idx.watch_add(ctx.entity, K, cell);
    }
}

pub(crate) fn watch_out<const K: usize>(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let cell = world.get::<Cell>(ctx.entity).map_or(u32::MAX, |c| c.0);
    if let Some(mut idx) = world.get_resource_mut::<Index>() {
        idx.watch_remove(ctx.entity, K, cell);
    }
}
