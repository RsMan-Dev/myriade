//! `SetLookAndInteract` (`villager_brain.rs:1210`): with no interaction target and a visible player within 4 blocks, the nearest player
//! becomes the interaction target and the look target, at once.
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::brain::*;
use crate::goals::d2;
use crate::rules::look_and_interact as rules;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:set_look_and_interact",
    mems: &[&INTERACTION_TARGET, &LOOK_TARGET, &VISIBLE_MOBS],
    one_shot: true,
    can_start,
    ..Behaviour::DEFAULT
};

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if present(w, e, &INTERACTION_TARGET) {
        return false;
    }
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let found = find_closest(w, e, |w, who| matches!(who, Who::Player(_)) && info(w, who).is_some_and(|i| rules::in_reach(d2(pos, i.pos))));
    let Some(who) = found else { return false };
    set(w, e, &INTERACTION_TARGET, MemValue::Who(who), None);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    true
}
