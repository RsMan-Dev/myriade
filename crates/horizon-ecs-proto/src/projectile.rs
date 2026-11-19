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
use bevy_ecs::prelude::*;
use horizon_client_lib::arrows::{back_off, clip_block_hit};
use horizon_core::geometry::{Aabb, Vec3};
use std::collections::HashMap;

/// What differs between the kinds of projectile: how they fall, slow down, end and hurt.
#[derive(Component, Clone, Copy, Debug)]
pub struct Flight {
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    MoveThenDrag,
    DragThenMove,
}
#[derive(Clone, Copy, Debug)]
pub enum Damage {
    /// `ceil(speed * base)` (`AbstractArrow.onHitEntity`, a non-critical arrow).
    BySpeed(f64),
    /// A fixed amount (a snowball hurts for 0, a blaze's for 3).
    Fixed(f32),
}
/// `0.99F`, the way vanilla multiplies the velocity of a projectile.
const INERTIA: f64 = 0.99_f32 as f64;
impl Flight {
    pub const ARROW: Flight = Flight { gravity: 0.05, inertia: INERTIA, sticks: true, damage: Damage::BySpeed(2.0), size: 0.5, order: Order::MoveThenDrag };
    pub const SNOWBALL: Flight = Flight { gravity: 0.03, inertia: INERTIA, sticks: false, damage: Damage::Fixed(0.0), size: 0.25, order: Order::DragThenMove };
}
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
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(stuck_wake.after(block_changes).before(crate::targeting::sight_eval));
    s.add_systems(fly.after(travel).before(reindex));
}
/// Launches a projectile at `at` with `velocity`, owned by `owner`.
pub fn launch(world: &mut World, flight: Flight, at: Vec3, velocity: Vec3, owner: Option<Who>) -> Entity {
    world.spawn((Pos(at), Vel(velocity), flight, Projectile { owner, age: 0, left_owner: false, expires: 0 }, Flying, OwnTime)).id()
}
fn bounds(at: Vec3, size: f64) -> Aabb {
    let w = size / 2.0;
    Aabb::new(Vec3::new(at.x - w, at.y, at.z - w), Vec3::new(at.x + w, at.y + size, at.z + w))
}
/// `ProjectileUtil.getManyEntityHitResult` for one box: where the path enters it, else where it enters the box grown by
/// `margin`, if a line from there towards the centre (stopped by blocks) reaches the box.
fn entity_hit(level: &Level, b: Aabb, from: Vec3, to: Vec3, margin: f64) -> Option<Vec3> {
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
    mut q: Query<(Entity, &mut Pos, &mut Vel, &mut Projectile, &Flight), With<Flying>>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    // The ones whose block went on the last tick start to fall now (their own tick, without a move).
    for e in std::mem::take(&mut stuck.falling) {
        commands.entity(e).remove::<Stuck>().insert(Flying);
    }
    for (e, mut pos, mut vel, mut p, f) in &mut q {
        p.age += 1;
        if f.order == Order::DragThenMove {
            // `ThrowableProjectile`: gravity, then inertia, before the move.
            vel.0 = Vec3::new(vel.0.x, vel.0.y - f.gravity, vel.0.z).scale(f.inertia);
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
            let amount = match f.damage {
                Damage::BySpeed(base) => (movement.length() * base).clamp(0.0, f64::from(i32::MAX)).ceil() as f32,
                Damage::Fixed(a) => a,
            };
            let from = (original.x, original.z);
            match (who, p.owner) {
                (Who::Entity(t), Some(Who::Entity(o))) => dq.add_by_entity(t, amount, from, o),
                (Who::Entity(t), Some(Who::Player(id))) => dq.add_by(t, amount, from, id),
                (Who::Entity(t), None) => dq.add_from(t, amount, Cause::Entity, from),
                (Who::Player(id), _) => ps.hurt(clock.0, id, amount, from),
            }
            if let Who::Entity(t) = who {
                events.projectile_hit.push(t);
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
                    wheel.push(clock.at(e, p.expires).min(clock.0 + 4095), e, Due::Expire);
                    stuck.at.entry(at).or_default().push(e);
                    commands.entity(e).remove::<Flying>().insert(Stuck { block: at });
                } else {
                    commands.entity(e).despawn();
                }
                continue;
            }
            None => pos.0 = end,
        }
        // The end of the tick in flight: inertia, then gravity (an arrow; a thrown item did both before its move).
        if f.order == Order::MoveThenDrag {
            vel.0 = vel.0.scale(f.inertia);
            vel.0 = Vec3::new(vel.0.x, vel.0.y - f.gravity, vel.0.z);
        }
        if pos.0.y < mw.world.below_world_y() || p.age >= 6000 && !f.sticks {
            commands.entity(e).despawn();
        }
    }
}
/// A deadline of the wheel for a stuck projectile: it is gone once its life is over (the deadline is the wheel's reach,
/// the projectile's may be later).
pub fn expire(w: &mut World, e: Entity) {
    let (now, world_now) = (now_of(w, e), w.resource::<Clock>().0);
    let Some(p) = w.get::<Projectile>(e).copied() else { return };
    let Some(s) = w.get::<Stuck>(e).copied() else { return };
    if now < p.expires {
        let at = w.resource::<Clock>().at(e, p.expires).min(world_now + 4095);
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
/// `CollisionGetter.noCollision(null, box)` over blocks.
fn no_block_collision(mw: &horizon_client_lib::mobs::MobWorld<'static>, b: Aabb) -> bool {
    use horizon_client_render::collisions::SceneCollisions;
    let (lo, hi) = (b.min_position(), b.max_position());
    let mut scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
    for x in (lo.x.floor() as i32 - 1)..=(hi.x.floor() as i32 + 1) {
        for y in (lo.y.floor() as i32 - 1)..=(hi.y.floor() as i32 + 1) {
            for z in (lo.z.floor() as i32 - 1)..=(hi.z.floor() as i32 + 1) {
                let state = mw.world.block(x, y, z);
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
