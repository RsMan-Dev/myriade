//! `Mob.checkDespawn` by event. Horizon asks every ticking mob, every tick, for its distance to the nearest player: past 128 blocks
//! (64 for a water ambient) it goes at once, between 32 and that it goes with one chance in 800 per tick once its `noActionTime` is past
//! 600 (it counts up each tick of the mob and goes back to 0 whenever a player is within 32 blocks, or it is hurt), and a persistent
//! mob (named, tamed, holding what it picked up, leashed...) never goes. Here a creature asks nothing:
//!  - far: the framework knows when a player moves (`Players` old and new positions) or a creature changes column or wakes; only the
//!    creatures in the crescent a step sweeps around the old position of a moved player, and the ones that moved or woke, are measured;
//!  - between: the 32-block ring of `player_near` already tells when the creature leaves it (`LeftRing`); from then the chance of each
//!    tick is a DEADLINE drawn in advance (the first success of a 1-in-800 draw each tick, `cow::geometric`: the same law), in the entity's
//!    own time (a sleeping chunk adds nothing, as `noActionTime` does not count there), and a creature that enters the ring again lets
//!    the deadline pass unused (it is checked when due);
//!  - persistence is a component (`Persistent`): `keep` / `release`.
use crate::cow::{ActiveUntil, Bounds, Cell, Due, Index, Players, Rng};
use bevy_ecs::prelude::*;

/// Whether a creature that no player is near may go. The rule of its species, not of the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removable {
    Always,
    /// Animals, villagers, golems...
    Never,
    /// `Cat.removeWhenFarAway`: while untamed and older than this many ticks.
    UntamedAfter(u32),
}
#[derive(Clone, Copy, Debug)]
pub struct DespawnDef {
    /// Blocks past which it goes at once (`MobCategory.getDespawnDistance`).
    pub distance: f32,
    pub removable: Removable,
    /// The ring that resets `noActionTime`, the ticks it must count to, the chance (1 in `chance` per tick) once it has.
    pub near: f32,
    pub idle: u32,
    pub chance: i32,
}
impl DespawnDef {
    pub const MOB: DespawnDef = DespawnDef { distance: 128.0, removable: Removable::Always, near: 32.0, idle: 600, chance: 800 };
    pub const KEPT: DespawnDef = DespawnDef { removable: Removable::Never, ..Self::MOB };
}

/// What a creature that may go away carries.
#[derive(Component, Clone, Copy, Debug)]
pub struct Despawn {
    pub def: DespawnDef,
    /// Its own time when it was created (an age).
    pub born: u64,
    /// The own-time tick from which the chance of going has come up (`u64::MAX`: none armed).
    pub deadline: u64,
}
/// A mob the world must keep (`Mob.persistenceRequired`).
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct Persistent;
/// The creatures that left the 32-block ring of a player since the last step (`cow::player_near` writes it).
#[derive(Resource, Default)]
pub struct LeftRing(pub Vec<Entity>);
#[derive(Resource)]
pub struct DespawnRng(pub Rng);
impl Default for DespawnRng {
    fn default() -> Self {
        Self(Rng(0x2545_F491_4F6C_DD1D))
    }
}
/// The distances at which some creature goes at once.
#[derive(Resource, Default)]
pub struct Radii(pub Vec<f32>);
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct DespawnStats {
    pub far: u64,
    pub idle: u64,
    pub measured: u64,
}

fn inside_ring(w: &World, e: Entity) -> bool {
    w.get::<ActiveUntil>(e).is_some_and(|a| a.0 == u64::MAX)
}
fn removable(w: &World, e: Entity, d: &Despawn) -> bool {
    match d.def.removable {
        Removable::Always => true,
        Removable::Never => false,
        Removable::UntamedAfter(t) => !w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some()) && crate::cow::now_of(w, e) - d.born > u64::from(t),
    }
}
/// The resources of the service, made when the first creature that may go away is given its rule (a world that has none does not carry them).
pub fn ensure(w: &mut World) {
    if !w.contains_resource::<DespawnRng>() {
        w.insert_resource(LeftRing::default());
        w.insert_resource(DespawnRng::default());
        w.insert_resource(Radii::default());
        w.insert_resource(DespawnStats::default());
    }
}
/// Gives a new creature its rule; `nearest_sqr` is its distance to the nearest player (squared; infinity without one).
pub fn admit(w: &mut World, e: Entity, def: DespawnDef, nearest_sqr: f64) {
    ensure(w);
    let born = crate::cow::now_of(w, e);
    w.entity_mut(e).insert(Despawn { def, born, deadline: u64::MAX });
    let mut r = w.resource_mut::<Radii>();
    if !r.0.contains(&def.distance) {
        r.0.push(def.distance);
    }
    if nearest_sqr < f64::from(def.near).powi(2) {
        // A player is within the ring at its birth: active without end until the ring is left.
        if let Some(mut a) = w.get_mut::<ActiveUntil>(e) {
            a.0 = u64::MAX;
        }
    } else {
        arm(w, e);
    }
    // Born past its distance (a mod's spawn, a creature loaded far away): the first measure decides, as the first check of Horizon's tick.
    far_check(w, e);
}
/// The chance of going comes up `idle + 1` ticks from now, then the first success of the per-tick draw.
fn arm(w: &mut World, e: Entity) {
    let Some(d) = w.get::<Despawn>(e).copied() else { return };
    if w.get::<Persistent>(e).is_some() {
        return;
    }
    let g = {
        let mut r = w.resource_mut::<DespawnRng>();
        crate::cow::geometric(&mut r.0, d.def.chance)
    };
    let at = crate::cow::now_of(w, e) + u64::from(d.def.idle) + 1 + g.max(0) as u64;
    w.get_mut::<Despawn>(e).unwrap().deadline = at;
    crate::cow::arm_at(w, e, at, Due::Despawn);
}
/// `noActionTime = 0` (a creature hurt, as `ai::after_hurt` does): the count starts again, unless a player is within the ring.
pub fn touch(w: &mut World, e: Entity) {
    if w.get::<Despawn>(e).is_some() && !inside_ring(w, e) {
        arm(w, e);
    }
}
/// `setPersistenceRequired` (named, tamed, picked something up): it never goes, and the spawn caps no longer count it.
pub fn keep(w: &mut World, e: Entity) {
    if let Some(mut d) = w.get_mut::<Despawn>(e) {
        d.deadline = u64::MAX;
    }
    w.entity_mut(e).insert(Persistent);
    crate::spawning::uncount(w, e);
}
pub fn release(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Persistent>();
    crate::spawning::recount(w, e);
    if !inside_ring(w, e) {
        arm(w, e);
    }
}
/// The creature goes (it is not a death: no loot).
pub fn remove(w: &mut World, e: Entity) {
    if let Some(c) = w.get::<Cell>(e).map(|c| c.0)
        && c != u32::MAX
    {
        w.resource_mut::<Index>().remove(e, c);
    }
    if let Ok(m) = w.get_entity_mut(e) {
        m.despawn();
    }
}
/// The deadline of the wheel: the chance of going has come up, unless a player is within the ring again, the creature is kept, or
/// it is not yet one that may go (a young cat): then the draw starts again.
pub fn due(w: &mut World, e: Entity) {
    let Some(d) = w.get::<Despawn>(e).copied() else { return };
    let now = crate::cow::now_of(w, e);
    if d.deadline == u64::MAX || now < d.deadline || w.get::<Persistent>(e).is_some() || inside_ring(w, e) {
        return;
    }
    if !removable(w, e, &d) {
        if d.def.removable == Removable::Never {
            return;
        }
        arm(w, e);
        return;
    }
    w.get_mut::<Despawn>(e).unwrap().deadline = u64::MAX;
    w.resource_mut::<DespawnStats>().idle += 1;
    remove(w, e);
}
/// A creature measured against the nearest player: past its distance it goes at once. Without any player, it stays.
fn far_check(w: &mut World, e: Entity) {
    let Some(d) = w.get::<Despawn>(e).copied() else { return };
    if w.get::<Persistent>(e).is_some() || w.get::<crate::zone::Dormant>(e).is_some() || !removable(w, e, &d) {
        return;
    }
    let Some(b) = w.get::<Bounds>(e) else { return };
    let (mn, mx) = (b.0.min_position(), b.0.max_position());
    let (x, z) = ((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
    w.resource_mut::<DespawnStats>().measured += 1;
    let list = &w.resource::<Players>().list;
    let Some(best) = list.iter().map(|p| (p.0 - x).powi(2) + (p.1 - z).powi(2)).reduce(f64::min) else { return };
    if best > f64::from(d.def.distance).powi(2) {
        w.resource_mut::<DespawnStats>().far += 1;
        remove(w, e);
    }
}
/// A creature wakes (its chunk ticks again): it is checked at once, as Horizon's first tick of it does.
pub fn on_wake(w: &mut World, e: Entity) {
    if w.get::<Despawn>(e).is_some() {
        far_check(w, e);
    }
}
/// Before `player_near`: the creatures that left a ring get their deadline, and the ones the distance may have changed for are measured.
pub fn step(
    left: Option<ResMut<LeftRing>>,
    players: Res<Players>,
    idx: Res<Index>,
    radii: Option<Res<Radii>>,
    has: Query<(), With<Despawn>>,
    mut commands: Commands,
) {
    let (Some(mut left), Some(radii)) = (left, radii) else { return };
    if left.0.is_empty() && !players.dirty && idx.crossed.is_empty() {
        return;
    }
    for e in left.0.drain(..) {
        if has.contains(e) {
            commands.queue(move |w: &mut World| {
                if !inside_ring(w, e) {
                    arm(w, e);
                }
            });
        }
    }
    if radii.0.is_empty() {
        return;
    }
    let mut cand: Vec<Entity> = Vec::new();
    if players.dirty {
        for i in 0..players.list.len().max(players.prev.len()) {
            let (n, o) = (players.list.get(i).copied(), players.prev.get(i).copied());
            // A player who arrived makes nothing farther; one who moved or left may.
            let Some(o) = o else { continue };
            if n == Some(o) {
                continue;
            }
            for &r in &radii.0 {
                idx.crescent(o, n, f64::from(r), |e| cand.push(e));
            }
        }
    }
    if !players.list.is_empty() {
        cand.extend(idx.crossed.iter().map(|c| c.0));
    }
    cand.sort_unstable();
    cand.dedup();
    for e in cand {
        if has.contains(e) {
            commands.queue(move |w: &mut World| far_check(w, e));
        }
    }
}
