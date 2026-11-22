//! The simulated chunks: a creature whose chunk does not tick costs nothing and keeps its time.
//!
//! Horizon's rule (`ticket_levels.rs`, `creatures/borrowed_tick.rs:263-270`): a `Mob` in a loaded chunk that is not
//! entity-ticking is not ticked at all (no AI, no physics, no age, fire, love or egg timer, no despawn check) and picks
//! up where it was when the chunk ticks again, with no catch-up. The chunks that tick are the ones within
//! `min(view distance, simulation distance)` chunks (Chebyshev) of a player.
//!
//! Here a count of the players that cover each chunk is kept by EVENT (a player changes chunk: only the chunks of the
//! ring between the two squares are touched; a creature changes chunk: its new chunk is looked at). A chunk that goes from
//! 1 player to 0 puts its creatures to sleep, one that goes from 0 to 1 wakes them.
//!
//! A sleeping creature (`Dormant`, form (a)) keeps its row in the world with the `Disabled` marker, which takes it out of
//! every query; its sparse-set markers (the goals that were running, `Moving`...) are taken off and kept in `Dormant`, and
//! its entries of the timer wheel are taken out. Waking it shifts every deadline it holds by the time it slept (the
//! `Mob`'s counters do not count while it is not ticked) and puts the entries back.
use crate::IFloor as _;
use crate::cow::{Clock, Crowded, Due, Following, Index, Moving, NeedsPath, Overlapping, Players, Recheck, Touching, Wheel};
use bevy_ecs::{entity_disabling::Disabled, prelude::*};

/// How a creature is kept while its chunk does not tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Form {
    /// Kept in the world, out of the queries (`Disabled`), its markers and wheel entries put aside.
    #[default]
    Marker,
}

/// A creature asleep: when it fell asleep and the entries it had in the wheel (as ticks from then), and the markers of the
/// frame it had.
#[derive(Component, Debug)]
pub struct Dormant {
    pub since: u64,
    pub wheel: Vec<(u64, Due)>,
    pub stash: Vec<Stashed>,
}

/// The sparse-set markers of the frame taken off a creature that sleeps (they would be visited every tick otherwise). The
/// markers of the modules stay on it, with their data: they are in the entity's own time, which stands still.
#[derive(Debug)]
pub enum Stashed {
    Moving,
    Following,
    NeedsPath,
    Touching,
    Overlapping,
    Crowded,
    /// A projectile's flight markers (`wake_object` puts them back).
    Flying,
    TridentFlying,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ZoneStats {
    pub slept: u64,
    pub woke: u64,
}

/// The count of players covering each chunk of the slab, by event.
#[derive(Default)]
pub struct Zone {
    pub on: bool,
    pub radius: i32,
    pub form: Form,
    pub stats: ZoneStats,
    width: usize,
    cx0: i32,
    cz0: i32,
    /// Chunks per side, the chunk of each column, the first column of each chunk (and one past the last).
    cw: i32,
    col: Vec<i32>,
    first: Vec<usize>,
    count: Vec<u16>,
    pre: Vec<u16>,
    stamp: Vec<u32>,
    touched: Vec<usize>,
    epoch: u32,
    /// The chunk (grid coordinates, may be off the grid) each player stands in.
    chunks: Vec<(i32, i32)>,
}

const CHUNK: f64 = 16.0;

impl Zone {
    fn build_grid(&mut self, origin: f64, width: usize, cell: f64) {
        self.width = width;
        self.cx0 = (origin / CHUNK).ifloor();
        self.cz0 = self.cx0;
        self.col = (0..width).map(|c| ((origin + c as f64 * cell) / CHUNK).ifloor() - self.cx0).collect();
        self.cw = self.col.last().copied().unwrap_or(0) + 1;
        self.first = vec![0; self.cw as usize + 1];
        for k in 0..self.cw as usize {
            self.first[k] = self.col.iter().position(|&c| c == k as i32).unwrap_or(width);
        }
        self.first[self.cw as usize] = width;
        let n = (self.cw * self.cw) as usize;
        self.count = vec![0; n];
        self.pre = vec![0; n];
        self.stamp = vec![0; n];
        self.touched.clear();
        self.chunks.clear();
    }
    /// The chunk (grid index) of a column pair.
    fn chunk_of_cell(&self, cell: u32) -> usize {
        let (cx, cz) = (cell as usize % self.width, cell as usize / self.width);
        (self.col[cz] * self.cw + self.col[cx]) as usize
    }
    /// Whether the chunk of a cell ticks.
    pub fn simulated_cell(&self, cell: u32) -> bool {
        !self.on || self.count[self.chunk_of_cell(cell)] > 0
    }
    /// Whether the chunk at block coordinates ticks.
    pub fn simulated_at(&self, x: f64, z: f64) -> bool {
        if !self.on {
            return true;
        }
        let (cx, cz) = ((x / CHUNK).ifloor() - self.cx0, (z / CHUNK).ifloor() - self.cz0);
        (0..self.cw).contains(&cx) && (0..self.cw).contains(&cz) && self.count[(cz * self.cw + cx) as usize] > 0
    }
    /// Whether chunk `(cx, cz)` (absolute chunk coordinates) ticks.
    pub fn chunk_simulated(&self, cx: i32, cz: i32) -> bool {
        let (x, z) = (cx - self.cx0, cz - self.cz0);
        !self.on || ((0..self.cw).contains(&x) && (0..self.cw).contains(&z) && self.count[(z * self.cw + x) as usize] > 0)
    }
    /// How many chunks tick.
    pub fn simulated_chunks(&self) -> usize {
        self.count.iter().filter(|&&c| c > 0).count()
    }
    fn bump(&mut self, k: usize, up: bool) {
        if self.stamp[k] != self.epoch {
            self.stamp[k] = self.epoch;
            self.pre[k] = self.count[k];
            self.touched.push(k);
        }
        if up {
            self.count[k] += 1;
        } else {
            self.count[k] -= 1;
        }
    }
    /// The chunks of the square of `radius` around `at` that `other` does not hold, clipped to the grid: each is bumped.
    fn bump_square_minus(&mut self, at: (i32, i32), other: Option<(i32, i32)>, up: bool) {
        let r = self.radius;
        let (x0, x1, z0, z1) = ((at.0 - r).max(0), (at.0 + r).min(self.cw - 1), (at.1 - r).max(0), (at.1 + r).min(self.cw - 1));
        for z in z0..=z1 {
            // The part of this row `other`'s square holds, if any.
            let hold = other.filter(|o| (z - o.1).abs() <= r).map(|o| (o.0 - r, o.0 + r));
            match hold {
                None => (x0..=x1).for_each(|x| self.bump((z * self.cw + x) as usize, up)),
                Some((h0, h1)) => {
                    for x in x0..=x1.min(h0 - 1) {
                        self.bump((z * self.cw + x) as usize, up);
                    }
                    for x in x0.max(h1 + 1)..=x1 {
                        self.bump((z * self.cw + x) as usize, up);
                    }
                }
            }
        }
    }
    /// The players moved, came or left: the chunks of the two squares of each are counted again, only the ring they
    /// differ by. Fills `touched`.
    fn sync_players(&mut self, list: &[(f64, f64)]) {
        self.epoch += 1;
        let n = list.len().max(self.chunks.len());
        for i in 0..n {
            let new = list.get(i).map(|p| ((p.0 / CHUNK).ifloor() - self.cx0, (p.1 / CHUNK).ifloor() - self.cz0));
            let old = self.chunks.get(i).copied();
            if new == old {
                continue;
            }
            if let Some(o) = old {
                self.bump_square_minus(o, new, false);
            }
            if let Some(nw) = new {
                self.bump_square_minus(nw, old, true);
            }
        }
        self.chunks.clear();
        self.chunks.extend(list.iter().map(|p| ((p.0 / CHUNK).ifloor() - self.cx0, (p.1 / CHUNK).ifloor() - self.cz0)));
    }
    /// The chunks whose state changed (true: now ticks), after `sync_players`.
    fn transitions(&mut self) -> Vec<(usize, bool)> {
        let out = self.touched.iter().filter(|&&k| (self.pre[k] > 0) != (self.count[k] > 0)).map(|&k| (k, self.count[k] > 0)).collect();
        self.touched.clear();
        out
    }
}

/// The creatures in the columns of chunk `k`.
fn in_chunk(idx: &Index, k: usize, out: &mut Vec<Entity>) {
    let z = &idx.zone;
    let (kx, kz) = (k % z.cw as usize, k / z.cw as usize);
    for zz in z.first[kz]..z.first[kz + 1] {
        for xx in z.first[kx]..z.first[kx + 1] {
            out.extend_from_slice(&idx.cells[zz * idx.width + xx]);
        }
    }
}

pub fn zone_on(idx: Res<Index>) -> bool {
    idx.zone.on
}

/// First system of the tick: the players' chunks, the creatures that changed chunk, then the chunks that began or ceased to
/// tick.
pub fn zone_step(world: &mut World) {
    let now = world.resource::<Clock>().0;
    world.resource_scope(|world, mut idx: Mut<Index>| {
        let mut slept: Vec<Entity> = Vec::new();
        let mut woke: Vec<Entity> = Vec::new();
        let dirty = std::mem::take(&mut world.resource_mut::<Players>().zone_dirty);
        let (mut down, mut up) = (false, false);
        if dirty {
            let list = world.resource::<Players>().list.clone();
            idx.zone.sync_players(&list);
            for (k, ticks) in idx.zone.transitions() {
                if ticks {
                    up = true;
                } else {
                    down = true;
                }
                let mut found = Vec::new();
                in_chunk(&idx, k, &mut found);
                for e in found {
                    let asleep = world.entity(e).contains::<Dormant>();
                    match (ticks, asleep) {
                        (true, true) => woke.push(e),
                        (false, false) => slept.push(e),
                        _ => {}
                    }
                }
            }
        }
        // A creature that walked into a chunk that does not tick goes to sleep (its tick that moved it was its last).
        for &(e, _) in &idx.crossed {
            let Some(c) = world.get::<crate::cow::Cell>(e) else { continue };
            if !idx.zone.simulated_cell(c.0) && !world.entity(e).contains::<Dormant>() {
                slept.push(e);
            }
        }
        for e in slept {
            // A creature can be listed by its chunk and by its move.
            if world.entity(e).contains::<Dormant>() {
                continue;
            }
            sleep(world, e, now);
            idx.zone.stats.slept += 1;
        }
        for e in woke {
            wake(world, e, now);
            idx.zone.stats.woke += 1;
        }
        // The objects outside the index (projectiles, hooks, eyes) are not listed by chunk: on a chunk that began or ceased to tick, the
        // ones in flight (or stuck) are looked at once, by event and never per tick, and the frozen ones likewise. Cost: one pass over those
        // sets per change of a player's chunk that flipped a chunk, nothing per entity and nothing while no player moves.
        if down {
            let list: Vec<Entity> = world.query_filtered::<(Entity, &crate::cow::Pos), (Loose, Without<Dormant>)>().iter(world).filter(|(_, p)| !idx.zone.simulated_at(p.0.x, p.0.z)).map(|(e, _)| e).collect();
            idx.zone.stats.slept += list.len() as u64;
            for e in list {
                // A `Hurting` projectile leaves the loaded chunks: it is discarded, not kept (`projectile::fly`).
                if world.get::<crate::projectile::Flight>(e).is_some_and(|f| f.order == crate::projectile::Order::Hurting) {
                    world.despawn(e);
                } else {
                    freeze(world, e, now);
                }
            }
        }
        if up {
            let list: Vec<Entity> = world
                .query_filtered::<(Entity, &crate::cow::Pos), (Loose, With<Dormant>, Allow<Disabled>)>()
                .iter(world)
                .filter(|(_, p)| idx.zone.simulated_at(p.0.x, p.0.z))
                .map(|(e, _)| e)
                .collect();
            idx.zone.stats.woke += list.len() as u64;
            for e in list {
                thaw(world, e, now);
            }
        }
    });
}

/// The objects that have a tick of their own and no row in the creature index: a projectile, a fishing hook, an eye of ender, a primed TNT, a falling block.
type Loose = Or<(With<crate::projectile::Projectile>, With<crate::fishing_hook::Hook>, With<crate::eye::Eye>, With<crate::tnt::Primed>, With<crate::falling_block::FallingBlock>)>;

/// Freezes an object whose chunk does not tick (`ServerLevel.tick`: `inEntityTickingRange` gates `Entity.tick`): out of the queries
/// (`Disabled`), its flight markers and its wheel entries (a stuck projectile's life) put aside, its own time stopped.
pub fn freeze(world: &mut World, e: Entity, now: u64) {
    if world.get_entity(e).is_err_and(|_| true) || world.entity(e).contains::<Dormant>() {
        return;
    }
    let entries = world.resource_mut::<Wheel>().take_entries(e);
    let wheel = entries.into_iter().map(|(at, d)| (at - now, d)).collect();
    let mut em = world.entity_mut(e);
    let mut stash = Vec::new();
    if em.take::<crate::projectile::Flying>().is_some() {
        stash.push(Stashed::Flying);
    }
    if em.take::<crate::trident::TridentFlying>().is_some() {
        stash.push(Stashed::TridentFlying);
    }
    if em.take::<crate::cow::Moving>().is_some() {
        stash.push(Stashed::Moving);
    }
    em.insert((Disabled, Dormant { since: now, wheel, stash }));
}

/// A loose object just made: it waits at once when its chunk does not tick.
pub fn settle(world: &mut World, e: Entity) {
    if world.get_resource::<Index>().is_some_and(|i| world.get::<crate::cow::Pos>(e).is_some_and(|p| !i.zone.simulated_at(p.0.x, p.0.z))) {
        if world.get::<crate::projectile::Flight>(e).is_some_and(|f| f.order == crate::projectile::Order::Hurting) {
            world.despawn(e);
            return;
        }
        let now = world.resource::<Clock>().0;
        freeze(world, e, now);
    }
}

/// Thaws a frozen object: its own time resumes, the markers and the entries come back where they were, and a stuck projectile looks
/// again at whether its block still holds it (the check `AbstractArrow.tick` makes every tick it is ticked).
pub fn thaw(world: &mut World, e: Entity, now: u64) {
    let mut em = world.entity_mut(e);
    let Some(mut dormant) = em.get_mut::<Dormant>() else { return };
    let (since, wheel, stash) = (dormant.since, std::mem::take(&mut dormant.wheel), std::mem::take(&mut dormant.stash));
    em.remove::<(Disabled, Dormant)>();
    for s in stash {
        match s {
            Stashed::Flying => drop(em.insert(crate::projectile::Flying)),
            Stashed::TridentFlying => drop(em.insert(crate::trident::TridentFlying)),
            Stashed::Moving => drop(em.insert(Moving)),
            _ => {}
        }
    }
    world.resource_mut::<Clock>().sleep_for(e, now - since);
    let mut wh = world.resource_mut::<Wheel>();
    for (rem, due) in wheel {
        wh.push(now + rem, e, due);
    }
    crate::projectile::recheck_stuck(world, e);
}

/// Turns the zone on for `radius` chunks around the players (their list as `Players` holds it now): every creature of a
/// chunk that does not tick goes to sleep.
pub fn enable(world: &mut World, radius: i32) {
    world.resource_mut::<Wheel>().enable_tracking();
    let now = world.resource::<Clock>().0;
    let list = world.resource::<Players>().list.clone();
    world.resource_scope(|world, mut idx: Mut<Index>| {
        let (origin, width) = (idx.origin, idx.width);
        let z = &mut idx.zone;
        z.build_grid(origin, width, 1.0);
        z.on = true;
        z.radius = radius.max(2);
        // Every chunk ticked until now.
        z.epoch += 1;
        for k in 0..z.count.len() {
            z.stamp[k] = z.epoch;
            z.pre[k] = 1;
            z.touched.push(k);
        }
        z.sync_players(&list);
        let mut sleepers = Vec::new();
        for (k, ticks) in idx.zone.transitions() {
            if !ticks {
                in_chunk(&idx, k, &mut sleepers);
            }
        }
        for e in sleepers {
            if !world.entity(e).contains::<Dormant>() {
                sleep(world, e, now);
                idx.zone.stats.slept += 1;
            }
        }
    });
    world.resource_mut::<Players>().dirty = true;
}

/// New creatures (armed, in the index): the ones whose chunk does not tick go to sleep at once.
pub fn admit(world: &mut World, fresh: &[Entity]) {
    let now = world.resource::<Clock>().0;
    let idx = world.resource::<Index>();
    if !idx.zone.on {
        return;
    }
    let asleep: Vec<Entity> = fresh.iter().copied().filter(|&e| world.get::<Dormant>(e).is_none() && world.get::<crate::cow::Cell>(e).is_some_and(|c| !idx.zone.simulated_cell(c.0))).collect();
    for e in asleep {
        sleep(world, e, now);
        world.resource_mut::<Index>().zone.stats.slept += 1;
    }
}

/// Puts a creature to sleep: out of the queries, its entries out of the wheel, its frame markers aside. Its own time stops
/// (`Clock::of`): every deadline it holds, in every module, is in that time and needs no change.
pub fn sleep(world: &mut World, e: Entity, now: u64) {
    let entries = world.resource_mut::<Wheel>().take_entries(e);
    let wheel = entries.into_iter().map(|(at, d)| (at - now, d)).collect();
    let mut em = world.entity_mut(e);
    let mut stash = Vec::new();
    macro_rules! unit {
        ($($t:ident),*) => { $( if em.take::<$t>().is_some() { stash.push(Stashed::$t); } )* };
    }
    unit!(Moving, Following, NeedsPath, Touching, Overlapping, Crowded);
    // `Recheck` goes in with it: the creature is looked at again when it wakes, with no second move of its row.
    em.insert((Disabled, Dormant { since: now, wheel, stash }, Recheck));
}

/// Wakes a creature: its own time resumes (the world's time it slept is added to its offset), its wheel entries come back
/// where they were in its own time.
pub fn wake(world: &mut World, e: Entity, now: u64) {
    let mut em = world.entity_mut(e);
    let Some(mut dormant) = em.get_mut::<Dormant>() else { return };
    let (since, wheel, stash) = (dormant.since, std::mem::take(&mut dormant.wheel), std::mem::take(&mut dormant.stash));
    em.remove::<(Disabled, Dormant)>();
    for s in stash {
        match s {
            Stashed::Moving => drop(em.insert(Moving)),
            Stashed::Following => drop(em.insert(Following)),
            Stashed::NeedsPath => drop(em.insert(NeedsPath)),
            Stashed::Touching => drop(em.insert(Touching)),
            Stashed::Overlapping => drop(em.insert(Overlapping)),
            Stashed::Crowded => drop(em.insert(Crowded)),
            Stashed::Flying => drop(em.insert(crate::projectile::Flying)),
            Stashed::TridentFlying => drop(em.insert(crate::trident::TridentFlying)),
        }
    }
    // The world may have changed under it while it slept (a block, a fluid): its physics looks again.
    em.insert(Moving);
    world.resource_mut::<Clock>().sleep_for(e, now - since);
    // The melee check keeps its time in the WORLD's (vanilla's `lastCanUseCheck` against `getGameTime`): after 20 ticks or more
    // asleep the last ask is old and `canUse` is asked at once, as the `Mob` does.
    let melee = world.get::<crate::hostile::Attack>(e).filter(|_| now - since >= 20).map(|a| a.melee_slot);
    let mut wh = world.resource_mut::<Wheel>();
    for (rem, due) in wheel {
        let stale = matches!((due, melee), (Due::Goal(s), Some(m)) if s == m);
        wh.push(if stale { now + 1 } else { now + rem }, e, due);
    }
    // Its trackers dropped it while it slept (or never had it): it is sent again to whoever sees it now.
    if let Some(mut m) = world.get_resource_mut::<crate::bridge::view::Modified>() {
        m.push(e);
    }
    crate::despawn::on_wake(world, e);
    crate::spring_trap::on_wake(world, e);
    // A creaking back in the ticking zone asks its heart again (`Creaking.tick`).
    crate::bridge::host_creaking::ask_heart(world, e);
    // A brain that was due a step when it fell asleep (or that never sleeps) was left out of the list: it is queued again.
    crate::brain::requeue(world, e);
}
