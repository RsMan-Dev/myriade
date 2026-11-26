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
use crate::rules::despawn as rule;
pub use crate::rules::despawn::Removable;
use bevy_ecs::prelude::*;

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
    /// The own-time tick its `noActionTime` count started at (`touch` restarts it): `noActionTime` is now minus this.
    pub idle_since: u64,
}
/// A mob the world must keep (`Mob.persistenceRequired`).
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct Persistent;
/// `Bucketable.setFromBucket`: `AbstractFish.requiresCustomPersistence` keeps the creature from the despawns, but it is not
/// `PersistenceRequired` (not saved as it, not counted by the spawn caps). Saved as `FromBucket`.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct FromBucket;
/// The creature is kept from the despawns: persistent, or from a bucket (`requiresCustomPersistence`).
fn kept(w: &World, e: Entity) -> bool {
    w.get::<Persistent>(e).is_some() || w.get::<FromBucket>(e).is_some()
}
/// `Monster.updateNoActionTime`: a creature counts 3 a tick where the spot is brighter than 0.5 and 1 in the dark (`lit`: always 3,
/// as a blaze or a vex). The count is SAMPLED: read at the deadlines of the ring, each segment since the last sample credited at the
/// rate of the spot the creature stands in then. Only the monsters carry it.
#[derive(Component, Clone, Copy, Debug)]
pub struct LightRated {
    pub lit: bool,
    /// `noActionTime` as of the last sample.
    pub count: u64,
    /// The own-time tick of the last sample.
    pub sampled: u64,
    /// The chance of going is drawn: the deadline is the going.
    pub drawn: bool,
}
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

/// Gives a creature the light count (`lit`: a blaze or a vex) and starts it over now.
pub fn rate_light(w: &mut World, e: Entity, lit: bool) {
    let now = crate::cow::now_of(w, e);
    w.entity_mut(e).insert(LightRated { lit, count: 0, sampled: now, drawn: false });
    arm(w, e);
}
fn restart_light(w: &mut World, e: Entity) {
    let now = crate::cow::now_of(w, e);
    if let Some(mut l) = w.get_mut::<LightRated>(e) {
        l.count = 0;
        l.sampled = now;
        l.drawn = false;
    }
    sample_light(w, e);
}
/// The rate of the spot the creature stands in now (read at a sample, not per tick).
fn light_rate(w: &World, e: Entity, lit: bool) -> u64 {
    if lit {
        return 3;
    }
    let (Some(p), Some(l)) = (w.get::<crate::cow::Pos>(e), w.get_resource::<crate::cow::Level>()) else { return 1 };
    let v = p.0;
    let bright = crate::rules::flee_sun::bright_enough(f64::from(l.mw.0.brightness(v.x.floor() as i32, v.y.floor() as i32, v.z.floor() as i32)));
    if bright { 3 } else { 1 }
}
/// A deadline of a light-counted creature: the segment since the last sample is credited at the rate of the spot now. Below `idle`
/// the next deadline comes as early as the fastest rate reaches it, at most 100 ticks on; at `idle` the chance of going is drawn.
fn sample_light(w: &mut World, e: Entity) {
    let Some(l) = w.get::<LightRated>(e).copied() else { return };
    let (Some(d), now) = (w.get::<Despawn>(e).copied(), crate::cow::now_of(w, e)) else { return };
    let count = l.count + now.saturating_sub(l.sampled) * light_rate(w, e, l.lit);
    let left = u64::from(d.def.idle).saturating_sub(count);
    let at = if left > 0 {
        now + left.div_ceil(3).min(100)
    } else {
        let g = {
            let mut r = w.resource_mut::<DespawnRng>();
            crate::cow::geometric(&mut r.0, d.def.chance)
        };
        now + 1 + u64::try_from(g.max(0)).unwrap_or(0)
    };
    if let Some(mut m) = w.get_mut::<LightRated>(e) {
        m.count = count;
        m.sampled = now;
        m.drawn = left == 0;
    }
    w.get_mut::<Despawn>(e).unwrap().deadline = at;
    crate::cow::arm_at(w, e, at, Due::Despawn);
}
fn inside_ring(w: &World, e: Entity) -> bool {
    w.get::<ActiveUntil>(e).is_some_and(|a| a.0 == u64::MAX)
}
fn removable(w: &World, e: Entity, d: &Despawn) -> bool {
    // A trusting ocelot counts as tamed (`Ocelot.isTrusting`), a cat by its owner (`Cat.isTame`).
    let tamed = w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some()) || w.get::<crate::behaviours::cat_state::Trusting>(e).is_some();
    let named = w.get::<crate::custom_name::CustomName>(e).is_some();
    // `ZombieVillager.removeWhenFarAway`: never while it is being cured.
    let curing = w.get::<crate::give_item::Curing>(e).is_some();
    !curing && rule::removable(d.def.removable, tamed, named, crate::cow::now_of(w, e) - d.born)
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
    w.entity_mut(e).insert(Despawn { def, born, deadline: u64::MAX, idle_since: born });
    let mut r = w.resource_mut::<Radii>();
    if !r.0.contains(&def.distance) {
        r.0.push(def.distance);
    }
    if rule::within_ring(nearest_sqr, def.near) {
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
    if kept(w, e) {
        return;
    }
    if w.get::<LightRated>(e).is_some() {
        return restart_light(w, e);
    }
    let g = {
        let mut r = w.resource_mut::<DespawnRng>();
        crate::cow::geometric(&mut r.0, d.def.chance)
    };
    let at = rule::deadline(crate::cow::now_of(w, e), d.def.idle, g);
    w.get_mut::<Despawn>(e).unwrap().deadline = at;
    crate::cow::arm_at(w, e, at, Due::Despawn);
}
/// `noActionTime = 0` (a creature hurt, as `ai::after_hurt` does): the count starts again, unless a player is within the ring.
pub fn touch(w: &mut World, e: Entity) {
    let now = crate::cow::now_of(w, e);
    if let Some(mut d) = w.get_mut::<Despawn>(e) {
        d.idle_since = now;
    }
    if w.get::<Despawn>(e).is_some() && !inside_ring(w, e) {
        arm(w, e);
    }
}
/// `noActionTime` of a creature with a rule, in its own ticks (saturated); None without one.
/// Inside a ring the count is 0 (`checkDespawn` resets it while a player is within 32 blocks).
pub fn idle_for(w: &World, e: Entity) -> Option<i32> {
    let d = w.get::<Despawn>(e)?;
    if inside_ring(w, e) {
        return Some(0);
    }
    if let Some(l) = w.get::<LightRated>(e) {
        return Some(i32::try_from(l.count).unwrap_or(i32::MAX));
    }
    Some(i32::try_from(crate::cow::now_of(w, e).saturating_sub(d.idle_since)).unwrap_or(i32::MAX))
}
/// Sets `noActionTime` to `ticks` (a creature with a rule only): the count started `ticks` ago, and the chance of going is drawn from
/// that start, as `arm` draws it from now. Nothing is counted per tick.
pub fn set_idle(w: &mut World, e: Entity, ticks: i32) {
    let Some(d) = w.get::<Despawn>(e).copied() else { return };
    if w.get::<LightRated>(e).is_some() {
        let now = crate::cow::now_of(w, e);
        if let Some(mut l) = w.get_mut::<LightRated>(e) {
            l.count = u64::try_from(ticks.max(0)).unwrap_or(0);
            l.sampled = now;
            l.drawn = false;
        }
        if !kept(w, e) && !inside_ring(w, e) {
            sample_light(w, e);
        }
        return;
    }
    let since = crate::cow::now_of(w, e).saturating_sub(u64::try_from(ticks.max(0)).unwrap_or(0));
    if let Some(mut m) = w.get_mut::<Despawn>(e) {
        m.idle_since = since;
    }
    if kept(w, e) || inside_ring(w, e) {
        return;
    }
    let g = {
        let mut r = w.resource_mut::<DespawnRng>();
        crate::cow::geometric(&mut r.0, d.def.chance)
    };
    let at = rule::deadline(since, d.def.idle, g);
    w.get_mut::<Despawn>(e).unwrap().deadline = at;
    crate::cow::arm_at(w, e, at, Due::Despawn);
}
/// `setPersistenceRequired` (named, tamed, picked something up): it never goes, and the spawn caps no longer count it.
pub fn keep(w: &mut World, e: Entity) {
    if let Some(mut d) = w.get_mut::<Despawn>(e) {
        d.deadline = u64::MAX;
    }
    w.entity_mut(e).insert(Persistent);
    crate::spawning::uncount(w, e);
    w.entity_mut(e).remove::<crate::spawn_cost::ChargeBearer>();
}
pub fn release(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Persistent>();
    crate::spawn_cost::count(w, e);
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
    if d.deadline == u64::MAX || now < d.deadline || kept(w, e) || inside_ring(w, e) {
        return;
    }
    if w.get::<LightRated>(e).is_some_and(|l| !l.drawn) {
        return sample_light(w, e);
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
    if kept(w, e) || w.get::<crate::zone::Dormant>(e).is_some() || !removable(w, e, &d) {
        return;
    }
    let Some(b) = w.get::<Bounds>(e) else { return };
    let (mn, mx) = (b.0.min_position(), b.0.max_position());
    let (x, z) = ((mn.x + mx.x) / 2.0, (mn.z + mx.z) / 2.0);
    w.resource_mut::<DespawnStats>().measured += 1;
    if rule::is_far(x, z, &w.resource::<Players>().list, d.def.distance) {
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
                    // Left the ring: the count starts again from the leave (`serverAiStep` counts up from 0).
                    let now = crate::cow::now_of(w, e);
                    if let Some(mut d) = w.get_mut::<Despawn>(e) {
                        d.idle_since = now;
                    }
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
