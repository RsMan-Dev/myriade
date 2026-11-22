//! `DismountOrSkipMounting.create(range, stop)` (`PiglinAi` ride activity, last): the creature's vehicle, or else its ride target, is
//! the candidate. When the candidate is not valid (alive and within `range` blocks) or `stop(entity, candidate)` says so, the
//! creature gets off and the ride target is forgotten. Vanilla `DismountOrSkipMounting`, one shot.
use crate::brain::*;
use crate::cow::Pos;
use crate::mount::{self, Riding};
use crate::rules::mount as rules;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:dismount_or_skip_mounting",
    mems: &[&RIDE_TARGET],
    requires: &[Cap::Brain],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

/// `range` in blocks; `stop` is `BiPredicate.test(entity, candidate)`.
pub struct Params {
    pub range: f64,
    pub stop: fn(&World, Entity, Entity) -> bool,
}

/// The vehicle, else the ride target (an entity).
fn candidate(w: &World, e: Entity) -> Option<Entity> {
    let vehicle = w.get::<Riding>(e).map(|r| r.0);
    vehicle.or_else(|| match get(w, e, &RIDE_TARGET) {
        Some(MemValue::Who(Who::Entity(target))) => Some(*target),
        _ => None,
    })
}

/// `isVehicleValid`: the candidate is in the world (its box known) and within `range` blocks of the creature.
fn valid(w: &World, e: Entity, vehicle: Entity, range: f64) -> bool {
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    info(w, Who::Entity(vehicle)).is_some_and(|i| rules::dist_sq(i.pos, me) < range * range)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(vehicle) = candidate(w, e) else { return false };
    !valid(w, e, vehicle, p.range) || (p.stop)(w, e, vehicle)
}

/// Gets off the vehicle (as its first or second rider) and forgets the ride target.
fn start(w: &mut World, e: Entity, _: &Ctx) {
    if let Some(vehicle) = w.get::<Riding>(e).map(|r| r.0) {
        if mount::rider_of(w, vehicle) == Some(Who::Entity(e)) {
            mount::dismount(w, vehicle);
        } else {
            mount::dismount_passenger(w, vehicle);
        }
    }
    erase(w, e, &RIDE_TARGET);
}
