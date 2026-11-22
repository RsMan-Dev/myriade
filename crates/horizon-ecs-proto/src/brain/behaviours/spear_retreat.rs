//! `SpearRetreat(speed)` (`PiglinAi` spear fight, status retreat): with a kinetic weapon and an attack target, not using an item, the
//! entity picks a spot away from the target (`SPEAR_FLEEING_POSITION`) and walks there while it looks at the target, for at most
//! 100 ticks (`SPEAR_FLEEING_TIME`). Its end erases the spear status, so the approach starts again. The 100-tick run is the entry's
//! duration in the brain (vanilla `Behavior` 100), not set here.
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::d2;
use crate::mount::Riding;
use crate::rules::spear_use as rules;
use crate::spots::Pick;
use crate::targeting::{info, Who};
use super::spear_approach::{able_to_attack, RETREAT};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:spear_retreat",
    mems: &[&SPEAR_STATUS, &SPEAR_FLEEING_POSITION, &SPEAR_FLEEING_TIME, &WALK_TARGET, &LOOK_TARGET, &ATTACK_TARGET],
    requires: &[Cap::Brain, Cap::Locomotion, Cap::Targets],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

/// `speed` is the walk speed; `mount_speed` and `mount_distance` as in `SpearAttack`.
pub struct Params {
    pub speed: f64,
    pub mount_speed: f64,
    pub mount_distance: i32,
}

/// The ticks of retreat before the walk is given up (`SpearRetreat` runs for 100 ticks).
const FLEEING_TICKS: i64 = 100;

/// `checkExtraStartConditions`: the target and weapon, not using an item, the status retreat, and a spot away from the target
/// (kept as the fleeing position).
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(who) = able_to_attack(w, e) else { return false };
    if crate::using::state(w, e).is_some() || int(w, e, &SPEAR_STATUS).unwrap_or(0) != RETREAT {
        return false;
    }
    let (Some(me), Some(i)) = (w.get::<Pos>(e).map(|x| x.0), info(w, who)) else { return false };
    let (_, bonus) = rules::mount_bonus(w.get::<Riding>(e).is_some(), p.mount_speed, p.mount_distance);
    let d = d2(me, i.pos).sqrt();
    let from = [i.pos.x, i.pos.y, i.pos.z];
    let (min_h, max_h) = ((9.0 + f64::from(bonus) - d).max(0.0), (11.0 + f64::from(bonus) - d).max(1.0));
    match pick(w, e, Pick::LandAway { min_h, max_h, v: 7, from }) {
        Some(at) => {
            set(w, e, &SPEAR_FLEEING_POSITION, MemValue::Look(Tracker::Point(at)), None);
            true
        }
        None => false,
    }
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    set(w, e, &SPEAR_FLEEING_TIME, MemValue::Int(0), None);
}

/// `canStillUse` (time under 100, a fleeing position, the walk not done, target and weapon), then the look and the walk.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let time = int(w, e, &SPEAR_FLEEING_TIME).unwrap_or(0);
    let Some(who) = able_to_attack(w, e) else { return false };
    if time >= FLEEING_TICKS || !present(w, e, &SPEAR_FLEEING_POSITION) || nav_done(w, e) {
        return false;
    }
    let (factor, _) = rules::mount_bonus(w.get::<Riding>(e).is_some(), p.mount_speed, p.mount_distance);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    set(w, e, &SPEAR_FLEEING_TIME, MemValue::Int(time + 1), None);
    let at = match get(w, e, &SPEAR_FLEEING_POSITION) {
        Some(MemValue::Look(Tracker::Point(at))) => Some(*at),
        _ => None,
    };
    if let Some(at) = at {
        let walk = WalkTarget { target: Tracker::Point(at), speed: (factor * p.speed) as f32, close: 0 };
        set(w, e, &WALK_TARGET, MemValue::Walk(walk), None);
    }
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
    crate::using::stop(w, e);
    erase(w, e, &SPEAR_FLEEING_TIME);
    erase(w, e, &SPEAR_FLEEING_POSITION);
    erase(w, e, &SPEAR_STATUS);
}
