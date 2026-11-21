//! `EraseAttackBreeding` / `EraseDeadBreeding` (`axolotl_action`, `animal_brain.rs:2485-2500`): a one-shot of an activity that forgets a memory
//! (`forget`: the attack target, or the play-dead count) when the entity has a breed target (the entry's `needs` say so, and name
//! `forget` too, so that it has a slot). Forgetting the attack target also drops the target of the targeting and starts the hunting cooldown.
use crate::behaviours::Cap;
use crate::brain::sensors::attackables::HAS_HUNTING_COOLDOWN;
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:forget_memory", mems: &[&HAS_HUNTING_COOLDOWN], requires: &[Cap::Brain], one_shot: true, start, ..Behaviour::DEFAULT };

pub struct Params {
    pub forget: &'static MemKey,
    pub clear_target: bool,
    pub hunting_cooldown: Option<u64>,
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    erase(w, e, p.forget);
    if p.clear_target {
        crate::targeting::clear_target(w, e);
    }
    if let Some(t) = p.hunting_cooldown {
        set(w, e, &HAS_HUNTING_COOLDOWN, MemValue::Bool(true), Some(t));
    }
}
