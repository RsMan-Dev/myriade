//! `Mount.create(speed)` (`PiglinAi` ride activity): with a ride target in memory, no walk target and not a passenger, the entity
//! rides the target when it stands within a block, else walks to it at `speed` (close: 1 block). Vanilla `Mount`, one shot.
use crate::brain::*;
use crate::cow::Pos;
use crate::mount::{self, Riding};
use crate::rules::mount as rules;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:mount",
    mems: &[&LOOK_TARGET, &WALK_TARGET, &RIDE_TARGET],
    requires: &[Cap::Brain, Cap::Locomotion],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub speed: f32,
}

fn ride_target(w: &World, e: Entity) -> Option<Entity> {
    match get(w, e, &RIDE_TARGET) {
        Some(MemValue::Who(Who::Entity(vehicle))) => Some(*vehicle),
        _ => None,
    }
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    w.get::<Riding>(e).is_none() && !present(w, e, &WALK_TARGET) && ride_target(w, e).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some(vehicle) = ride_target(w, e) else { return };
    let Some(i) = info(w, Who::Entity(vehicle)) else { return };
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return };
    if rules::dist_sq(i.pos, me) < 1.0 {
        mount::ride(w, vehicle, Who::Entity(e));
        return;
    }
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(Who::Entity(vehicle), true)), None);
    let walk = WalkTarget { target: Tracker::Entity(Who::Entity(vehicle), false),speed: c.get::<Params>().speed, close: 1 };
    set(w, e, &WALK_TARGET, MemValue::Walk(walk), None);
}
