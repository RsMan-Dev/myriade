//! `BackUpIfTooClose.create(distance, speed)` (`PiglinAi` fight): with no walk target, an attack target in sight and closer than
//! `distance` blocks, the entity looks at it. The strafe back (`MoveControl.strafe(-speed, 0)`) is the goal's
//! (`behaviours/back_up_if_too_close`): the brain has no move control, so only the look is written here.
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::d2;
use crate::rules::back_up_if_too_close as rules;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:back_up_if_too_close",
    mems: &[&WALK_TARGET, &LOOK_TARGET, &ATTACK_TARGET, &VISIBLE_MOBS],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

/// `distance` in blocks; `speed` is the strafe's, kept for the goal's pairing (not used by the brain); `when` is the
/// `triggerIf` predicate of vanilla (the crossbow in hand for a piglin).
pub struct Params {
    pub distance: f64,
    pub speed: f32,
    pub when: fn(&World, Entity) -> bool,
}

/// The attack target when it is in sight and closer than the distance.
fn too_close(w: &mut World, e: Entity, p: &Params) -> Option<Who> {
    let Some(MemValue::Who(who)) = get(w, e, &ATTACK_TARGET) else { return None };
    let who = *who;
    let (Some(me), Some(i)) = (w.get::<Pos>(e).map(|x| x.0), info(w, who)) else { return None };
    let seen = visible_contains(w, e, who);
    rules::too_close(p.distance, seen, d2(i.pos, me)).then_some(who)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    (p.when)(w, e) && !present(w, e, &WALK_TARGET) && too_close(w, e, p).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some(who) = too_close(w, e, c.get::<Params>()) else { return };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
}
