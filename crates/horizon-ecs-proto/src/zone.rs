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
use crate::{
    cow::{
        ActiveUntil, Burning, Clock, Crowded, Due, GrowAt, HurtAt, Index, Moving, Nav, Overlapping, Players, Recheck, Touching, Wheel,
    },
    farm::{Egg, Grazing},
    goals::{Breeding, Glancing, Love, Panic, Phase, Resting, Scanning, Tempting, Tempts, Trailing},
    hostile::{Aggro, Exposed, Meleeing},
};
use bevy_ecs::{entity_disabling::Disabled, prelude::*};

/// How a creature is kept while its chunk does not tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Form {
    /// Kept in the world, out of the queries (`Disabled`), its markers and wheel entries put aside.
    #[default]
    Marker,
}

/// A creature asleep: when it fell asleep, the entries it had in the wheel (as ticks from then), the markers it had.
#[derive(Component, Debug)]
pub struct Dormant {
    pub since: u64,
    pub wheel: Vec<(u64, Due)>,
    pub stash: Vec<Stashed>,
}

/// The sparse-set components taken off a creature that sleeps (they would be visited every tick otherwise).
#[derive(Debug)]
pub enum Stashed {
    Moving,
    Following,
    NeedsPath,
    Floating,
    Panicking,
    Looking,
    Tempting(Tempting),
    Breeding(Breeding),
    Trailing(Trailing),
    Glancing(Glancing),
    Scanning(Scanning),
    Grazing(Grazing),
    Touching,
    Overlapping,
    Crowded,
    Aggro(Aggro),
    Meleeing,
    Exposed(Exposed),
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
        self.cx0 = (origin / CHUNK).floor() as i32;
        self.cz0 = self.cx0;
        self.col = (0..width).map(|c| ((origin + c as f64 * cell) / CHUNK).floor() as i32 - self.cx0).collect();
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
        let (cx, cz) = ((x / CHUNK).floor() as i32 - self.cx0, (z / CHUNK).floor() as i32 - self.cz0);
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
            let new = list.get(i).map(|p| ((p.0 / CHUNK).floor() as i32 - self.cx0, (p.1 / CHUNK).floor() as i32 - self.cz0));
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
        self.chunks.extend(list.iter().map(|p| ((p.0 / CHUNK).floor() as i32 - self.cx0, (p.1 / CHUNK).floor() as i32 - self.cz0)));
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
        let dirty = world.resource::<Players>().dirty;
        if dirty {
            let list = world.resource::<Players>().list.clone();
            idx.zone.sync_players(&list);
            for (k, ticks) in idx.zone.transitions() {
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
        for &e in &idx.crossed {
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
    });
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

/// Puts a creature to sleep: out of the queries, its markers and wheel entries aside.
pub fn sleep(world: &mut World, e: Entity, now: u64) {
    let entries = world.resource_mut::<Wheel>().take_entries(e);
    if std::env::var_os("ZDEBUG").is_some() {
        eprintln!("sleep {e:?} at {now}: {entries:?}");
    }
    let wheel = entries.into_iter().map(|(at, d)| (at - now, d)).collect();
    let mut em = world.entity_mut(e);
    let mut stash = Vec::new();
    macro_rules! unit {
        ($($t:ident),*) => { $( if em.take::<crate::cow::$t>().is_some() { stash.push(Stashed::$t); } )* };
    }
    macro_rules! goal_unit {
        ($($t:ident),*) => { $( if em.take::<crate::goals::$t>().is_some() { stash.push(Stashed::$t); } )* };
    }
    macro_rules! data {
        ($($m:ident :: $t:ident),*) => { $( if let Some(x) = em.take::<crate::$m::$t>() { stash.push(Stashed::$t(x)); } )* };
    }
    unit!(Moving, Following, NeedsPath, Touching, Overlapping, Crowded);
    goal_unit!(Floating, Panicking, Looking);
    data!(goals::Tempting, goals::Breeding, goals::Trailing, goals::Glancing, goals::Scanning, farm::Grazing, hostile::Aggro, hostile::Exposed);
    if em.take::<Meleeing>().is_some() {
        stash.push(Stashed::Meleeing);
    }
    em.insert((Disabled, Dormant { since: now, wheel, stash }));
}

/// Wakes a creature: every deadline it holds moves by the time it slept, its markers and wheel entries come back.
pub fn wake(world: &mut World, e: Entity, now: u64) {
    let mut em = world.entity_mut(e);
    let Some(Dormant { since, wheel, stash }) = em.take::<Dormant>() else { return };
    em.remove::<Disabled>();
    let d = now - since;
    shift_times(&mut em, e, d);
    for s in stash {
        match s {
            Stashed::Moving => drop(em.insert(crate::cow::Moving)),
            Stashed::Following => drop(em.insert(crate::cow::Following)),
            Stashed::NeedsPath => drop(em.insert(crate::cow::NeedsPath)),
            Stashed::Floating => drop(em.insert(crate::goals::Floating)),
            Stashed::Panicking => drop(em.insert(crate::goals::Panicking)),
            Stashed::Looking => drop(em.insert(crate::goals::Looking)),
            Stashed::Tempting(x) => drop(em.insert(x)),
            Stashed::Breeding(mut x) => {
                x.since += d;
                drop(em.insert(x))
            }
            Stashed::Trailing(x) => drop(em.insert(x)),
            Stashed::Glancing(mut x) => {
                x.until += d;
                drop(em.insert(x))
            }
            Stashed::Scanning(mut x) => {
                x.until += d;
                drop(em.insert(x))
            }
            Stashed::Grazing(mut x) => {
                x.eat_at += d;
                x.end_at += d;
                drop(em.insert(x))
            }
            Stashed::Touching => drop(em.insert(Touching)),
            Stashed::Overlapping => drop(em.insert(Overlapping)),
            Stashed::Crowded => drop(em.insert(Crowded)),
            Stashed::Aggro(x) => drop(em.insert(x)),
            Stashed::Meleeing => drop(em.insert(Meleeing)),
            Stashed::Exposed(x) => drop(em.insert(x)),
        }
    }
    // The world may have changed under it while it slept (a block, a fluid): its physics looks again.
    em.insert((Moving, Recheck));
    if std::env::var_os("ZDEBUG").is_some() {
        eprintln!("wake {e:?} at {now} (slept {d}): {wheel:?}");
    }
    let mut wh = world.resource_mut::<Wheel>();
    for (rem, due) in wheel {
        wh.push(now + rem, e, due);
    }
}

/// Moves every absolute deadline of the creature by `d` ticks (the `Mob`'s counters did not count while it slept).
fn shift_times(em: &mut EntityWorldMut, e: Entity, d: u64) {
    if d == 0 {
        return;
    }
    macro_rules! add {
        ($t:ty, $($f:ident),+) => { if let Some(mut c) = em.get_mut::<$t>() { $( c.$f += d; )+ } };
    }
    if let Some(mut a) = em.get_mut::<ActiveUntil>()
        && a.0 != u64::MAX
    {
        a.0 += d;
    }
    if let Some(mut g) = em.get_mut::<GrowAt>() {
        g.0 += d;
    }
    if let Some(mut l) = em.get_mut::<Love>() {
        l.0 += d;
    }
    if let Some(mut r) = em.get_mut::<Resting>() {
        r.0 += d;
    }
    add!(HurtAt, at);
    add!(Burning, until);
    add!(Panic, until);
    add!(Nav, last_check, last_recompute);
    if let Some(mut egg) = em.get_mut::<Egg>()
        && egg.at != u64::MAX
    {
        egg.at += d;
    }
    if let Some(mut ts) = em.get_mut::<Tempts>() {
        for t in &mut ts.0 {
            t.calm_until += d;
            if t.paused_at != 0 {
                t.paused_at += d;
            }
        }
    }
    // The goal selector asks on the ticks where tick + phase is even: `Mob` counts its own ticks, which did not pass.
    let phase = match em.get::<Phase>().map(|p| p.0) {
        Some(u64::MAX) | None => e.index().index() as u64,
        Some(p) => p,
    };
    em.insert(Phase(phase + d));
}
