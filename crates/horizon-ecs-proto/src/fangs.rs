//! `EvokerFangs` (`projectile/EvokerFangs`): a fang is an entity of its own with a position, its owner and its clock, and no tick.
//! Three deadlines of its own time wake it: the start at warmup `+ 1` (event 4 to the host, which plays the attack sound), the bite at
//! `-8` (6 of indirect magic to what stands in its box), the end at `-23` (22 ticks of life after the start).
use crate::area::{creatures_in, players_in, species_name};
use crate::cow::*;
use crate::goals::Events;
use crate::hostile::PlayerState;
use crate::targeting::Who;
use crate::tame::Owner;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// A fang and the entity that summoned it (`EvokerFangs.owner`, a reference: gone when the owner is despawned).
#[derive(Component, Clone, Copy, Debug)]
pub struct Fang {
    pub owner: Entity,
}

/// `new EvokerFangs(level, x, y, z, yaw, warmup, owner)`: the three deadlines are armed now, nothing runs in between.
pub fn spawn_fang(w: &mut World, at: Vec3, warmup: i32, owner: Entity) -> Entity {
    let e = w.spawn((Pos(at), OwnTime, Fang { owner })).id();
    after(w, e, warmup + 1, start);
    after(w, e, warmup + 8, bite);
    after(w, e, warmup + 23, end);
    e
}

/// `EvokerFangs.tick` at `warmup` 0 after the countdown: the start (`broadcastEntityEvent` 4, once).
fn start(w: &mut World, e: Entity) {
    w.resource_mut::<Events>().host.entity_event(e, 4);
}

fn end(w: &mut World, e: Entity) {
    w.despawn(e);
}

/// The box of a fang: 0.5 wide (0.45 each side once inflated by 0.2 sideways), 0.8 high.
fn fang_box(at: Vec3) -> Aabb {
    let h = 0.45;
    Aabb::new(Vec3::new(at.x - h, at.y, at.z - h), Vec3::new(at.x + h, at.y + 0.8, at.z + h))
}

fn is_illager(name: &str) -> bool {
    matches!(name, "minecraft:evoker" | "minecraft:illusioner" | "minecraft:pillager" | "minecraft:vindicator")
}

/// `EvokerFangs.dealDamageTo` for a creature: no owner present hurts everyone; with one, an illager owner spares the illagers and
/// the vexes whose owner is an illager (`isAlliedTo`, no teams in the ECS). Players are never allied here (no teams).
fn allied(w: &World, owner: Option<Entity>, target: Entity) -> bool {
    let Some(owner) = owner else { return false };
    if !is_illager(species_name(w, owner)) {
        return false;
    }
    let name = species_name(w, target);
    if is_illager(name) {
        return true;
    }
    name == "minecraft:vex" && w.get::<Owner>(target).and_then(|o| o.0).is_some_and(|who| matches!(who, Who::Entity(r) if is_illager(species_name(w, r))))
}

/// The bite (`warmup == -8`): 6 of indirect magic (the owner still there) or of magic (no owner) to each living creature and
/// player in the box, the owner and its allies excepted.
fn bite(w: &mut World, e: Entity) {
    let (Some(at), Some(owner)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Fang>(e).map(|f| f.owner)) else { return };
    let b = fang_box(at);
    let owner = w.get_entity(owner).is_ok().then_some(owner);
    let by = owner.map(Who::Entity);
    let from = Some((at.x, at.z));
    let victims: Vec<Entity> = creatures_in(w, b, e)
        .into_iter()
        .filter(|&c| Some(c) != owner && w.get::<Health>(c).is_some_and(|h| h.0 > 0.0) && !allied(w, owner, c))
        .collect();
    for v in victims {
        w.resource_mut::<DamageQueue>().0.push(Hit { target: v, amount: 6.0, cause: Cause::Magic, from, by, direct: false });
    }
    let tick = w.resource::<Clock>().0;
    for id in players_in(w, b) {
        w.resource_mut::<PlayerState>().hurt(tick, id, 6.0, (at.x, at.z), by);
    }
}
