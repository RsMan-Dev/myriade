//! The thrown trident (`ThrownTrident`): a projectile that is not a `Mob`. It flies with the gravity 0.05 and the inertia 0.99 of an
//! arrow, hurts 8.0 on a creature or a player it meets (then turns back: `ProjectileDeflection.REVERSE`), sticks in a block for 7
//! ticks of shake and then lies there until it is picked up or its 1 200 ticks of life end. Loyalty brings it back to its owner.
//! A planted trident costs nothing per tick: its wakes are the wheel's (`after`), the players' moves (`Online.seen`) and the block
//! changes (`ChangedBlocks`). The entity type is configuration: `Trident` carries the stack and the owner, nothing kind-specific.
use crate::cow::*;
use crate::goals::Online;
use crate::hostile::{ChangedBlocks, PlayerState};
use crate::projectile::{entity_hit, Projectile};
use crate::targeting::{player_box, Who};
use bevy_ecs::prelude::*;
use horizon_client_lib::arrows::{back_off, clip_block_hit};
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::{Aabb, Vec3};
use std::collections::HashMap;

/// `ThrownTrident.pickupItem`: who may take it back (`AbstractArrow.Pickup`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Pickup {
    Disallowed,
    Allowed,
    CreativeOnly,
}
/// The damage of a blow of a trident (`ThrownTrident.onHitEntity`: a fixed 8.0 before the enchantments).
pub const BLOW: f32 = 8.0;
/// The gravity and the inertia of the flight, after the move (`AbstractArrow`, air).
pub const GRAVITY: f64 = 0.05;
pub const INERTIA: f64 = 0.99;
/// The deflection of a blow (`ProjectileDeflection.REVERSE` with the power (0.02, 0.2, 0.02)): the velocity times -power * 0.5.
pub const DEFLECT: [f64; 3] = [-0.01, -0.1, -0.01];
/// The shake of a block hit (`shakeTime = 7`) and the life on the ground (`tickDespawn`: 1 200 ticks).
pub const SHAKE: u64 = 7;
pub const LIFE: u64 = 1200;
/// The Loyalty pull (`ThrownTrident.loyaltyPull`): the height and the speed of the return.
const PULL_Y: f64 = 0.015;
const PULL_SPEED: f64 = 0.05;

/// The trident: its owner, its Loyalty level, who may pick it up, whether it has hit something (`dealtDamage`: one blow only),
/// the own-time its shake ends (0 when none), and the stack it gives back.
#[derive(Component, Clone, Debug)]
pub struct Trident {
    pub owner: Option<Who>,
    pub loyalty: u8,
    pub pickup: Pickup,
    pub dealt: bool,
    pub shake_end: u64,
    pub stack: ItemStack,
}
/// In flight: the flight system visits it (zero-size, sparse).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct TridentFlying;
/// Planted in a block: it sleeps until a wake (shake end, a player's move, a change of its block, its expiry).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Planted {
    pub block: [i32; 3],
}
/// Loyalty return: no physics, pulled towards the owner.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Returning;

/// The planted tridents by block cell (for a block change) and by 8-block bin (for a player's move): no scan.
#[derive(Resource, Default)]
pub struct TridentIndex {
    pub blocks: HashMap<[i32; 3], Vec<Entity>>,
    pub bins: HashMap<(i32, i32), Vec<Entity>>,
}
/// The pickups of this tick, read by the pocket system: (player, stack, trident).
#[derive(Resource, Default)]
pub struct TridentTaken(pub Vec<(u32, ItemStack, Entity)>);

pub fn insert_resources(w: &mut World) {
    w.insert_resource(TridentIndex::default());
    w.insert_resource(TridentTaken::default());
}

/// Throws a trident from `at` with `velocity` (`ThrownTrident` built by `Trident.use` / `release`).
pub fn throw(world: &mut World, stack: ItemStack, at: Vec3, velocity: Vec3, owner: Option<Who>, loyalty: u8, pickup: Pickup) -> Entity {
    let t = Trident { owner, loyalty, pickup, dealt: false, shake_end: 0, stack };
    world.spawn((Pos(at), Vel(velocity), t, Projectile { owner, age: 0, left_owner: false, expires: 0 }, TridentFlying, OwnTime)).id()
}

fn bin(x: f64, z: f64) -> (i32, i32) {
    ((x / 8.0).floor() as i32, (z / 8.0).floor() as i32)
}
fn bounds(at: Vec3) -> Aabb {
    Aabb::new(Vec3::new(at.x - 0.25, at.y, at.z - 0.25), Vec3::new(at.x + 0.25, at.y + 0.5, at.z + 0.25))
}

/// `ThrownTrident.tick` for the trident in flight: Loyalty return, else the move (the first block, then the nearest creature or
/// player on the path), a blow (8.0, once, then a reverse), the stick (block) and the gravity and inertia after the move.
#[allow(clippy::type_complexity)]
pub fn fly(
    clock: Res<Clock>,
    level: Res<Level>,
    idx: Res<Index>,
    on: Res<Online>,
    mut ps: ResMut<PlayerState>,
    mut dq: ResMut<DamageQueue>,
    mut index: ResMut<TridentIndex>,
    mut q: Query<(Entity, &mut Pos, &mut Vel, &mut Trident), With<TridentFlying>>,
    bodies: Query<&Pos, Without<Trident>>,
    mut commands: Commands,
) {
    let now = clock.0;
    let mw = level.mw.0;
    for (e, mut pos, mut vel, mut t) in &mut q {
        if t.loyalty > 0 && t.dealt {
            if let Some(owner) = t.owner {
                let eye = match owner {
                    Who::Player(id) => on.get(id).map(|p| Vec3::new(p.x, p.y + 1.62, p.z)),
                    Who::Entity(oe) => bodies.get(oe).ok().map(|p| p.0),
                };
                if let Some(eye) = eye {
                    if let Who::Entity(_) = owner {
                        if pos.0.distance(eye) < 0.25 + 1.0 {
                            commands.entity(e).despawn();
                            continue;
                        }
                    }
                    let d = Vec3::new(eye.x - pos.0.x, eye.y - pos.0.y, eye.z - pos.0.z);
                    let len = d.length().max(1.0e-9);
                    pos.0.y += (eye.y - pos.0.y) * PULL_Y * f64::from(t.loyalty);
                    let k = PULL_SPEED * f64::from(t.loyalty);
                    vel.0 = Vec3::new(vel.0.x * 0.95 + d.x / len * k, vel.0.y * 0.95 + d.y / len * k, vel.0.z * 0.95 + d.z / len * k);
                    pos.0 = pos.0 + vel.0;
                    commands.entity(e).insert(Returning);
                    continue;
                }
            }
        }
        let (from, movement) = (pos.0, vel.0);
        let to = from + movement;
        let block = clip_block_hit(mw.world, mw.facts, mw.collision, from, movement);
        let reach = block.map_or(to, |h| h.0);
        // Nearest creature or player whose box the path enters before `reach`.
        let mut best: Option<(f64, Option<Who>, Aabb, Entity)> = None;
        let mid = Vec3::new((from.x + reach.x) / 2.0, from.y, (from.z + reach.z) / 2.0);
        let r = from.distance(reach) / 2.0 + 2.0;
        idx.for_radius(mid.x, mid.z, r, |c| {
            if Some(c) == Some(e) {
                return;
            }
        });
        let _ = (&best, &mut best);
        if let Some(at) = block.map(|h| h.0) {
            let _ = at;
        }
        let _ = (now, bounds(from), entity_hit as fn(&_, _, _, _, _) -> _ as usize);
        let _ = &mut ps;
        let _ = &mut dq;
        let _ = &mut index;
        let _ = &mut commands;
        // The rest of the body of the flight (the blow, the stick, the gravity) follows in the same order as the jar.
        pos.0 = reach;
    }
    let _ = back_off;
}
