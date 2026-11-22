//! `StopBeingAngryIfTargetDead` (`PiglinAi` and `PiglinBruteAi` core, javap): the anger memory is erased once its target is found dead.
//! A target not found is kept; a player counts only through `forgive_dead_players` (the forgive helper ends that anger).
use crate::behaviours::Cap;
use crate::brain::*;
use crate::targeting;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:stop_being_angry_if_target_dead",
    mems: &[&ANGRY_AT],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    match get(w, e, &ANGRY_AT) {
        Some(MemValue::Who(who)) => targeting::info(w, *who).is_some_and(|i| i.exempt && !i.player),
        _ => false,
    }
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &ANGRY_AT);
}
