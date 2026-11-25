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
use crate::cow::{Chunks, Clock, Crowded, Due, Following, Index, Moving, NeedsPath, Overlapping, Players, Recheck, Touching, Wheel, NO_SLOT};
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

/// The count of players covering each loaded chunk, by event (the counts live in the chunks' records, `cow/chunks.rs`).
#[derive(Default)]
pub struct Zone {
    pub on: bool,
    pub radius: i32,
    pub form: Form,
    pub stats: ZoneStats,
    /// The slots of the chunks whose count changed since `sync_players` began.
    pub(crate) touched: Vec<u32>,
    epoch: u32,
    /// The chunk (absolute coordinates) each player stands in.
    at: Vec<(i32, i32)>,
    /// The chunks a ticket other than a player's keeps entity-ticking (forced, portal, viewed): each counts as one more cover.
    pinned: horizon_core::fast_hash::FxHashSet<(i32, i32)>,
}

const CHUNK: f64 = 16.0;

impl Zone {
    /// How many players cover chunk `(cx, cz)` (0 while the zone is off).
    pub(crate) fn covering(&self, cx: i32, cz: i32) -> u16 {
        if !self.on {
            return 0;
        }
        let pin = u16::from(self.pinned.contains(&(cx, cz)));
        pin + self.at.iter().filter(|p| (p.0 - cx).abs() <= self.radius && (p.1 - cz).abs() <= self.radius).count() as u16
    }
    /// A ticket other than a player's starts (`up`) or stops keeping chunk `(cx, cz)` entity-ticking: its count moves by one, by event.
    /// The transition is read by the next `zone_step` (the caller marks `Players::zone_dirty`).
    pub(crate) fn pin(&mut self, ch: &mut Chunks, cx: i32, cz: i32, up: bool) -> bool {
        let moved = if up { self.pinned.insert((cx, cz)) } else { self.pinned.remove(&(cx, cz)) };
        if !moved || !self.on {
            return false;
        }
        // The pins of one interval share an epoch (`pre` is the count before the first), which `sync_players` keeps while some wait.
        if self.touched.is_empty() {
            self.epoch += 1;
        }
        self.bump(ch, cx, cz, up);
        true
    }
    fn bump(&mut self, ch: &mut Chunks, x: i32, z: i32, up: bool) {
        // A chunk that is not loaded has no count: it is worked out when it is made (`Index::chunk_created`).
        let k = ch.slot_at(x, z);
        if k == NO_SLOT {
            return;
        }
        let r = &mut ch.recs[k as usize];
        if r.stamp != self.epoch {
            r.stamp = self.epoch;
            r.pre = r.count;
            self.touched.push(k);
        }
        if up {
            r.count += 1;
        } else {
            r.count -= 1;
        }
    }
    /// The chunks of the square of `radius` around `at` that `other` does not hold: each is bumped.
    fn bump_square_minus(&mut self, ch: &mut Chunks, at: (i32, i32), other: Option<(i32, i32)>, up: bool) {
        let r = self.radius;
        for z in at.1 - r..=at.1 + r {
            // The part of this row `other`'s square holds, if any.
            let hold = other.filter(|o| (z - o.1).abs() <= r).map(|o| (o.0 - r, o.0 + r));
            match hold {
                None => (at.0 - r..=at.0 + r).for_each(|x| self.bump(ch, x, z, up)),
                Some((h0, h1)) => {
                    for x in at.0 - r..=(at.0 + r).min(h0 - 1) {
                        self.bump(ch, x, z, up);
                    }
                    for x in (at.0 - r).max(h1 + 1)..=at.0 + r {
                        self.bump(ch, x, z, up);
                    }
                }
            }
        }
    }
    /// The players moved, came or left: the chunks of the two squares of each are counted again, only the ring they
    /// differ by. Fills `touched`.
    fn sync_players(&mut self, ch: &mut Chunks, list: &[(f64, f64)]) {
        if self.touched.is_empty() {
            self.epoch += 1;
        }
        let n = list.len().max(self.at.len());
        for i in 0..n {
            let new = list.get(i).map(|p| ((p.0 / CHUNK).ifloor(), (p.1 / CHUNK).ifloor()));
            let old = self.at.get(i).copied();
            if new == old {
                continue;
            }
            if let Some(o) = old {
                self.bump_square_minus(ch, o, new, false);
            }
            if let Some(nw) = new {
                self.bump_square_minus(ch, nw, old, true);
            }
        }
        self.at.clear();
        self.at.extend(list.iter().map(|p| ((p.0 / CHUNK).ifloor(), (p.1 / CHUNK).ifloor())));
    }
    /// The chunks whose state changed (true: now ticks), after `sync_players`.
    fn transitions(&mut self, ch: &Chunks) -> Vec<(u32, bool)> {
        let out = self.touched.iter().filter_map(|&k| ch.recs.get(k as usize).filter(|r| (r.pre > 0) != (r.count > 0)).map(|r| (k, r.count > 0))).collect();
        self.touched.clear();
        out
    }
}

/// The creatures in the columns of the chunk in `slot`.
fn in_chunk(idx: &Index, slot: u32, out: &mut Vec<Entity>) {
    if let Some(c) = idx.chunks.recs[slot as usize].cols.as_deref() {
        out.extend(c.cells.iter().flatten().copied());
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
            let ix = &mut *idx;
            ix.zone.sync_players(&mut ix.chunks, &list);
            for (k, ticks) in ix.zone.transitions(&ix.chunks) {
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
            if !idx.simulated_cell(c.0) && !world.entity(e).contains::<Dormant>() {
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
            let list: Vec<Entity> = crate::queries::with::<(Entity, &'static crate::cow::Pos), (Loose, Without<Dormant>), _>(world, |q, world| q.iter(world).filter(|(_, p)| !idx.simulated_at(p.0.x, p.0.z)).map(|(e, _)| e).collect());
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
                .filter(|(_, p)| idx.simulated_at(p.0.x, p.0.z))
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
    if world.get_resource::<Index>().is_some_and(|i| world.get::<crate::cow::Pos>(e).is_some_and(|p| !i.simulated_at(p.0.x, p.0.z))) {
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
        let ix = &mut *idx;
        ix.zone.on = true;
        ix.zone.radius = radius.max(2);
        ix.zone.at.clear();
        ix.zone.touched.clear();
        // Every chunk ticked until now.
        ix.zone.epoch += 1;
        let epoch = ix.zone.epoch;
        let live: Vec<u32> = ix.chunks.live().collect();
        for k in live {
            let r = &mut ix.chunks.recs[k as usize];
            (r.stamp, r.pre, r.count) = (epoch, 1, 0);
            ix.zone.touched.push(k);
        }
        ix.zone.sync_players(&mut ix.chunks, &list);
        let mut sleepers = Vec::new();
        for (k, ticks) in ix.zone.transitions(&ix.chunks) {
            if !ticks {
                in_chunk(ix, k, &mut sleepers);
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

/// The simulation distance changed (a server's properties, `/setsimulationdistance`): the zone is counted again at `radius`; the creatures
/// of chunks that no longer tick sleep, those of chunks that now tick wake.
pub fn resize(world: &mut World, radius: i32) {
    if world.resource::<Index>().zone.radius == radius.max(2) {
        return;
    }
    enable(world, radius);
    let now = world.resource::<Clock>().0;
    let list: Vec<Entity> = world
        .query_filtered::<(Entity, &crate::cow::Cell), (With<Dormant>, Allow<Disabled>)>()
        .iter(world)
        .filter(|(_, c)| world.resource::<Index>().simulated_cell(c.0))
        .map(|(e, _)| e)
        .collect();
    for e in list {
        wake(world, e, now);
    }
}

/// New creatures (armed, in the index): the ones whose chunk does not tick go to sleep at once.
pub fn admit(world: &mut World, fresh: &[Entity]) {
    let now = world.resource::<Clock>().0;
    let idx = world.resource::<Index>();
    if !idx.zone.on {
        return;
    }
    let asleep: Vec<Entity> = fresh.iter().copied().filter(|&e| world.get::<Dormant>(e).is_none() && world.get::<crate::cow::Cell>(e).is_some_and(|c| !idx.simulated_cell(c.0))).collect();
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
