//! The ordered pass: islands of creatures that touch, in the vanilla order.
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

/// How near (blocks) another animal must come for an animal to stay in the ordered pass: a little more than the largest
/// step of an animal in a tick.
pub(crate) const ORDERED_MARGIN: f64 = 0.1;

/// Two animals whose boxes come within this distance (blocks) of each other in the ordered pass are in one island: more
/// than the two largest steps of a tick (0.6 each).
pub(crate) const ISLAND_REACH: f64 = 1.2;

pub(crate) fn island_reach() -> f64 {
    static R: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *R.get_or_init(|| std::env::var("ISLAND_REACH").ok().and_then(|v| v.parse().ok()).unwrap_or(ISLAND_REACH))
}

/// An animal that overlapped another when `push` last looked at it (batch mode, `Config.keep_touching`): it is looked at
/// again each tick, moving or not, until nothing overlaps it.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Touching;

/// An animal that overlaps at least one other (ordered push mode): its travel and its push run in `ordered_pass`.
#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct Overlapping;

/// How many animals the ordered pass handled: the last tick, and the sum over the ticks (for the benches).
#[derive(Resource, Default)]
pub struct OrderedStats {
    pub last: usize,
    pub total: u64,
    pub ticks: u64,
    /// Islands over the ticks, the largest one, and how many islands had 1, 2-3, 4-10, 11-100, more than 100 animals.
    pub islands: u64,
    pub largest: usize,
    pub sizes: [u64; 5],
}

/// The ordered push mode (`Config.ordered_push`): the animals that overlap another travel and push in ONE pass, in the
/// order of their creation, with the `Mob` pass's semantics: an animal travels, then pushes every animal that overlaps
/// it (its own velocity and the other's at once); the impulse an animal gets from one that comes before it is used by
/// its own travel of this tick (the `Mob` delivers the event at the start of its turn), and the impulse from one that
/// comes after it by the travel of the next tick. Every other animal stays in the parallel batch systems.
/// What an island of the ordered pass asks the world for (applied after the islands ran, in the order of the islands).
pub(crate) enum Effect {
    Mark(Entity),
    Unmark(Entity),
    Moving(Entity),
    Crowded(Entity),
    /// A landing: the fall damage (0 when none or immune) and the distance fallen.
    Fall(Entity, f32, f64),
}

pub(crate) type OrderedItems = (
    Entity,
    &'static mut Pos,
    &'static mut Vel,
    &'static Yaw,
    &'static mut Contact,
    &'static mut Drive,
    &'static Body,
    &'static mut Bounds,
    &'static FluidState,
    Has<Following>,
    Option<&'static crate::goals::Jump>,
    &'static FluidParams,
    Has<Moving>,
    Option<&'static Immune>,
);

/// One island of the ordered pass, its animals in the order of creation: each travels, then pushes every animal that
/// overlaps its box now (see `ordered_pass`). Touches only the components of the animals of its island and of the
/// animals its members overlap, which no other island touches.
#[allow(clippy::type_complexity)]
pub(crate) fn run_island(
    ids: &[Entity],
    level: &Level,
    table: &SpeciesTable,
    idx: &Index,
    q: &Query<OrderedItems>,
    marked: &Query<Entity, With<Overlapping>>,
    moving_now: &Query<(), With<Moving>>,
) -> Vec<Effect> {
    let mut out = Vec::new();
    let mut near: Vec<Entity> = Vec::new();
    let mut hits: Vec<(Entity, f64, f64)> = Vec::new();
    let ro = q.as_readonly();
    for &e in ids {
        {
            // SAFETY: the islands are disjoint: no other thread touches the components of this animal.
            let Ok((_, mut p, mut v, y, mut c, mut d, body, mut b, fs, _following, jump, sp, _, imm)) = (unsafe { q.get_unchecked(e) }) else { continue };
            let mut fall = 0.0_f32;
            let jumping = jump.is_some_and(|j| j.0);
            let (was_in_air, fell) = (!c.on_ground, c.fall_distance);
            travel_one(level, sp, &mut p, &mut v, y, &mut c, &mut d, body, &mut b, fs, jumping, &mut fall);
            let dmg = if fall > 0.0 && !imm.is_some_and(|i| i.has(IMMUNE_FALL)) { fall } else { 0.0 };
            if dmg > 0.0 || (was_in_air && c.on_ground && fell > 1.0) {
                out.push(Effect::Fall(e, dmg, fell));
            }
        }
        let Ok(me) = ro.get(e) else { continue };
        let b = me.7.0;
        let still = me.2.0.x.abs() + me.2.0.z.abs() < 1.0e-4;
        let (mn, mx) = (b.min_position(), b.max_position());
        let (cx, cz) = ((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
        near.clear();
        hits.clear();
        idx.for_near(cx, cz, |o| near.push(o));
        let mut own = (0.0_f64, 0.0_f64);
        let mut overlapping = 0;
        let mut close = 0;
        let wide = b.inflate(Vec3::new(ORDERED_MARGIN, 0.0, ORDERED_MARGIN));
        for &o in &near {
            if o == e {
                continue;
            }
            let Ok(oth) = ro.get(o) else { continue };
            let ob = oth.7;
            if !b.intersects(ob.0) {
                if !still && wide.intersects(ob.0) {
                    close += 1;
                    if !marked.contains(o) {
                        out.push(Effect::Mark(o));
                    }
                }
                continue;
            }
            overlapping += 1;
            let (omn, omx) = (ob.0.min_position(), ob.0.max_position());
            let (mut xa, mut za) = ((omn.x + omx.x) / 2.0 - cx, (omn.z + omx.z) / 2.0 - cz);
            let mut dd = xa.abs().max(za.abs());
            if dd < f64::from(0.01_f32) {
                continue;
            }
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let pow = (1.0_f64 / dd).min(1.0);
            xa *= pow * f64::from(0.05_f32);
            za *= pow * f64::from(0.05_f32);
            own.0 -= xa;
            own.1 -= za;
            hits.push((o, xa, za));
        }
        // SAFETY: as above (this animal and the ones it overlaps belong to this island).
        if let Ok((_, _, mut v, ..)) = unsafe { q.get_unchecked(e) } {
            v.0 = Vec3::new(v.0.x + own.0, v.0.y, v.0.z + own.1);
        }
        for &(o, xa, za) in &hits {
            // SAFETY: as above.
            if let Ok((_, _, mut ov, ..)) = unsafe { q.get_unchecked(o) } {
                ov.0 = Vec3::new(ov.0.x + xa, ov.0.y, ov.0.z + za);
            }
            if !marked.contains(o) {
                out.push(Effect::Mark(o));
            }
            if !moving_now.contains(o) {
                out.push(Effect::Moving(o));
            }
        }
        if overlapping + close == 0 {
            out.push(Effect::Unmark(e));
        } else if overlapping > 0 && overlapping + 1 >= MAX_ENTITY_CRAMMING {
            out.push(Effect::Crowded(e));
        }
    }
    out
}

pub(crate) fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize];
        i = parent[i as usize];
    }
    i
}

/// The ordered push mode (`Config.ordered_push`): the animals that overlap another travel and push in islands: animals
/// connected by overlap (or by a gap too small to cross in one tick), each island in the order of creation of its
/// members, with the `Mob` pass's semantics: an animal travels, then pushes every animal that overlaps it (its own
/// velocity and the other's at once); the impulse an animal gets from one that comes before it is used by its own
/// travel of this tick, from one that comes after it by the next tick's. The islands do not touch each other, so they run
/// one after the other or in parallel (`Config.parallel`) with the same result. Every other animal stays in the batch.
#[allow(clippy::type_complexity)]
pub(crate) fn ordered_pass(
    cfg: Res<Config>,
    level: Res<Level>,
    table: Res<SpeciesTable>,
    idx: Res<Index>,
    mut stats: ResMut<OrderedStats>,
    mut queue: ResMut<DamageQueue>,
    q: Query<OrderedItems>,
    marked: Query<Entity, With<Overlapping>>,
    uids: Query<&crate::goals::Uid>,
    moving_now: Query<(), With<Moving>>,
    mut commands: Commands,
    mut node: Local<Vec<u32>>,
) {
    if !cfg.ordered_push {
        return;
    }
    let mut ids: Vec<Entity> = marked.iter().collect();
    // The order of creation (the `Mob` herd's list order): the animal's `Uid` (an entity index is reused after a death).
    ids.sort_unstable_by_key(|e| uids.get(*e).map_or(0, |u| u.0));
    stats.last = ids.len();
    stats.total += ids.len() as u64;
    stats.ticks += 1;
    // The islands: union-find over the marked animals and the animals within one tick's reach of them.
    let ro = q.as_readonly();
    // `node[entity index]`: the node of the animal in the union-find (u32::MAX: none); only the touched slots are reset.
    let mut touched: Vec<u32> = Vec::new();
    let need = ids.iter().map(|e| e.index().index() as usize + 1).max().unwrap_or(0);
    if node.len() < need {
        node.resize(need, u32::MAX);
    }
    let mut parent: Vec<u32> = Vec::with_capacity(ids.len() * 2);
    let mut ents: Vec<Entity> = Vec::with_capacity(ids.len() * 2);
    for &e in &ids {
        node[e.index().index() as usize] = parent.len() as u32;
        touched.push(e.index().index());
        parent.push(parent.len() as u32);
        ents.push(e);
    }
    let reach_by = island_reach();
    let mut near: Vec<Entity> = Vec::new();
    for &e in &ids {
        let Ok(me) = ro.get(e) else { continue };
        let reach = me.7.0.inflate(Vec3::new(reach_by, 0.0, reach_by));
        let (mn, mx) = (me.7.0.min_position(), me.7.0.max_position());
        near.clear();
        idx.for_near((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0, |o| near.push(o));
        let ne = node[e.index().index() as usize];
        for &o in &near {
            if o == e || !ro.get(o).is_ok_and(|ob| reach.intersects(ob.7.0)) {
                continue;
            }
            // An animal that is not in the pass is a node too (it receives impulses: two islands must not both give it any).
            let slot = o.index().index() as usize;
            if slot >= node.len() {
                node.resize(slot + 1, u32::MAX);
            }
            if node[slot] == u32::MAX {
                node[slot] = parent.len() as u32;
                touched.push(o.index().index());
                parent.push(parent.len() as u32);
                ents.push(o);
            }
            let (ra, rb) = (find(&mut parent, ne), find(&mut parent, node[slot]));
            if ra != rb {
                parent[ra.max(rb) as usize] = ra.min(rb);
            }
        }
    }
    let mut islands: Vec<Vec<Entity>> = Vec::new();
    let mut root_island: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for &e in &ids {
        let r = find(&mut parent, node[e.index().index() as usize]);
        let i = *root_island.entry(r).or_insert_with(|| {
            islands.push(Vec::new());
            islands.len() - 1
        });
        islands[i].push(e);
    }
    for i in touched {
        node[i as usize] = u32::MAX;
    }
    let _ = ents;

    stats.islands += islands.len() as u64;
    stats.largest = stats.largest.max(islands.iter().map(Vec::len).max().unwrap_or(0));
    for i in &islands {
        let bucket = match i.len() {
            1 => 0,
            2..=3 => 1,
            4..=10 => 2,
            11..=100 => 3,
            _ => 4,
        };
        stats.sizes[bucket] += 1;
    }
    let effects: Vec<Vec<Effect>> = if cfg.parallel && islands.len() > 1 {
        let pool = bevy_tasks::ComputeTaskPool::get();
        let (level, table, idx, q, marked, moving_now) = (&*level, &*table, &*idx, &q, &marked, &moving_now);
        pool.scope(|s| {
            for i in &islands {
                s.spawn(async move { run_island(i, level, table, idx, q, marked, moving_now) });
            }
        })
    } else {
        islands.iter().map(|i| run_island(i, &level, &table, &idx, &q, &marked, &moving_now)).collect()
    };
    for fx in effects {
        for f in fx {
            match f {
                Effect::Mark(o) => {
                    commands.entity(o).insert(Overlapping);
                }
                Effect::Unmark(e) => {
                    commands.entity(e).remove::<Overlapping>();
                }
                Effect::Moving(o) => {
                    commands.entity(o).insert(Moving);
                }
                Effect::Crowded(e) => {
                    commands.entity(e).insert(Crowded);
                }
                Effect::Fall(e, f, dist) => {
                    if f > 0.0 {
                        queue.add(e, f, Cause::Fall);
                    }
                    commands.queue(move |w: &mut World| {
                        crate::fall_passengers::land_sound(w, e, dist);
                        if f > 0.0 {
                            crate::fall_passengers::propagate(w, e, dist, 1.0);
                        }
                    });
                }
            }
        }
    }
}
