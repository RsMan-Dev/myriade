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
use horizon_client_lib::mobs::{
    Mob, MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

/// The widening of a slot's extent (blocks): covers the `f32` rounding of the relative extent.
pub(crate) const SLOT_MARGIN: f64 = 1e-3;

/// The side of a block of columns of the index, in columns.
pub(crate) const BLOCK: usize = 8;

/// `BLOCK` as a signed column count.
pub(crate) const B: i64 = BLOCK as i64;

/// The spatial index: the entities by 2-block column of their box's centre, a resource. It is changed only when a
/// box changes column (`reindex`, after the move): a cow that does not move costs it nothing.
#[derive(Resource, Default)]
pub struct Index {
    pub(crate) origin: f64,
    pub(crate) width: usize,
    pub(crate) cells: Vec<Vec<Entity>>,
    /// Parallel to `cells`: the XZ extent of each entity's box (min x, max x, min z, max z) relative to the column's corner, widened by `SLOT_MARGIN`,
    /// as `f32` (16 bytes per entity): `push` skips the neighbours whose extent does not touch its own without an ECS lookup. An entity whose box
    /// `reindex` has not seen yet holds an infinite extent (always a candidate).
    pub(crate) slots: Vec<Vec<[f32; 4]>>,
    pub(crate) stamp: Vec<u32>,
    /// The entities whose cell changed since the last `player_near` (`reindex` writes them).
    pub(crate) crossed: Vec<(Entity, u32)>,
    /// The simulated chunks (`zone.rs`): off unless `Herd::set_simulation` turned it on.
    pub zone: crate::zone::Zone,
    /// How many animals each block of 8 x 8 columns holds (`player_near` looks only at the blocks that hold some).
    /// A bit per column of each block (row-major, 8 x 8): the columns that hold an animal.
    pub(crate) blocks: Vec<u64>,
    pub(crate) bw: usize,
    /// The watched columns of each consumer of `CellMoves` that looks at the NEIGHBOURS of a mover (see `Watch`).
    pub(crate) watch: [Watch; WATCHERS],
    /// By entity index: the consumers (bit `WATCH_*`) the entity is a watcher of.
    pub(crate) watching: Vec<u8>,
}

/// The consumers of the moves of columns that wake the watchers around a mover. A consumer registers its watchers (an entity
/// carrying its component, see `watch_hooks!`); a mover that enters a column far from every watcher of the consumer costs it nothing.
pub const WATCH_FOLLOW_PARENT: usize = 0;

pub const WATCH_AVOID: usize = 1;

pub const WATCH_FOLLOW_MOB: usize = 2;

pub const WATCH_PUFFER: usize = 3;

pub(crate) const WATCHERS: usize = 4;

/// The side of a bucket of watched columns: a watcher within 32 columns of a mover is in the 3 x 3 buckets around its column.
pub(crate) const BUCKET: i64 = 32;

/// The watchers of one consumer, counted by bucket of 32 x 32 columns; `total` is zero while nobody watches (the whole cost then).
#[derive(Default)]
pub(crate) struct Watch {
    pub(crate) total: u32,
    pub(crate) counts: Vec<u32>,
}

impl Index {
    pub fn new(origin: f64, width: usize) -> Self {
        Self { origin, width, cells: vec![Vec::new(); width * width], slots: vec![Vec::new(); width * width], stamp: vec![0; width * width], crossed: Vec::new(), zone: Default::default(), blocks: vec![0; width.div_ceil(BLOCK).pow(2)], bw: width.div_ceil(BLOCK), watch: Default::default(), watching: Vec::new() }
    }
    #[inline]
    pub(crate) fn bucket_of(&self, cell: u32) -> (usize, usize) {
        let (cx, cz) = ((cell as usize % self.width) as i64 / BUCKET, (cell as usize / self.width) as i64 / BUCKET);
        (cx as usize, cz as usize)
    }
    pub(crate) fn buckets(&self) -> usize {
        self.width.div_ceil(BUCKET as usize)
    }
    /// `e`, in column `cell`, watches for consumer `k` (adds it to the count of its bucket).
    pub(crate) fn watch_count(&mut self, k: usize, cell: u32, delta: i32) {
        let n = self.buckets();
        let (bx, bz) = self.bucket_of(cell);
        let w = &mut self.watch[k];
        if w.counts.is_empty() {
            w.counts = vec![0; n * n];
        }
        let c = &mut w.counts[bz * n + bx];
        *c = (*c as i64 + delta as i64) as u32;
        w.total = (w.total as i64 + delta as i64) as u32;
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
        if m == 0 || (old != u32::MAX && self.bucket_of(old) == self.bucket_of(new)) {
            return;
        }
        for k in 0..WATCHERS {
            if m >> k & 1 == 1 {
                if old != u32::MAX {
                    self.watch_count(k, old, -1);
                }
                self.watch_count(k, new, 1);
            }
        }
    }
    /// Does consumer `k` have a watcher within 32 columns of column `cell`? (zero cost while it has none.)
    #[inline]
    pub fn watched_near(&self, k: usize, cell: u32) -> bool {
        let w = &self.watch[k];
        if w.total == 0 {
            return false;
        }
        let n = self.buckets();
        let (bx, bz) = self.bucket_of(cell);
        (bz.saturating_sub(1)..=(bz + 1).min(n - 1)).any(|z| (bx.saturating_sub(1)..=(bx + 1).min(n - 1)).any(|x| w.counts[z * n + x] > 0))
    }
    pub(crate) fn column(&self, x: f64) -> i64 {
        let v = (x - self.origin) / CELL;
        let i = v as i64;
        i.saturating_sub(i64::from(v < i as f64))
    }
    pub fn cell_of(&self, x: f64, z: f64) -> u32 {
        let cx = self.column(x).clamp(0, self.width as i64 - 1) as usize;
        let cz = self.column(z).clamp(0, self.width as i64 - 1) as usize;
        (cz * self.width + cx) as u32
    }
    pub fn insert(&mut self, e: Entity, cell: u32) {
        self.cells[cell as usize].push(e);
        self.slots[cell as usize].push([f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY]);
        let (b, bit) = self.block_of(cell);
        self.blocks[b] |= bit;
    }
    #[inline]
    pub(crate) fn block_of(&self, cell: u32) -> (usize, u64) {
        let (cx, cz) = (cell as usize % self.width, cell as usize / self.width);
        ((cz / BLOCK) * self.bw + cx / BLOCK, 1 << ((cz % BLOCK) * BLOCK + cx % BLOCK))
    }
    pub fn remove(&mut self, e: Entity, cell: u32) {
        let list = &mut self.cells[cell as usize];
        if let Some(i) = list.iter().position(|&o| o == e) {
            list.swap_remove(i);
            self.slots[cell as usize].swap_remove(i);
            let emptied = list.is_empty();
            if emptied {
                let (b, bit) = self.block_of(cell);
                self.blocks[b] &= !bit;
            }
        }
    }
    /// Calls `f` for the entities in the columns within `r` blocks of `(x, z)` (a box is at most 2 blocks wide).
    /// Two levels: the blocks of 8 x 8 columns hold a bit per occupied column, so the cost follows the occupied columns, not the area.
    /// The visit order is the plain row-major order of the columns (rows outer, columns inner), as a loop over every column would give.
    pub fn for_radius(&self, x: f64, z: f64, r: f64, mut f: impl FnMut(Entity)) {
        let w = self.width as i64;
        let (x0, x1) = (self.column(x - r - 1.0).max(0), self.column(x + r + 1.0).min(w - 1));
        let (z0, z1) = (self.column(z - r - 1.0).max(0), self.column(z + r + 1.0).min(w - 1));
        if x0 > x1 || z0 > z1 {
            return;
        }
        let (bx0, bx1) = ((x0 / B) as usize, (x1 / B) as usize);
        let mut zz = z0;
        while zz <= z1 {
            let band = (zz / B) as usize * self.bw;
            let row_end = z1.min(zz / B * B + B - 1);
            if self.blocks[band + bx0..=band + bx1].iter().all(|&m| m == 0) {
                zz = row_end + 1;
                continue;
            }
            for z in zz..=row_end {
                let shift = (z % B) * B;
                for bx in bx0..=bx1 {
                    let mut bits = (self.blocks[band + bx] >> shift) & 0xff;
                    if bits == 0 {
                        continue;
                    }
                    let lo = if bx == bx0 { x0 % B } else { 0 };
                    let hi = if bx == bx1 { x1 % B } else { B - 1 };
                    bits &= (0xffu64 << lo) & (0xffu64 >> (B - 1 - hi));
                    while bits != 0 {
                        let i = bits.trailing_zeros() as i64;
                        bits &= bits - 1;
                        for &e in &self.cells[(z * w + bx as i64 * B + i) as usize] {
                            f(e);
                        }
                    }
                }
            }
            zz = row_end + 1;
        }
    }
    /// Calls `f` for the entities whose column may lie in the disk of radius `r` around `old` and not in the one around `new` (a
    /// superset: the crescent a player's step sweeps, or a whole disk when `new` is `None`), row by row: the cost follows the length
    /// of the crescent, not the area of the disk.
    pub fn crescent(&self, old: (f64, f64), new: Option<(f64, f64)>, r: f64, mut f: impl FnMut(Entity)) {
        let w = self.width as i64;
        let (r, ro) = (r + 1.0, r - 1.0);
        for zz in self.column(old.1 - r).max(0)..=self.column(old.1 + r).min(w - 1) {
            let (z0, z1) = (self.origin + zz as f64 * CELL, self.origin + (zz + 1) as f64 * CELL);
            let near = (old.1.clamp(z0, z1) - old.1).abs();
            if near >= r {
                continue;
            }
            let half = (r * r - near * near).sqrt();
            let (mut a, b) = (self.column(old.0 - half).max(0), self.column(old.0 + half).min(w - 1));
            // The columns surely inside the new disk (its chord at the far edge of the row) are left out: two runs.
            let skip = new.and_then(|n| {
                let far = (n.1 - z0).abs().max((n.1 - z1).abs());
                (far < ro).then(|| ((ro * ro - far * far).sqrt(), n.0))
            });
            let mut run = |lo: i64, hi: i64, f: &mut dyn FnMut(Entity)| {
                for xx in lo..=hi {
                    for &e in &self.cells[(zz * w + xx) as usize] {
                        f(e);
                    }
                }
            };
            if let Some((h, nx)) = skip {
                let (s0, s1) = (self.column(nx - h + 1.0), self.column(nx + h - 1.0));
                run(a, b.min(s0 - 1), &mut f);
                a = a.max(s1 + 1);
            }
            run(a, b, &mut f);
        }
    }
    /// Calls `f` for the entities in the columns around `(x, z)` (3 x 3, a box is at most 2 blocks wide).
    /// Records the XZ extent of the box of `e`, which is in `cell` (see `slots`).
    pub(crate) fn set_box(&mut self, e: Entity, cell: u32, mn: (f64, f64), mx: (f64, f64)) {
        let Some(i) = self.cells[cell as usize].iter().rposition(|&o| o == e) else { return };
        let (x0, z0) = self.corner(cell);
        self.slots[cell as usize][i] = [(mn.0 - x0 - SLOT_MARGIN) as f32, (mx.0 - x0 + SLOT_MARGIN) as f32, (mn.1 - z0 - SLOT_MARGIN) as f32, (mx.1 - z0 + SLOT_MARGIN) as f32];
    }
    #[inline]
    pub(crate) fn corner(&self, cell: u32) -> (f64, f64) {
        let (cx, cz) = (cell as usize % self.width, cell as usize / self.width);
        (self.origin + cx as f64 * CELL, self.origin + cz as f64 * CELL)
    }
    /// `for_near` for the entities whose box may touch the XZ extent `mn..mx` (a superset: the caller tests the real boxes).
    pub fn for_near_touching(&self, x: f64, z: f64, mn: (f64, f64), mx: (f64, f64), mut f: impl FnMut(Entity)) {
        let (cx, cz) = (self.column(x), self.column(z));
        let w = self.width as i64;
        for zz in (cz - 1).max(0)..=(cz + 1).min(w - 1) {
            for xx in (cx - 1).max(0)..=(cx + 1).min(w - 1) {
                let c = (zz * w + xx) as usize;
                let list = &self.cells[c];
                if list.is_empty() {
                    continue;
                }
                let (x0, z0) = self.corner(c as u32);
                let (ax, bx, az, bz) = ((mn.0 - x0) as f32, (mx.0 - x0) as f32, (mn.1 - z0) as f32, (mx.1 - z0) as f32);
                for (&e, s) in list.iter().zip(&self.slots[c]) {
                    if s[1] >= ax && s[0] <= bx && s[3] >= az && s[2] <= bz {
                        f(e);
                    }
                }
            }
        }
    }
    pub fn for_near(&self, x: f64, z: f64, mut f: impl FnMut(Entity)) {
        let (cx, cz) = (self.column(x), self.column(z));
        let w = self.width as i64;
        for zz in (cz - 1).max(0)..=(cz + 1).min(w - 1) {
            for xx in (cx - 1).max(0)..=(cx + 1).min(w - 1) {
                for &e in &self.cells[(zz * w + xx) as usize] {
                    f(e);
                }
            }
        }
    }
}

/// A new animal is in the spatial index from its creation (not after its first tick: the pushes and the searches of that
/// tick see it).
pub fn enter_index(world: &mut World, e: Entity) {
    let Some(b) = world.get::<Bounds>(e).copied() else { return };
    let (mn, mx) = (b.0.min_position(), b.0.max_position());
    let cell = world.resource::<Index>().cell_of((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
    let old = world.get::<Cell>(e).unwrap().0;
    {
        let mut idx = world.resource_mut::<Index>();
        if old != u32::MAX {
            idx.remove(e, old);
        }
        idx.insert(e, cell);
        idx.move_watcher(e, old, cell);
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
    mut q: Query<(Entity, &Bounds, &mut Cell, Option<&Hazard>, Has<InFluid>, &mut FluidState), Changed<Bounds>>,
    // Read only when the entity enters or leaves a fluid (rare): not part of the query that runs over every moved entity.
    all_modes: Query<&crate::locomotion::Modes>,
    mut commands: Commands,
) {
    for (e, b, mut c, hz, in_fluid, mut fs) in &mut q {
        let (mn, mx) = (b.0.min_position(), b.0.max_position());
        let cell = idx.cell_of((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        if cell != c.0 {
            if c.0 != u32::MAX {
                idx.remove(e, c.0);
            }
            idx.insert(e, cell);
            idx.move_watcher(e, c.0, cell);
            c.0 = cell;
            moves.0.push(e);
            idx.crossed.push((e, cell));
        }
        idx.set_box(e, cell, (mn.x, mn.z), (mx.x, mx.z));
        let bits = hazards.of_box_kept(&b.0, c.bypass_change_detection(), hz.map_or(0, |h| h.0));
        let fluid = bits & (HAZ_WATER | HAZ_LAVA) != 0;
        if fluid && !in_fluid {
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
