//! `SpearAttack(charging, repositioning, radius)` (`PiglinAi` spear fight, status charging): the entity uses its kinetic weapon for
//! the weapon's use duration (`SPEAR_ENGAGE_TIME`), charges the target while it stands farther than `radius` blocks, and once in
//! reach or done with the walk, picks a spot to reposition to (`SPEAR_CHARGE_POSITION`). Its end sets the status to retreat.
//! Vanilla strikes nothing here: the hit comes from the use's end, which the item layer does.
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::{d2, main_hand};
use crate::mount::Riding;
use crate::rules::spear_use as rules;
use crate::spots::Pick;
use crate::targeting::{info, Who};
use horizon_client_lib::spear::Kinetic;
use horizon_core::geometry::Vec3;
use super::spear_approach::{able_to_attack, CHARGING, RETREAT};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:spear_attack",
    mems: &[&SPEAR_STATUS, &SPEAR_ENGAGE_TIME, &SPEAR_CHARGE_POSITION, &WALK_TARGET, &LOOK_TARGET, &ATTACK_TARGET],
    requires: &[Cap::Brain, Cap::Locomotion, Cap::Targets],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

/// `charging` and `repositioning` are the walk speeds; `radius` the reach that ends a charge (blocks); `mount_speed` and
/// `mount_distance` the mount's charge speed and distance bonus (`mount_bonus`: the bonus is 2 for a passenger in vanilla).
pub struct Params {
    pub charging: f64,
    pub repositioning: f64,
    pub radius: f32,
    pub mount_speed: f64,
    pub mount_distance: i32,
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    able_to_attack(w, e).is_some() && crate::using::state(w, e).is_none() && int(w, e, &SPEAR_STATUS).unwrap_or(0) == CHARGING
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let ticks = main_hand(w, e).and_then(|s| Kinetic::of(&s.item)).map_or(0, |k| k.damage_use_duration());
    set(w, e, &SPEAR_ENGAGE_TIME, MemValue::Int(i64::from(ticks)), None);
    erase(w, e, &SPEAR_CHARGE_POSITION);
    crate::using::start(w, e, false);
}

/// `canStillUse` (the engage time left and the target and weapon), then one step of the charge or of the reposition.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(who) = able_to_attack(w, e) else { return false };
    let engage = int(w, e, &SPEAR_ENGAGE_TIME).unwrap_or(0);
    if engage <= 0 {
        return false;
    }
    let (Some(me), Some(i)) = (w.get::<Pos>(e).map(|x| x.0), info(w, who)) else { return false };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    set(w, e, &SPEAR_ENGAGE_TIME, MemValue::Int(engage - 1), None);
    let (factor, bonus) = rules::mount_bonus(w.get::<Riding>(e).is_some(), p.mount_speed, p.mount_distance);
    let charge = match get(w, e, &SPEAR_CHARGE_POSITION) {
        Some(MemValue::Look(Tracker::Point(at))) => Some(*at),
        _ => None,
    };
    if let Some(at) = charge {
        let done = nav_done(w, e);
        walk(w, e, Tracker::Point(at), factor * p.repositioning);
        if done {
            erase(w, e, &SPEAR_CHARGE_POSITION);
        }
        return true;
    }
    let done = nav_done(w, e);
    walk(w, e, Tracker::Entity(who, false), factor * p.charging);
    let dist2 = d2(me, i.pos);
    if dist2 < f64::from(p.radius * p.radius) || done {
        reposition(w, e, i.pos, dist2.sqrt(), f64::from(bonus));
    }
    true
}

/// `getPosAway` from the target, in the band `(6 + bonus - d, 7 + bonus - d)`, 7 blocks up: the spot is kept as the charge position.
fn reposition(w: &mut World, e: Entity, target: Vec3, d: f64, bonus: f64) {
    let from = [target.x, target.y, target.z];
    if let Some(at) = pick(w, e, Pick::LandAway { min_h: 6.0 + bonus - d, max_h: 7.0 + bonus - d, v: 7, from }) {
        set(w, e, &SPEAR_CHARGE_POSITION, MemValue::Look(Tracker::Point(at)), None);
    }
}

fn walk(w: &mut World, e: Entity, target: Tracker, speed: f64) {
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target, speed: speed as f32, close: 0 }), None);
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
    crate::using::stop(w, e);
    erase(w, e, &SPEAR_CHARGE_POSITION);
    erase(w, e, &SPEAR_ENGAGE_TIME);
    set(w, e, &SPEAR_STATUS, MemValue::Int(RETREAT), None);
}
