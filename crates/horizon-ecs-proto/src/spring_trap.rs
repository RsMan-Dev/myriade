//! A mount that waits for a player and springs a trap (`SkeletonHorse.isTrap`, `SkeletonTrapGoal`, `ServerLevel.tickThunder`): armed, it holds
//! the state `Armed` and a deadline on the wheel (`lifetime` ticks of its own time, then it goes); a living player within `radius` blocks springs
//! it: a bolt that burns nothing, the mount is tamed and ridden, and `extra` more mounts appear, each ridden. Nothing runs per tick: the player
//! events of the bridge (`player_changed`), the arming and the wake-up of a sleeping mount are the only checks, and only the armed ones are asked.
use crate::cow::{after, now_of, Pos, Rng, SpeciesTable, Vel};
use crate::goals::Online;
use crate::rules::{no_attrs, no_interact, params, Rule};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};

/// How a species springs a trap.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `SkeletonTrapGoal.canUse`: a living player this close springs it.
    pub radius: f64,
    /// The ticks an armed one waits (`SkeletonHorse.customServerAiStep`: 15 minutes), counted in its own time while it is not kept.
    pub lifetime: i32,
    /// The mount of the other riders and the species that rides each mount.
    pub mount: &'static str,
    pub rider: &'static str,
    /// The mounts that appear besides the sprung one.
    pub extra: u8,
    /// `LIGHTNING_INVULNERABLE_TICKS`: the invulnerability the riders and the new mounts start with.
    pub invulnerable: u64,
    /// The helmet a rider gets when it wears none.
    pub helmet: &'static str,
    /// The deviation of the push that scatters a new mount.
    pub scatter: f64,
}
impl Params {
    pub const SKELETON_TRAP: Self = Self { radius: 10.0, lifetime: 18000, mount: crate::species::lists::SKELETON_HORSE, rider: crate::species::lists::SKELETON, extra: 3, invulnerable: 60, helmet: "minecraft:iron_helmet", scatter: 1.1485 };
}

/// The species can be armed (set at its creation by its rule).
#[derive(Component, Clone, Copy, Debug)]
pub struct Springs(pub &'static Params);
/// The mount waits for a player: the tick of its own time its wait began at (`trapTime` is now minus this).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Armed {
    pub since: u64,
}
/// A `SkeletonTrapTime` read before its `SkeletonTrap`: used when the mount is armed.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct LoadedTime(pub i32);
/// `AbstractHorse.setTamed(true)` without an owner.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Tamed;
/// The armed mounts (a stale entry is dropped when read).
#[derive(Resource, Default)]
pub struct Traps(Vec<Entity>);

/// The rule `spring_trap`, parameters `Params`.
pub static RULE: Rule = Rule {
    name: "spring_trap",
    equip: |w, e, p| {
        w.entity_mut(e).insert(Springs(params::<Params>(p)));
    },
    attrs: no_attrs,
    interact: no_interact,
};

/// `setTrap(true)`: `e` waits for a player, `elapsed` ticks already counted. False when its species springs no trap.
pub fn arm(w: &mut World, e: Entity, elapsed: i32) -> bool {
    let Some(p) = w.get::<Springs>(e).map(|s| s.0) else { return false };
    let since = now_of(w, e).saturating_sub(elapsed.max(0) as u64);
    w.entity_mut(e).insert(Armed { since }).remove::<LoadedTime>();
    w.get_resource_or_insert_with(Traps::default).0.push(e);
    after(w, e, p.lifetime - elapsed.max(0), expire);
    check(w, e);
    true
}
/// `setTrap(false)`.
pub fn disarm(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<(Armed, LoadedTime)>();
}
/// The ticks `e` has waited (`trapTime`); None when it is not armed.
pub fn elapsed(w: &World, e: Entity) -> Option<i32> {
    let a = w.get::<Armed>(e)?;
    Some(now_of(w, e).saturating_sub(a.since).min(i32::MAX as u64) as i32)
}
/// `SkeletonTrapTime` loaded: moves the start of the wait of an armed mount (its deadline follows), or is kept for its arming.
pub fn set_elapsed(w: &mut World, e: Entity, ticks: i32) {
    let Some(p) = w.get::<Springs>(e).map(|s| s.0) else { return };
    if w.get::<Armed>(e).is_none() {
        w.entity_mut(e).insert(LoadedTime(ticks));
        return;
    }
    let since = now_of(w, e).saturating_sub(ticks.max(0) as u64);
    w.get_mut::<Armed>(e).unwrap().since = since;
    after(w, e, p.lifetime - ticks.max(0), expire);
}
/// The time a mount was loaded with, when it is armed after it.
pub fn loaded_time(w: &World, e: Entity) -> i32 {
    w.get::<LoadedTime>(e).map_or(0, |t| t.0)
}

/// The wait is over: an armed mount that is not kept goes (`discard`); a kept one does not count, it waits another life.
fn expire(w: &mut World, e: Entity) {
    let (Some(p), Some(a)) = (w.get::<Springs>(e).map(|s| s.0), w.get::<Armed>(e).copied()) else { return };
    let now = now_of(w, e);
    if now < a.since + p.lifetime as u64 {
        return;
    }
    // A kept one stays armed and keeps counting (`trapTime` goes on growing); nothing re-arms it and nothing runs per tick.
    if w.get::<crate::despawn::Persistent>(e).is_some() {
        return;
    }
    crate::despawn::remove(w, e);
}

/// A player came, moved or changed (the bridge's `set_player`): the armed mounts look for it. Costs nothing while none is armed.
pub fn player_changed(w: &mut World) {
    let Some(mut list) = w.get_resource_mut::<Traps>().map(|mut t| std::mem::take(&mut t.0)) else { return };
    list.retain(|&e| w.get::<Armed>(e).is_some());
    list.dedup();
    for &e in &list.clone() {
        check(w, e);
    }
    let sprung = |w: &World, e: &Entity| w.get::<Armed>(*e).is_some();
    list.retain(|e| sprung(w, e));
    w.resource_mut::<Traps>().0.extend(list);
}
/// A mount wakes (its chunk ticks again): a player may have come while it slept.
pub fn on_wake(w: &mut World, e: Entity) {
    if w.get::<Armed>(e).is_some() {
        check(w, e);
    }
}

/// `Level.hasNearbyAlivePlayer`: a living player that is not a spectator within `radius` of the mount.
fn check(w: &mut World, e: Entity) {
    let (Some(p), Some(at)) = (w.get::<Springs>(e).map(|s| s.0), w.get::<Pos>(e).map(|p| p.0)) else { return };
    if w.get::<Armed>(e).is_none() || w.get::<crate::zone::Dormant>(e).is_some() {
        return;
    }
    let state = w.get_resource::<crate::hostile::PlayerState>();
    let near = w.get_resource::<Online>().is_some_and(|o| o.nearest_where(at.x, at.y, at.z, p.radius, |pl, d| d < p.radius * p.radius && state.is_none_or(|s| s.alive(pl.id) && !s.spectator(pl.id))).is_some());
    if near {
        spring(w, e, p);
    }
}

fn seed_of(w: &mut World, e: Entity) -> i64 {
    let Some(mut r) = w.get_mut::<Rng>(e) else { return 0 };
    ((u64::from(r.next_u32()) << 32) | u64::from(r.next_u32())) as i64
}

/// `SkeletonTrapGoal.tick`: the bolt, the mount tamed and ridden, the other mounts each ridden.
fn spring(w: &mut World, e: Entity, p: &'static Params) {
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return };
    disarm(w, e);
    w.entity_mut(e).insert(Tamed);
    crate::lightning_tick::strike(w, at, true, None);
    let mut r = LegacyRandom::new(seed_of(w, e));
    let special = crate::effects::special_difficulty(w, e);
    ride_with(w, e, p, at, special, &mut r);
    for _ in 0..p.extra {
        let seed = seed_of(w, e);
        let Some(horse) = spawn(w, p.mount, at, seed) else { continue };
        w.entity_mut(horse).insert(Tamed);
        guard(w, horse, p);
        ride_with(w, horse, p, at, special, &mut r);
        let (dx, dz) = (r.triangle(0.0, p.scatter), r.triangle(0.0, p.scatter));
        if let Some(mut v) = w.get_mut::<Vel>(horse) {
            v.0 = Vec3::new(v.0.x + dx, v.0.y, v.0.z + dz);
        }
    }
}
/// `SkeletonTrapGoal.createSkeleton` and `startRiding`: a rider with a helmet and enchanted gear takes the seat of `mount`.
fn ride_with(w: &mut World, mount: Entity, p: &'static Params, at: Vec3, special: f32, r: &mut LegacyRandom) -> Option<Entity> {
    use horizon_items::equipment::{HEAD, MAINHAND};
    let rider = spawn(w, p.rider, at, r.next_long())?;
    guard(w, rider, p);
    if crate::equip_stack::get(w, rider, HEAD).is_none() {
        crate::equip_stack::set(w, rider, HEAD, Some(myriade_engine_lib::items::ItemStack::new(p.helmet, 1)));
    }
    for slot in [MAINHAND, HEAD] {
        enchant(w, rider, slot, special, r);
    }
    crate::mount::ride(w, mount, Who::Entity(rider)).then_some(rider)
}
/// `EnchantmentHelper.enchantItem` on the stack held in `slot` (nothing is drawn for an empty slot): the cost is drawn up to the difficulty's top.
fn enchant(w: &mut World, e: Entity, slot: usize, special: f32, r: &mut LegacyRandom) {
    let Some(stack) = crate::equip_stack::get(w, e, slot) else { return };
    let Some((items, ench)) = w.get_resource::<crate::behaviours::trade_offers::TradeData>().map(|d| (d.items, d.enchantments)) else { return };
    let cost = r.next_int_between_inclusive(5, crate::rules::equipment_enchant::cost_top(special)).unwrap_or(5);
    let picked = ench.select(items, r, &stack, cost, ench.tag("minecraft:on_mob_spawn_equipment"));
    let mut out = stack;
    for (enchantment, level) in picked {
        out.enchant(&enchantment, level);
    }
    crate::equip_stack::set(w, e, slot, Some(out));
}
/// `Entity.invulnerableTime = 60`.
fn guard(w: &mut World, e: Entity, p: &Params) {
    let until = now_of(w, e) + p.invulnerable;
    w.entity_mut(e).insert(crate::cow::InvulnerableUntil(until));
}
/// A creature of `kind` at `at`, finalized for `Triggered` (`EntityType.create` then `finalizeSpawn`), in the index and told to the host.
fn spawn(w: &mut World, kind: &str, at: Vec3, seed: i64) -> Option<Entity> {
    use crate::spawn_reason::SpawnReason::Triggered;
    let sp = w.get_resource::<SpeciesTable>()?.0.iter().find(|s| s.name == kind).copied()?;
    let e = crate::cow::spawn_armed(w, &sp, at.x, at.z, seed as u64);
    w.get_mut::<Pos>(e)?.0.y = at.y;
    let special = crate::effects::special_difficulty(w, e);
    let class =crate::spawn_finalize::kind_of(w, e);
    crate::spawn_finalize::finalize(w, e, class, None, Triggered, special, &mut LegacyRandom::new(seed));
    crate::zone::admit(w, &[e]);
    crate::bridge::view::mark(w, e);
    crate::bridge::host_events_more::said_spawned(w, e, false, Triggered);
    Some(e)
}

impl crate::bridge::host::EcsWorld {
    /// `SkeletonHorse.setTrap`: arms (or disarms) the mount `e`; false when its species springs no trap.
    pub fn set_trap(&mut self, e: Entity, on: bool) -> bool {
        if on {
            arm(self.world_mut(), e, 0)
        } else {
            disarm(self.world_mut(), e);
            true
        }
    }
}
