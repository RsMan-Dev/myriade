//! `SpearApproach(speed, distance)` (`PiglinAi` spear fight, `SpearAttack.SpearStatus`): with an attack target and a kinetic weapon in
//! the main hand, the entity looks at the target and walks to it at `speed` (times the mount's) while farther than `distance`
//! blocks and not using an item. Its end sets the status to charging. The status memory is shared by the three spear behaviours.
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::{d2, main_hand};
use crate::mount::Riding;
use crate::rules::spear_use as rules;
use crate::targeting::{info, Who};
use horizon_client_lib::spear::Kinetic;

pub const APPROACH: i64 = 0;
pub const CHARGING: i64 = 1;
pub const RETREAT: i64 = 2;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:spear_approach",
    mems: &[&SPEAR_STATUS, &WALK_TARGET, &LOOK_TARGET, &ATTACK_TARGET],
    requires: &[Cap::Brain, Cap::Locomotion, Cap::Targets],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

/// `speed` is the repositioning speed; `distance` the approach distance (blocks); `mount_speed` the charge speed of a mount.
pub struct Params {
    pub speed: f64,
    pub distance: f32,
    pub mount_speed: f64,
}

/// `ableToAttack`: the attack target, when the main hand holds a kinetic weapon (`SpearAttack` and `SpearRetreat` use it too).
pub fn able_to_attack(w: &World, e: Entity) -> Option<Who> {
    let Some(MemValue::Who(who)) = get(w, e, &ATTACK_TARGET) else { return None };
    let who = *who;
    main_hand(w, e).and_then(|s| Kinetic::of(&s.item)).map(|_| who)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    able_to_attack(w, e).is_some() && crate::using::state(w, e).is_none()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    set(w, e, &SPEAR_STATUS, MemValue::Int(APPROACH), None);
}

/// `canStillUse` (target and weapon, and farther than the distance), then the walk to the target.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(who) = able_to_attack(w, e) else { return false };
    let (Some(me), Some(i)) = (w.get::<Pos>(e).map(|x| x.0), info(w, who)) else { return false };
    if d2(me, i.pos) <= f64::from(p.distance * p.distance) {
        return false;
    }
    let (speed, _) = rules::mount_bonus(w.get::<Riding>(e).is_some(), p.mount_speed, 0);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    let walk = WalkTarget { target: Tracker::Entity(who, false), speed: (speed * p.speed) as f32, close: 0 };
    set(w, e, &WALK_TARGET, MemValue::Walk(walk), None);
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
    set(w, e, &SPEAR_STATUS, MemValue::Int(CHARGING), None);
}
