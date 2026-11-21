//! The projectile capability: an entity that has a body (a position and a velocity) and a `Projectile`, and NOTHING else: no
//! life, no goals, no box in the spatial index (a projectile is not hit by another). It is the first entity of the frame that
//! is not alive. The flight is `AbstractArrow.tick` for an arrow (gravity, inertia, the first block on the path, the nearest
//! creature or player on it, sticking in the block, a life of 1 200 ticks on the ground); a second kind (a snowball) differs only by
//! its `Flight` data. Only a projectile in flight (`Flying`) is visited each tick; a stuck one sleeps until its
//! expiry (a deadline of the wheel) or a block change on the block it is in.
use crate::cow::*;
use crate::goals::{Events, Online};
use crate::hostile::{ChangedBlocks, PlayerState};
use crate::targeting::{player_box, Who};
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use horizon_client_lib::arrows::{back_off, clip_block_hit};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::collections::HashMap;

/// What differs between the kinds of projectile: how they fall, slow down, end and hurt.
#[derive(Component, Clone, Copy, Debug)]
pub struct Flight {
    /// The identifier the kind is saved under (`minecraft:arrow`...); a projectile carries no `TypeName`, so this is its id.
    pub id: &'static str,
    /// The pull per tick (the arrow 0.05, the snowball 0.03), the factor of the velocity per tick (0.99).
    pub gravity: f64,
    pub inertia: f64,
    /// A block it hits keeps it (an arrow); else it is gone (a snowball).
    pub sticks: bool,
    pub damage: Damage,
    /// The size of its box (an arrow 0.5 x 0.5; a thrown item 0.25).
    pub size: f64,
    /// The order of the tick: an arrow moves, then slows and falls; a thrown item falls and slows, then moves.
    pub order: Order,
    /// `AbstractHurtingProjectile.accelerationPower` (a fireball 0.1): the push along its velocity each tick, before the inertia.
    pub accel: f64,
    /// The age at which it is gone in the air (the rest 6 000). Not used by the `Order::Hurting` flights, which have none.
    pub max_age: u32,
    /// What it does where it ends, after the damage.
    pub hit: OnHit,
    /// `WitherSkull.onHitEntity`: a creature it hits takes wither II for 10 s on normal, 40 s on hard, none on easy.
    pub wither: bool,
    /// `LlamaSpit.tick`: gone once its box holds no air (it is inside blocks), and where it is in water.
    pub spit_end: bool,
    /// `ShulkerBullet`: its end (a creature hit, a block) plays its sound and particles (`shulker_bullet`).
    pub bullet: bool,
    /// A blue (dangerous) wither skull: its blast caps the blocks' resistance at 0.8 (`explode_wither`).
    pub blue: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    MoveThenDrag,
    DragThenMove,
    /// `AbstractHurtingProjectile`: the acceleration and the inertia, then the move; no gravity.
    Hurting,
}
/// What a projectile does where it ends (the damage is `Flight.damage`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OnHit {
    Nothing,
    /// The creature it hits burns for this many ticks (`igniteForSeconds`).
    Ignite(u64),
    /// The creature it hits takes this effect (a tipped arrow): kind, level, ticks.
    Effect(crate::effects::Kind, i32, i32),
    /// An explosion of this power where it ends, on a creature or a block; with fire or not (`LargeFireball.onHit`). The frame has no
    /// explosion yet: `Events.exploded` says it, the damage in the radius and the blocks are the explosion capability's.
    Explode { power: i32, fire: bool },
    /// A thrown potion breaks and splashes these brews (`effects::splash`).
    Splash(&'static [crate::effects::Brew]),
    /// A thrown potion whose brews are its `PotionId`'s in `effects::POTIONS`, splashed where it ends.
    Potion,
    /// A lingering potion breaks: an area cloud of its `PotionId` where it ends (`ThrownLingeringPotion.onHit`).
    LingeringPotion,
    /// A wind charge bursts where it ends: knockback only, no damage, on the creatures within `radius` (`explosion::wind_burst`).
    WindBurst { radius: f32 },
    /// A dragon's fireball ends: the breath cloud (`dragon::fireball::burst`).
    DragonBurst,
    /// `ThrownEgg`: a hatchling of species `into` with chance 1 in `one_in`, then `many` of them with chance 1 in `many_in`; babies that grow in `baby` ticks.
    Hatch { into: &'static str, one_in: i32, many_in: i32, many: u32, baby: u64 },
    /// `ThrownEnderpearl`: its owner goes to where the pearl started its last tick; a player takes `damage` and may bring an endermite (chance `endermite`).
    Teleport { endermite: f32, damage: f32 },
    /// `ThrownExperienceBottle`: `base + nextInt(bound) + nextInt(bound)` experience where it breaks.
    Orbs { base: i32, bound: i32 },
}
impl OnHit {
    /// The ends that `projectile_more::ends` applies.
    fn thrown(self) -> bool {
        matches!(self, Self::Hatch { .. } | Self::Teleport { .. } | Self::Orbs { .. })
    }
}
/// An explosion asked by a projectile.
#[derive(Clone, Copy, Debug)]
pub struct Blast {
    pub at: Vec3,
    pub radius: f32,
    pub fire: bool,
    pub owner: Option<Who>,
    /// The exploding creature itself (spared by its own blast).
    pub source: Option<Entity>,
}
#[derive(Clone, Copy, Debug)]
pub enum Damage {
    /// `ceil(speed * base)` (`AbstractArrow.onHitEntity`, a non-critical arrow).
    BySpeed(f64),
    /// A fixed amount (a snowball hurts for 0, a blaze's for 3).
    Fixed(f32),
    /// A fixed amount that only a living owner deals (`LlamaSpit.onHitEntity`): without one it does nothing.
    Owned(f32),
}
/// `0.99F`, the way vanilla multiplies the velocity of a projectile.
const INERTIA: f64 = 0.99_f32 as f64;
impl Flight {
    const BASE: Flight = Flight { id: "", gravity: 0.0, inertia: INERTIA, sticks: false, damage: Damage::Fixed(0.0), size: 0.25, order: Order::MoveThenDrag, accel: 0.0, max_age: 6000, hit: OnHit::Nothing, wither: false, spit_end: false, bullet: false, blue: false };
    pub const ARROW: Flight = Flight { id: "minecraft:arrow", gravity: 0.05, sticks: true, damage: Damage::BySpeed(2.0), size: 0.5, ..Self::BASE };
    /// `SmallFireball`: 5 damage and fire for 5 seconds; no explosion.
    pub const SMALL_FIREBALL: Flight = Flight { id: "minecraft:small_fireball", inertia: 0.95_f32 as f64, damage: Damage::Fixed(5.0), size: 0.3125, order: Order::Hurting, accel: 0.1, max_age: 1200, hit: OnHit::Ignite(100), ..Self::BASE };
    /// `LargeFireball` of the explosion power `power`: 6 damage, then the explosion (with fire).
    pub const fn large_fireball(power: i32) -> Flight {
        Flight { id: "minecraft:fireball", inertia: 0.95_f32 as f64, damage: Damage::Fixed(6.0), size: 1.0, order: Order::Hurting, accel: 0.1, max_age: 1200, hit: OnHit::Explode { power, fire: true }, ..Self::BASE }
    }
    /// `LlamaSpit`: 1 damage from a living owner, gravity 0.06; gone once its box is inside blocks, or in water.
    pub const SPIT: Flight = Flight { id: "minecraft:llama_spit", gravity: 0.06, damage: Damage::Owned(1.0), spit_end: true, ..Self::BASE };
    /// A thrown splash potion: gravity 0.05, no damage. Not done: what it splashes (the effects of its stack) where it breaks.
    /// `BreezeWindCharge`: straight flight at constant speed (inertia 1, no acceleration), one point of damage, a burst of radius 3.
    pub const BREEZE_WIND_CHARGE: Flight = Flight { id: "minecraft:breeze_wind_charge", inertia: 1.0, damage: Damage::Fixed(1.0), size: 0.3125, order: Order::Hurting, accel: 0.0, hit: OnHit::WindBurst { radius: 3.0 }, ..Self::BASE };
    /// The thrown potions are saved under their item's id (`entity_nbt.rs`, `throwable_to_nbt`): `minecraft:splash_potion`, `minecraft:lingering_potion`.
    pub const POTION: Flight = Flight { id: "minecraft:splash_potion", gravity: 0.05, order: Order::DragThenMove, hit: OnHit::Potion, ..Self::BASE };
    pub const LINGERING_POTION: Flight = Flight { id: "minecraft:lingering_potion", gravity: 0.05, order: Order::DragThenMove, hit: OnHit::LingeringPotion, ..Self::BASE };
    /// `ThrownTrident` as a mob throws it: gravity 0.05, `ceil(speed * 8)`; no pierce, no return (the item and the loyalty are not modelled).
    pub const TRIDENT: Flight = Flight { id: crate::species::lists::TRIDENT, gravity: 0.05, sticks: true, damage: Damage::BySpeed(8.0), size: 0.5, ..Self::BASE };
    /// `WitherSkull` (not dangerous): inertia 0.95, 8 damage, the wither effect on a hit, then a blast of power 1.
    pub const WITHER_SKULL: Flight = Self::wither_skull(false);
    pub const SNOWBALL: Flight = Flight { id: "minecraft:snowball", gravity: 0.03, order: Order::DragThenMove, ..Self::BASE };
    /// `WitherSkull`: a blue (dangerous) skull keeps 0.73 of its speed a tick, the others 0.95; 8 damage, then a power 1 blast without fire.
    pub const fn wither_skull(dangerous: bool) -> Flight {
        let inertia = if dangerous { 0.73_f32 as f64 } else { 0.95_f32 as f64 };
        Flight { id: "minecraft:wither_skull", inertia, damage: Damage::Fixed(8.0), size: 0.3125, order: Order::Hurting, accel: 0.1, max_age: 1200, hit: OnHit::Explode { power: 1, fire: false }, wither: true, blue: dangerous, ..Self::BASE }
    }
}
/// The identifiers of `KINDS` declared by a recipe of their own, not by `build`: a thrown trident (`trident.rs`).
pub const UNSAVED: &[&str] = &[crate::species::lists::TRIDENT];
/// Every projectile identifier with the `Flight` it is made from: `build` launches the kind of its saved `id`. A fireball comes back with
/// the explosion power its `ExplosionPower` field saves (1 when absent).
pub const KINDS: &[(&str, Flight)] = &[
    ("minecraft:arrow", Flight::ARROW),
    ("minecraft:small_fireball", Flight::SMALL_FIREBALL),
    ("minecraft:fireball", Flight::large_fireball(1)),
    ("minecraft:wither_skull", Flight::WITHER_SKULL),
    ("minecraft:breeze_wind_charge", Flight::BREEZE_WIND_CHARGE),
    ("minecraft:splash_potion", Flight::POTION),
    ("minecraft:lingering_potion", Flight::LINGERING_POTION),
    ("minecraft:snowball", Flight::SNOWBALL),
    (crate::species::lists::TRIDENT, Flight::TRIDENT),
    ("minecraft:llama_spit", Flight::SPIT),
    (crate::species::lists::SHULKER_BULLET, crate::species::shulker::SHOT.flight),
    (crate::species::lists::EGG, crate::projectile_more::EGG),
    (crate::species::lists::ENDER_PEARL, crate::projectile_more::ENDER_PEARL),
    (crate::species::lists::EXPERIENCE_BOTTLE, crate::projectile_more::EXPERIENCE_BOTTLE),
    (crate::species::lists::DRAGON_FIREBALL, crate::projectile_more::DRAGON_FIREBALL),
    (crate::species::lists::WIND_CHARGE, crate::projectile_more::WIND_CHARGE),
];
/// The six steps a projectile or a path takes from a block, by direction: down, up, north, south, west, east (the 3D data order).
pub const STEPS: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
/// The projectile: whose it is, its age, whether it has left its owner's box, the ticks it spent stuck.
#[derive(Component, Clone, Copy, Debug)]
pub struct Projectile {
    pub owner: Option<Who>,
    pub age: u32,
    pub left_owner: bool,
    /// The tick it expires at, while stuck.
    pub expires: u64,
}
/// In flight: the flight system visits it every tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Flying;
/// Stuck in a block: it sleeps; `block` is the block it hit.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Stuck {
    pub block: [i32; 3],
}
/// An entity that shot a `Order::Hurting` projectile: when it is removed, the ones it shot still in flight go with it (an owner that is
/// removed, `AbstractHurtingProjectile.tick`). Put on the shooter by `launch`; an entity that never shot has none.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_remove = shooter_removed)]
pub struct Shooter;
fn shooter_removed(mut world: DeferredWorld, ctx: HookContext) {
    let shooter = ctx.entity;
    world.commands().queue(move |w: &mut World| retire_shot_by(w, shooter));
}
/// Discards the `Order::Hurting` projectiles that `shooter` shot, frozen ones included.
fn retire_shot_by(w: &mut World, shooter: Entity) {
    let shot: Vec<Entity> = w
        .query_filtered::<(Entity, &Projectile, &Flight), Allow<Disabled>>()
        .iter(w)
        .filter(|(_, p, f)| f.order == Order::Hurting && p.owner == Some(Who::Entity(shooter)))
        .map(|(e, _, _)| e)
        .collect();
    for e in shot {
        w.despawn(e);
    }
}
/// The stuck projectiles by the block they are in, so that a block change finds them without a scan.
#[derive(Resource, Default)]
pub struct StuckIndex {
    pub at: HashMap<[i32; 3], Vec<Entity>>,
    /// The projectiles whose block went this tick: they start to fall at the end of the tick (`startFalling` is a tick of its own).
    pub falling: Vec<Entity>,
}

/// How long a stuck arrow stays (`AbstractArrow.tickDespawn`: 1 200 ticks).
const LIFE: u64 = 1200;

pub fn insert_resources(w: &mut World) {
    w.insert_resource(StuckIndex::default());
}
/// Some projectile is in flight, or one starts to fall: the system is skipped otherwise (its parameters cost about two thousand instructions a tick).
fn flights(q: Query<(), With<Flying>>, stuck: Res<StuckIndex>) -> bool {
    !q.is_empty() || !stuck.falling.is_empty()
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(stuck_wake.after(block_changes).before(crate::targeting::sight_eval));
    s.add_systems(fly.after(travel).before(reindex).run_if(flights));
}
/// Launches a projectile at `at` with `velocity`, owned by `owner`; saved (`Saved`, in the same insert) when its identifier is.
pub fn launch(world: &mut World, flight: Flight, at: Vec3, velocity: Vec3, owner: Option<Who>) -> Entity {
    let id = flight.id;
    let e = world.spawn((Pos(at), Vel(velocity), flight, Projectile { owner, age: 0, left_owner: false, expires: 0 }, Flying, OwnTime)).id();
    crate::nbt::mark_saved(world, e, id);
    if let (Order::Hurting, Some(Who::Entity(shooter))) = (flight.order, owner)
        && world.get_entity(shooter).is_ok()
    {
        world.entity_mut(shooter).insert_if_new(Shooter);
    }
    // Launched in a chunk that does not tick: it waits there (`ServerLevel.tick` does not tick it).
    crate::zone::settle(world, e);
    e
}
/// The builder of every projectile identifier (`nbt::Builder`): the kind of its saved `id` in `KINDS`, launched at the saved `Pos` with no
/// owner (the owner is not restored); the saved `Motion` is loaded on the entity by `restore`.
pub fn build(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let Some(Tag::String(id)) = c.get(&JavaString::from("id")) else { return None };
    let id = id.to_string_lossy();
    let (_, flight) = KINDS.iter().find(|(k, _)| *k == id)?;
    Some(launch(w, *flight, at, Vec3::ZERO, None))
}
fn bounds(at: Vec3, size: f64) -> Aabb {
    let w = size / 2.0;
    Aabb::new(Vec3::new(at.x - w, at.y, at.z - w), Vec3::new(at.x + w, at.y + size, at.z + w))
}
/// Whether a move took the projectile from one chunk to another: only such a move can take it out of the chunks that are loaded.
fn chunk_changed(a: Vec3, b: Vec3) -> bool {
    (a.x / 16.0).floor() != (b.x / 16.0).floor() || (a.z / 16.0).floor() != (b.z / 16.0).floor()
}
/// `ProjectileUtil.getManyEntityHitResult` for one box: where the path enters it, else where it enters the box grown by
/// `margin`, if a line from there towards the centre (stopped by blocks) reaches the box.
pub fn entity_hit(level: &Level, b: Aabb, from: Vec3, to: Vec3, margin: f64) -> Option<Vec3> {
    if let Some(at) = b.clip(from, to) {
        return Some(at);
    }
    if margin <= 0.0 {
        return None;
    }
    let outside = b.inflate(Vec3::new(margin, margin, margin)).clip(from, to)?;
    let centre = b.center();
    let mw = level.mw.0;
    let towards = clip_block_hit(mw.world, mw.facts, mw.collision, outside, Vec3::new(centre.x - outside.x, centre.y - outside.y, centre.z - outside.z)).map_or(centre, |h| h.0);
    b.clip(outside, towards).map(|_| outside)
}
/// A projectile fired during a tick (`homing_projectile`): an entity added in a tick is first ticked in the next one, with the world's clock at its birth.
#[derive(Component, Clone, Copy, Debug)]
pub struct BornAt(pub u64);
/// One tick of every projectile in flight.
#[allow(clippy::type_complexity)]
pub fn fly(
    clock: Res<Clock>,
    level: Res<Level>,
    idx: Res<Index>,
    on: Res<Online>,
    mut ps: ResMut<PlayerState>,
    mut dq: ResMut<DamageQueue>,
    mut wheel: ResMut<Wheel>,
    mut events: ResMut<Events>,
    mut stuck: ResMut<StuckIndex>,
    creatures: Query<&Bounds, With<Health>>,
    dodgers: Query<(), With<crate::behaviours::ender_teleport::Dodges>>,
    hurts: Query<&crate::cow::HurtAt>,
    potion: Query<&crate::effects::PotionId>,
    healths: Query<&Health>,
    mut q: Query<(Entity, &mut Pos, &mut Vel, &mut Projectile, &Flight, Option<&BornAt>), With<Flying>>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    // The ones whose block went on the last tick start to fall now (their own tick, without a move).
    for e in std::mem::take(&mut stuck.falling) {
        commands.entity(e).remove::<Stuck>().insert(Flying);
    }
    for (e, mut pos, mut vel, mut p, f, born) in &mut q {
        if born.is_some_and(|b| b.0 == clock.0) {
            continue;
        }
        p.age += 1;
        if f.order == Order::DragThenMove {
            // `ThrowableProjectile`: gravity, then inertia, before the move.
            vel.0 = Vec3::new(vel.0.x, vel.0.y - f.gravity, vel.0.z).scale(f.inertia);
        }
        if f.order == Order::Hurting {
            // `AbstractHurtingProjectile.applyInertia`: the push along the velocity, then the inertia.
            let v = vel.0;
            let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt().max(1.0e-9);
            vel.0 = Vec3::new((v.x + v.x / len * f.accel) * f.inertia, (v.y + v.y / len * f.accel) * f.inertia, (v.z + v.z / len * f.accel) * f.inertia);
        }
        let (original, movement) = (pos.0, vel.0);
        let end = Vec3::new(original.x + movement.x, original.y + movement.y, original.z + movement.z);
        // `Projectile.checkLeftOwner`: out of the owner's box once the box, swept by its motion and grown by 1, no longer touches it.
        if !p.left_owner {
            let owner = match p.owner {
                Some(Who::Entity(o)) => creatures.get(o).ok().map(|b| b.0),
                Some(Who::Player(id)) => on.get(id).map(|pl| player_box(&pl)),
                None => None,
            };
            let swept = bounds(pos.0, f.size).expand_towards(vel.0).inflate(Vec3::new(1.0, 1.0, 1.0));
            p.left_owner = owner.is_none_or(|b| !swept.intersects(b));
        }
        // `AbstractArrow.stepMoveAndHit`: the nearest creature on the way, else the block.
        let block = clip_block_hit(mw.world, mw.facts, mw.collision, original, movement);
        let stop = block.map_or(end, |b| b.0);
        let margin = f64::from(((p.age as i32 - 2) as f32 / 20.0).clamp(0.0, 0.3));
        let area = bounds(original, f.size).expand_towards(vel.0).inflate(Vec3::new(1.0, 1.0, 1.0));
        let mut best: Option<(Who, Vec3, f64)> = None;
        let mut consider = |who: Who, b: Aabb| {
            if Some(who) == p.owner && !p.left_owner || !b.intersects(area) {
                return;
            }
            if let Some(at) = entity_hit(&level, b, original, stop, margin) {
                let c = b.center();
                let d = original.distance_squared(Vec3::new(c.x, b.min_position().y, c.z));
                if best.is_none_or(|bb| d < bb.2) {
                    best = Some((who, at, d));
                }
            }
        };
        let reach = movement.length() + 2.0;
        idx.for_radius(original.x, original.z, reach, |c| {
            if let Ok(b) = creatures.get(c) {
                consider(Who::Entity(c), b.0);
            }
        });
        on.for_near(original.x, original.z, reach, |pl| consider(Who::Player(pl.id), player_box(&pl)));
        if let Some((who, at, _)) = best {
            // The projectile ends at the creature; the damage is queued with the shooter as the source.
            pos.0 = at;
            if let Who::Entity(t) = who
                && dodgers.contains(t)
            {
                // `Enderman.hurtServer`: a projectile does it no harm and it tries 64 times to leave.
                commands.queue(move |w: &mut World| {
                    crate::behaviours::ender_teleport::repeatedly(w, t, 64);
                });
                commands.entity(e).despawn();
                continue;
            }
            let amount = match f.damage {
                Damage::BySpeed(base) => (movement.length() * base).clamp(0.0, f64::from(i32::MAX)).ceil() as f32,
                Damage::Fixed(a) | Damage::Owned(a) => a,
            };
            // `WitherSkull.onHitEntity`: without a living owner the skull is a magic blow of 5.
            let amount = if f.wither && p.owner.is_none() { 5.0 } else { amount };
            let from = (original.x, original.z);
            let unowned_spit = matches!(f.damage, Damage::Owned(_)) && p.owner.is_none();
            match (who, p.owner) {
                _ if unowned_spit => {}
                (Who::Entity(t), None) if f.wither => dq.add(t, amount, Cause::Magic),
                (Who::Entity(t), Some(o)) => dq.add_via(t, amount, from, o),
                (Who::Entity(t), None) => dq.add_from(t, amount, Cause::Entity, from),
                (Who::Player(id), owner) => ps.hurt(clock.0, id, amount, from, owner),
            }
            if let Who::Entity(t) = who {
                events.projectile_hit.push(t);
                if let OnHit::Ignite(ticks) = f.hit {
                    commands.queue(move |w: &mut World| crate::cow::set_on_fire(w, t, ticks));
                }
                if let OnHit::Effect(kind, amp, ticks) = f.hit {
                    commands.queue(move |w: &mut World| {
                        crate::effects::add(w, t, kind, amp, ticks);
                    });
                }
                // The wither comes only when the blow hurts: outside the 10-tick window, or a larger blow in it (`HurtAt`).
                let now = clock.of(t);
                let hurting = !hurts.get(t).is_ok_and(|h| h.at != 0 && now < h.at + 10 && amount <= h.last);
                if f.wither && hurting {
                    commands.queue(move |w: &mut World| wither_hit(w, t));
                    // `WitherSkull.onHitEntity`: a blow that kills heals its living owner by 5 (the health is read before the blow lands).
                    if let (Some(Who::Entity(o)), true) = (p.owner, healths.get(t).is_ok_and(|h| h.0 <= amount)) {
                        commands.queue(move |w: &mut World| heal_owner(w, o, 5.0));
                    }
                }
                if matches!(f.hit, OnHit::LingeringPotion) {
                    commands.queue(move |w: &mut World| linger(w, e, at));
                }
            }
            if let (Who::Player(id), true) = (who, f.wither) {
                commands.queue(move |w: &mut World| wither_player(w, id));
            }
            if f.bullet && matches!(who, Who::Entity(_)) {
                commands.queue(move |w: &mut World| crate::shulker_bullet::hurt(w, at));
            }
            if let OnHit::Explode { power, fire } = f.hit {
                let blast = Blast { at, radius: power as f32, fire, owner: p.owner, source: None };
                let blue = f.blue;
                events.exploded.push(blast);
                commands.queue(move |w: &mut World| explode_hit(w, blast, blue));
            }
            if matches!(f.hit, OnHit::DragonBurst) {
                commands.queue(move |w: &mut World| { crate::dragon::fireball::burst(w, at); });
            }
            if let OnHit::WindBurst { radius } = f.hit {
                let owner = p.owner;
                commands.queue(move |w: &mut World| crate::explosion::wind_burst(w, at, radius, owner));
            }
            if let Some(brews) = splash_of(f.hit, potion.get(e).ok()) {
                let (hit, owner) = (if let Who::Entity(t) = who { Some(t) } else { None }, p.owner);
                commands.queue(move |w: &mut World| crate::effects::splash(w, at, hit, brews, owner));
            }
            if f.hit.thrown() {
                let (h, o, prev) = (f.hit, p.owner, original);
                commands.queue(move |w: &mut World| crate::projectile_more::ends(w, h, at, prev, o));
            }
            commands.entity(e).despawn();
            continue;
        }
        match block {
            Some((_, bpos, _)) => {
                // `AbstractArrow.onHitBlock`: back off the face by 0.05 along each axis it moved on, stop, and stick.
                pos.0 = back_off(stop, movement);
                vel.0 = Vec3::ZERO;
                if f.sticks {
                    let at = [bpos.x, bpos.y, bpos.z];
                    // Its life is counted in its own time (a projectile in a chunk nobody simulates does not age).
                    p.expires = clock.of(e) + LIFE;
                    wheel.push(clock.at(e, p.expires), e, Due::Expire);
                    stuck.at.entry(at).or_default().push(e);
                    commands.entity(e).remove::<Flying>().insert(Stuck { block: at });
                } else {
                    if let OnHit::Explode { power, fire } = f.hit {
                        let blast = Blast { at: pos.0, radius: power as f32, fire, owner: p.owner, source: None };
                        let blue = f.blue;
                        events.exploded.push(blast);
                        commands.queue(move |w: &mut World| explode_hit(w, blast, blue));
                    }
                    if matches!(f.hit, OnHit::DragonBurst) {
                        let at = pos.0;
                        commands.queue(move |w: &mut World| { crate::dragon::fireball::burst(w, at); });
                    }
                    if let OnHit::WindBurst { radius } = f.hit {
                        let (at, owner) = (pos.0, p.owner);
                        commands.queue(move |w: &mut World| crate::explosion::wind_burst(w, at, radius, owner));
                    }
                    if let Some(brews) = splash_of(f.hit, potion.get(e).ok()) {
                        let (at, owner) = (pos.0, p.owner);
                        commands.queue(move |w: &mut World| crate::effects::splash(w, at, None, brews, owner));
                    }
                    if matches!(f.hit, OnHit::LingeringPotion) {
                        let at = pos.0;
                        commands.queue(move |w: &mut World| linger(w, e, at));
                    }
                    if f.bullet {
                        let at = pos.0;
                        commands.queue(move |w: &mut World| crate::shulker_bullet::ends(w, at));
                    }
                    if f.hit.thrown() {
                        let (h, o, at, prev) = (f.hit, p.owner, pos.0, original);
                        commands.queue(move |w: &mut World| crate::projectile_more::ends(w, h, at, prev, o));
                    }
                    commands.entity(e).despawn();
                }
                continue;
            }
            None => {
                pos.0 = end;
                if f.spit_end && boxed_in_blocks(mw, bounds(pos.0, f.size)) {
                    commands.entity(e).despawn();
                    continue;
                }
                if f.spit_end {
                    // `LlamaSpit.tick`: its water check (`Entity.isInWater`, on the box) comes after the move, in the same tick; the world is read at the end of the frame.
                    let (lo, hi) = (bounds(pos.0, f.size).min_position(), bounds(pos.0, f.size).max_position());
                    let cells = [lo.x.floor() as i32, lo.y.floor() as i32, lo.z.floor() as i32, hi.x.floor() as i32, hi.y.floor() as i32, hi.z.floor() as i32];
                    commands.queue(move |w: &mut World| {
                        if boxed_in_liquid(w, cells) {
                            w.despawn(e);
                        }
                    });
                }
            }
        }
        // The end of the tick in flight: inertia, then gravity (an arrow; a thrown item did both before its move).
        if f.order == Order::MoveThenDrag {
            vel.0 = vel.0.scale(f.inertia);
            vel.0 = Vec3::new(vel.0.x, vel.0.y - f.gravity, vel.0.z);
        }
        let hurting = f.order == Order::Hurting;
        // `AbstractHurtingProjectile` has no age limit: it is discarded when it enters a chunk that is not loaded (`tick`: `hasChunkAt`).
        if pos.0.y < mw.world.below_world_y() || !hurting && p.age >= f.max_age && !f.sticks {
            commands.entity(e).despawn();
        } else if chunk_changed(original, pos.0) && !idx.zone.simulated_at(pos.0.x, pos.0.z) {
            if hurting {
                commands.entity(e).despawn();
            } else {
                // Out of the chunks that tick: it stops where it is, and flies on from there when a player comes back (`zone::thaw`).
                commands.queue(move |w: &mut World| {
                    let now = w.resource::<Clock>().0;
                    crate::zone::freeze(w, e, now);
                });
            }
        }
    }
}
/// A stuck projectile whose block went while it was frozen: it starts to fall at the end of the tick, as if it had been ticked.
pub fn recheck_stuck(w: &mut World, e: Entity) {
    let (Some(s), Some(p)) = (w.get::<Stuck>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { return };
    let level = w.resource::<Level>();
    let mw = level.mw.0;
    if no_block_collision(mw, Aabb::new(p, p).inflate(Vec3::new(0.06, 0.06, 0.06))) {
        let mut st = w.resource_mut::<StuckIndex>();
        if let Some(v) = st.at.get_mut(&s.block) {
            v.retain(|&o| o != e);
        }
        st.falling.push(e);
    }
}
/// A deadline of the wheel for a stuck projectile: it is gone once its life is over (the deadline is the wheel's reach,
/// the projectile's may be later).
pub fn expire(w: &mut World, e: Entity) {
    let (now, world_now) = (now_of(w, e), w.resource::<Clock>().0);
    let Some(p) = w.get::<Projectile>(e).copied() else { return };
    let Some(s) = w.get::<Stuck>(e).copied() else { return };
    if now < p.expires {
        let at = w.resource::<Clock>().at(e, p.expires);
        w.resource_mut::<Wheel>().push(at, e, Due::Expire);
        return;
    }
    if let Some(v) = w.resource_mut::<StuckIndex>().at.get_mut(&s.block) {
        v.retain(|&x| x != e);
    }
    w.entity_mut(e).despawn();
}
/// A block changed: the projectiles stuck in it fall if nothing holds them any more (`AbstractArrow.shouldFall`: the box
/// of the arrow's point grown by 0.06 meets no block).
fn stuck_wake(changed: Res<ChangedBlocks>, level: Res<Level>, mut stuck: ResMut<StuckIndex>, pos: Query<&Pos, With<Stuck>>, mut commands: Commands) {
    if changed.0.is_empty() || stuck.at.is_empty() {
        return;
    }
    let mw = level.mw.0;
    for &(x, y, z) in &changed.0 {
        let Some(list) = stuck.at.get(&[x, y, z]).cloned() else { continue };
        for e in list {
            let Ok(p) = pos.get(e) else { continue };
            if no_block_collision(mw, Aabb::new(p.0, p.0).inflate(Vec3::new(0.06, 0.06, 0.06))) {
                stuck.at.get_mut(&[x, y, z]).unwrap().retain(|&o| o != e);
                // `startFalling`: the velocity (zero for a stuck arrow) times three small randoms: it drops from rest.
                stuck.falling.push(e);
            }
        }
    }
}
/// `WitherSkull.onHitEntity` on a creature: wither II for 10 s on normal, 40 s on hard (`Difficulty` ids 2 and 3), none otherwise.
/// Vanilla gives it only when the hit hurt; the hit is taken as hurting here.
fn wither_hit(w: &mut World, t: Entity) {
    let ticks = wither_ticks(w);
    if ticks > 0 && w.get::<Health>(t).is_some() {
        crate::effects::add(w, t, crate::effects::Kind::Wither, 1, ticks);
    }
}
/// The brews a thrown potion splashes: its own `OnHit::Splash`, or the table's for its `PotionId`.
fn splash_of(hit: OnHit, id: Option<&crate::effects::PotionId>) -> Option<&'static [crate::effects::Brew]> {
    match hit {
        OnHit::Splash(brews) => Some(brews),
        OnHit::Potion => id.and_then(|p| crate::effects::potion_brews(p.0)),
        _ => None,
    }
}
/// The blast of a hit: a blue wither skull's caps the blocks' resistance (`explosion::explode_wither`).
fn explode_hit(w: &mut World, b: Blast, blue: bool) {
    if blue {
        crate::explosion::explode_wither(w, b, true);
    } else {
        crate::explosion::explode(w, b);
    }
}
/// `ThrownLingeringPotion.onHit` (`AreaEffectCloud`: radius 3, -0.5 a use, 600 ticks, wait 10; `radiusPerTick` = -radius / duration).
const LINGERING_CLOUD: crate::effect_cloud::EffectCloud = crate::effect_cloud::EffectCloud {
    radius: 3.0,
    radius_per_tick: -3.0 / 600.0,
    radius_on_use: -0.5,
    duration_on_use: 0,
    age: 0,
    wait: 10,
    duration: 600,
    reapplication_delay: 20,
    duration_scale: 1.0,
    hidden: &[],
    ambient: &[],
    owner: None,
};
/// A cloud of the flask's potion where it broke; a flask without a `PotionId` makes none.
fn linger(w: &mut World, e: Entity, at: Vec3) {
    if let Some(p) = w.get::<crate::effects::PotionId>(e).copied() {
        crate::effect_cloud::spawn(w, [at.x, at.y, at.z], p, LINGERING_CLOUD);
    }
}
/// `WitherSkull.onHitEntity`: the living owner heals 5 when its skull kills.
fn heal_owner(w: &mut World, o: Entity, amount: f32) {
    let max = w.get::<MaxHealth>(o).map_or(f32::MAX, |m| m.0);
    if let Some(mut h) = w.get_mut::<Health>(o) {
        h.0 = (h.0 + amount).min(max);
    }
}
/// A potion a player throws (the player is outside the ECS): its owner, its id, a splash or a lingering flask.
pub fn throw_potion(w: &mut World, owner: u32, at: Vec3, velocity: Vec3, potion: crate::effects::PotionId, lingering: bool) -> Entity {
    let flight = if lingering { Flight::LINGERING_POTION } else { Flight::POTION };
    let e = launch(w, flight, at, velocity, Some(Who::Player(owner)));
    w.entity_mut(e).insert(potion);
    e
}
/// The same effect on a player (`effects::add_player`, its end counted from now).
fn wither_player(w: &mut World, id: u32) {
    let ticks = wither_ticks(w);
    if ticks > 0 {
        crate::effects::add_player(w, id, crate::effects::Kind::Wither, 1, ticks);
    }
}
fn wither_ticks(w: &World) -> i32 {
    match crate::effects::difficulty(w) {
        2 => 200,
        3 => 800,
        _ => 0,
    }
}
/// `Entity.isInWater` on the box: any liquid cell among the box's floor range (`cells`: lo and hi corners).
fn boxed_in_liquid(w: &World, cells: [i32; 6]) -> bool {
    let [x0, y0, z0, x1, y1, z1] = cells;
    (x0..=x1).any(|x| (y0..=y1).any(|y| (z0..=z1).any(|z| crate::level::is_liquid(w, [x, y, z]))))
}
/// `findBlocksIn(box).filterState(isAir).noneMatched()`: every block the box touches is not air.
fn boxed_in_blocks(mw: &horizon_client_lib::mobs::MobWorld<'static>, b: Aabb) -> bool {
    let (lo, hi) = (b.min_position(), b.max_position());
    let mut cursor = horizon_client_render::world::BlockCursor::new(mw.world);
    for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
        for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
            for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                if mw.facts.is_air(cursor.block(x, y, z)) {
                    return false;
                }
            }
        }
    }
    true
}
/// `CollisionGetter.noCollision(null, box)` over blocks.
fn no_block_collision(mw: &horizon_client_lib::mobs::MobWorld<'static>, b: Aabb) -> bool {
    use horizon_client_render::collisions::SceneCollisions;
    let (lo, hi) = (b.min_position(), b.max_position());
    let mut scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
    let mut cursor = horizon_client_render::world::BlockCursor::new(mw.world);
    for x in (lo.x.floor() as i32 - 1)..=(hi.x.floor() as i32 + 1) {
        for y in (lo.y.floor() as i32 - 1)..=(hi.y.floor() as i32 + 1) {
            for z in (lo.z.floor() as i32 - 1)..=(hi.z.floor() as i32 + 1) {
                let state = cursor.block(x, y, z);
                if mw.facts.is_air(state) {
                    continue;
                }
                let origin = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                let boxes = mw.collision.collision_shape(&mut scene, state, [x, y, z]).map(|s| s.boxes().to_vec()).unwrap_or_default();
                if boxes.iter().any(|c| c.move_by(origin).intersects(b)) {
                    return false;
                }
            }
        }
    }
    true
}
/// No collision shape at `p`: the way a projectile or a steered skull can go.
pub fn block_empty(w: &World, p: [i32; 3]) -> bool {
    crate::level::block_empty(w, p)
}
/// A projectile at height `entity_y` stands on the block at `p`.
pub fn can_stand_on(w: &World, p: [i32; 3], entity_y: f64) -> bool {
    crate::level::can_stand_on(w, p, entity_y)
}
