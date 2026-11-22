//! `EraseMemoryIf` (`hoglin` avoid and fight activities): a one-shot that forgets a memory (`forget`) while the predicate `when` holds.
//! It is asked each time the activity is picked; the hoglin's `wantsToStopFleeing` ends its `AVOID_TARGET` this way.
use crate::behaviours::Cap;
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:erase_memory_if", mems: &[], requires: &[Cap::Brain], one_shot: true, can_start, start, ..Behaviour::DEFAULT };

pub struct Params {
    pub when: fn(&World, Entity) -> bool,
    pub forget: &'static MemKey,
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    (c.get::<Params>().when)(w, e)
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    erase(w, e, c.get::<Params>().forget);
}
