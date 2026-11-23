//! The thrown trident (`ThrownTrident`), an entity that is not a `Mob`. It flies with the arrow's gravity 0.05 and inertia 0.99, deals
//! 8.0 once to the first creature or player it meets (then turns back: `ProjectileDeflection.REVERSE`), sticks in a block for 7
//! ticks of shake, then lies there until it is picked up or its life of 1 200 ticks ends. Loyalty brings it back to its owner.
//! A planted trident costs nothing per tick: its wakes are the wheel's (`after`), a player's move (`Online.seen`) and a block change
//! (`ChangedBlocks`). Its flight is an exclusive system: it runs only when a trident flies.
use crate::cow::*;
use crate::goals::Online;
use crate::hostile::{ChangedBlocks, PlayerState};
use crate::projectile::{entity_hit, Projectile};
use crate::targeting::{player_box, Who};
use bevy_ecs::prelude::*;
use myriade_engine_lib::arrows::{back_off, clip_block_hit};
use myriade_engine_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use std::collections::HashMap;

/// Who may take a trident back (`AbstractArrow.Pickup`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Pickup {
    Disallowed,
    Allowed,
    CreativeOnly,
}
/// The blow of a trident: 8.0, a fixed amount before the enchantments (`ThrownTrident.onHitEntity`).
pub const BLOW: f32 = 8.0;
/// The reverse of a blow: the velocity times -power * 0.5, power (0.02, 0.2, 0.02) (`ProjectileDeflection.REVERSE`).
pub const DEFLECT: [f64; 3] = [-0.01, -0.1, -0.01];
/// The shake of a block hit (`shakeTime = 7`) and the life on the ground (`AbstractArrow.tickDespawn`: 1 200 ticks).
pub const SHAKE: i32 = 7;
pub const LIFE: u64 = 1200;
/// The margin that widens a creature's or a player's box for the path (`ProjectileUtil` margin of an arrow; doubt in the notes).
const MARGIN: f64 = 0.3;

/// The trident: its owner, its Loyalty level, who may take it back, whether it has hit (`dealtDamage`), and the stack it gives back.
#[derive(Component, Clone, Debug)]
pub struct Trident {
    pub owner: Option<Who>,
    pub loyalty: u8,
    pub pickup: Pickup,
    pub dealt: bool,
    pub stack: ItemStack,
    /// Channeling: a blow in a storm strikes the victim with lightning (`trident_hit::channels`).
    pub channeling: bool,
}
/// The damage a blow adds: `(stack, victim, the victim's species id)`, the host's table (`EnchantmentHelper.modifyDamage`).
pub type BlowBonus = Box<dyn Fn(&ItemStack, Who, &str) -> f32 + Send + Sync>;
#[derive(Resource)]
pub struct HitBonus(pub BlowBonus);
impl Default for HitBonus {
    fn default() -> Self {
        Self(Box::new(|_, _, _| 0.0))
    }
}
/// In flight: the flight visits it each tick (zero-size, sparse).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct TridentFlying;
/// Stuck in `block` since its own time `since`; it may be taken from `shake_end` on; it expires at `since + LIFE`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Planted {
    pub block: [i32; 3],
    pub since: u64,
    pub shake_end: u64,
}
/// Loyalty return: pulled towards its owner, no physics (zero-size, sparse).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Returning;

/// The planted tridents by block cell (a block change finds them) and by 8-block bin (a player's move finds them).
#[derive(Resource, Default)]
pub struct TridentIndex {
    pub blocks: HashMap<[i32; 3], Vec<Entity>>,
    pub bins: HashMap<(i32, i32), Vec<Entity>>,
}
/// The tridents taken this tick, for the pocket system: (player, stack, trident). Drained by it.
#[derive(Resource, Default)]
pub struct TridentTaken(pub Vec<(u32, ItemStack, Entity)>);

pub fn insert_resources(w: &mut World) {
    w.insert_resource(TridentIndex::default());
    w.insert_resource(TridentTaken::default());
    w.insert_resource(crate::host_out_trident::TridentOut::default());
    w.insert_resource(HitBonus::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((block_changes, fly, pickups).chain().after(crate::projectile::fly));
}

/// `ThrownTrident` thrown from `at` with `velocity`, owned by `owner`.
pub fn throw(world: &mut World, stack: ItemStack, at: Vec3, velocity: Vec3, owner: Option<Who>, loyalty: u8, pickup: Pickup) -> Entity {
    let t = Trident { owner, loyalty, pickup, dealt: false, stack, channeling: false };
    let e = world.spawn((Pos(at), Vel(velocity), t, Projectile { owner, age: 0, left_owner: false, expires: 0 }, TridentFlying, OwnTime)).id();
    crate::nbt::mark_saved(world, e, crate::species::lists::TRIDENT);
    world.entity_mut(e).insert(crate::cow::TypeName(crate::species::lists::TRIDENT));
    e
}

fn bin(x: f64, z: f64) -> (i32, i32) {
    ((x / 8.0).floor() as i32, (z / 8.0).floor() as i32)
}
/// The position of an owner: a player's eyes, a creature's feet (doubt in the notes).
fn owner_at(world: &World, owner: Option<Who>) -> Option<Vec3> {
    match owner? {
        Who::Player(id) => world.resource::<Online>().get(id).map(|p| Vec3::new(p.x, p.y + 1.62, p.z)),
        Who::Entity(e) => world.get::<Pos>(e).map(|p| p.0),
    }
}
fn now(world: &World) -> u64 {
    world.resource::<Clock>().0
}

/// The creatures and players whose box the path `from`..`to` enters, nearest first: (distance, who, the creature or player id).
#[derive(Clone, Copy)]
enum Hitter {
    Creature(Entity),
    Player(u32),
}
fn nearest_hit(world: &World, e: Entity, from: Vec3, reach: Vec3, owner: Option<Who>, left_owner: bool) -> Option<(f64, Vec3, Hitter)> {
    let level = *world.resource::<Level>();
    let (mx, mz) = ((from.x + reach.x) / 2.0, (from.z + reach.z) / 2.0);
    let r = from.distance(reach) / 2.0 + 1.5;
    let mut cands = Vec::new();
    world.resource::<Index>().for_radius(mx, mz, r, |c| cands.push(c));
    let mut best: Option<(f64, Vec3, Hitter)> = None;
    let mut keep = |d: f64, at: Vec3, h: Hitter| {
        if best.is_none_or(|(b, _, _)| d < b) {
            best = Some((d, at, h));
        }
    };
    for c in cands {
        if c == e {
            continue;
        }
        let (Some(b), Some(_)) = (world.get::<Bounds>(c), world.get::<Health>(c)) else { continue };
        if let Some(at) = entity_hit(&level, b.0, from, reach, MARGIN) {
            keep(from.distance(at), at, Hitter::Creature(c));
        }
    }
    let on = world.resource::<Online>();
    let ps = world.resource::<PlayerState>();
    let mut near = Vec::new();
    on.for_near(mx, mz, r, |p| near.push(p));
    for p in near {
        if ps.creative(p.id) || ps.spectator(p.id) {
            continue;
        }
        if matches!(owner, Some(Who::Player(id)) if id == p.id) && !left_owner {
            continue;
        }
        if let Some(at) = entity_hit(&level, player_box(&p), from, reach, MARGIN) {
            keep(from.distance(at), at, Hitter::Player(p.id));
        }
    }
    best
}

/// One tick of every trident in flight, in the jar's order: the Loyalty return, else the move (the nearest of the first block and
/// the first creature or player on the path), a blow (once), a reverse or a stick, then the inertia and the gravity after the move.
fn fly(world: &mut World) {
    if !crate::goals::sparse_any::<TridentFlying>(world) {
        return;
    }
    let flying: Vec<Entity> = world.query_filtered::<Entity, With<TridentFlying>>().iter(world).collect();
    for e in flying {
        let Some(t) = world.get::<Trident>(e) else { continue };
        let (owner, loyalty, dealt) = (t.owner, t.loyalty, t.dealt);
        let (from, mut vel) = (world.get::<Pos>(e).unwrap().0, world.get::<Vel>(e).unwrap().0);
        if dealt && loyalty > 0 {
            if let Some(eye) = owner_at(world, owner) {
                let k = 0.05 * f64::from(loyalty);
                let mut pos = from;
                if pos.distance(eye) < 0.25 + 1.0 {
                    if matches!(owner, Some(Who::Entity(_))) {
                        world.resource_mut::<crate::host_out_trident::TridentOut>().0.push(crate::host_out_trident::TridentEvent::Returned { entity: e, owner });
                        world.despawn(e);
                        continue;
                    }
                    // A player owner takes it back by the pickup path; `Returned` follows once it is taken.
                    touch(world, e);
                    if world.get_entity(e).is_err() {
                        world.resource_mut::<crate::host_out_trident::TridentOut>().0.push(crate::host_out_trident::TridentEvent::Returned { entity: e, owner });
                        continue;
                    }
                }
                pos.y += (eye.y - pos.y) * 0.015 * f64::from(loyalty);
                let d = Vec3::new(eye.x - pos.x, eye.y - pos.y, eye.z - pos.z);
                let len = d.length().max(1.0e-9);
                vel = Vec3::new(vel.x * 0.95 + d.x / len * k, vel.y * 0.95 + d.y / len * k, vel.z * 0.95 + d.z / len * k);
                let next = Vec3::new(pos.x + vel.x, pos.y + vel.y, pos.z + vel.z);
                world.entity_mut(e).insert((Pos(next), Vel(vel), Returning));
                touch(world, e);
                continue;
            }
        }
        let level = *world.resource::<Level>();
        let mw = level.mw.0;
        let reach_block = clip_block_hit(mw.world, mw.facts, mw.collision, from, vel);
        let reach = reach_block.map_or(Vec3::new(from.x + vel.x, from.y + vel.y, from.z + vel.z), |h| h.0);
        let left = world.get::<Projectile>(e).is_some_and(|p| p.left_owner);
        let hit = if dealt { None } else { nearest_hit(world, e, from, reach, owner, left) };
        if let Some((_, at, who)) = hit {
            let stack_pos = (from.x, from.z);
            match who {
                Hitter::Creature(c) => {
                    let amount = crate::trident_hit::hit_entity(world, e, c).damage;
                    world.resource_mut::<DamageQueue>().0.push(crate::cow::Hit { target: c, amount, cause: Cause::Entity, damage: "trident", from: Some(stack_pos), by: owner, direct: false, power: None, extra: None, projectile: Some(crate::species::lists::TRIDENT) });
                }
                Hitter::Player(id) => {
                    let amount = crate::trident_hit::hit_player(world, e, id, at).damage;
                    let tick = now(world);
                    world.resource_mut::<PlayerState>().hurt(tick, id, amount, stack_pos, owner);
                }
            }
            world.get_mut::<Trident>(e).unwrap().dealt = true;
            vel = Vec3::new(vel.x * DEFLECT[0], vel.y * DEFLECT[1], vel.z * DEFLECT[2]);
            world.entity_mut(e).insert((Pos(at), Vel(vel)));
            after_move(world, e, at, owner, vel);
            continue;
        }
        if let Some((at, b)) = reach_block.map(|h| (h.0, [h.1.x, h.1.y, h.1.z])) {
            if dealt && loyalty > 0 && owner.is_some() {
                world.entity_mut(e).insert((Pos(back_off(at, vel)), Vel(Vec3::new(0.0, 0.0, 0.0)), Returning));
                continue;
            }
            stick(world, e, back_off(at, vel), b, loyalty);
            crate::trident_hit::hit_block(world, e, b);
            continue;
        }
        world.entity_mut(e).insert((Pos(reach), Vel(vel)));
        after_move(world, e, reach, owner, vel);
    }
}
/// `0.99F` widened, as vanilla multiplies the velocity (`AbstractArrow.getWaterInertia`'s air counterpart).
const INERTIA: f64 = 0.99_f32 as f64;
/// The end of a flying tick: the age, the leaving of the owner, the inertia 0.99 and the gravity 0.05 of the velocity.
fn after_move(world: &mut World, e: Entity, at: Vec3, owner: Option<Who>, vel: Vec3) {
    let owner_pos = owner_at(world, owner);
    let v = Vec3::new(vel.x * INERTIA, vel.y * INERTIA - 0.05, vel.z * INERTIA);
    world.entity_mut(e).insert((Pos(at), Vel(v)));
    if let Some(mut p) = world.get_mut::<Projectile>(e) {
        p.age += 1;
        if !p.left_owner && owner_pos.is_some_and(|o| o.distance(at) > 1.0) {
            p.left_owner = true;
        }
    }
}
/// `ThrownTrident` lodged in a block: no motion, shake 7, then its life (which a Loyalty trident with a pickup allowed does not have).
fn stick(world: &mut World, e: Entity, at: Vec3, block: [i32; 3], loyalty: u8) {
    let since = now_of(world, e);
    let pickup = world.get::<Trident>(e).unwrap().pickup;
    let shake_end = since + SHAKE as u64;
    world.entity_mut(e).remove::<TridentFlying>().insert((Pos(at), Vel(Vec3::new(0.0, 0.0, 0.0)), Planted { block, since, shake_end }));
    let b = bin(at.x, at.z);
    let mut index = world.resource_mut::<TridentIndex>();
    index.blocks.entry(block).or_default().push(e);
    index.bins.entry(b).or_default().push(e);
    after(world, e, SHAKE, touch);
    if !(loyalty > 0 && pickup == Pickup::Allowed) {
        after(world, e, LIFE as i32, expire);
    }
}
/// Removes a planted trident from the indexes.
pub(crate) fn forget(world: &mut World, e: Entity) {
    let Some(p) = world.get::<Planted>(e).copied() else { return };
    let at = world.get::<Pos>(e).map_or(Vec3::new(0.0, 0.0, 0.0), |q| q.0);
    let mut index = world.resource_mut::<TridentIndex>();
    if let Some(v) = index.blocks.get_mut(&p.block) {
        v.retain(|&x| x != e);
    }
    if let Some(v) = index.bins.get_mut(&bin(at.x, at.z)) {
        v.retain(|&x| x != e);
    }
}
/// The expiry of a planted trident (a wheel call; a stale call from an earlier stick finds a later `since`).
fn expire(world: &mut World, e: Entity) {
    let Some(p) = world.get::<Planted>(e).copied() else { return };
    let Some(t) = world.get::<Trident>(e) else { return };
    if t.loyalty > 0 && t.pickup == Pickup::Allowed {
        return;
    }
    if now_of(world, e) >= p.since + LIFE {
        forget(world, e);
        world.despawn(e);
    }
}
/// The pickup of a trident by a player (`ThrownTrident.playerTouch`): the shake is over, the player is the owner (or any when there is
/// none), its box meets the trident (|dx| < 1, |dz| < 1, y within feet - 1 .. feet + 2.5), and the pickup rule allows it.
fn touch(world: &mut World, e: Entity) {
    let at = world.get::<Pos>(e).unwrap().0;
    let mut near = Vec::new();
    world.resource::<Online>().for_near(at.x, at.z, 1.0, |p| near.push(p.id));
    for id in near {
        if let Some(stack) = crate::trident_hit::pickup(world, e, id) {
            world.resource_mut::<TridentTaken>().0.push((id, stack, e));
            return;
        }
    }
}
/// The block of a planted trident changed: it falls again (`startFalling`), its life starts over.
fn block_changes(world: &mut World) {
    let changed = std::mem::take(&mut world.resource_mut::<ChangedBlocks>().0);
    if changed.is_empty() {
        return;
    }
    for (x, y, z) in changed {
        let Some(list) = world.resource::<TridentIndex>().blocks.get(&[x, y, z]).cloned() else { continue };
        for e in list {
            forget(world, e);
            world.entity_mut(e).remove::<Planted>().insert(TridentFlying);
        }
    }
}
/// A player's move (`Online.seen`) wakes the planted tridents of the bins it stands in or next to.
fn pickups(world: &mut World) {
    let moved: Vec<_> = world.resource::<Online>().seen.iter().filter_map(|(_, new)| *new).collect();
    for p in moved {
        let (bx, bz) = bin(p.x, p.z);
        let mut found = Vec::new();
        for dz in -1..=1 {
            for dx in -1..=1 {
                if let Some(v) = world.resource::<TridentIndex>().bins.get(&(bx + dx, bz + dz)) {
                    found.extend(v.iter().copied());
                }
            }
        }
        for e in found {
            touch(world, e);
        }
    }
}
