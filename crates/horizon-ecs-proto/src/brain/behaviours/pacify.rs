//! `BecomePassiveIfMemoryPresent` (`piglin.rs:1852-1870`, `hoglin_repellent`): the entry's needs hold the memory that makes it
//! passive. For `Params.ttl` ticks `PACIFIED` is set and the attack target is dropped. Generic: the memory is the entry's need.
use crate::behaviours::Cap;
use crate::brain::*;

/// `HoglinAi.isPacified`: present while the entity is pacified; it expires by its time to live.
pub static PACIFIED: MemKey = MemKey::new("minecraft:pacified", Shape::Bool);

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:become_passive_if_memory_present",
    mems: &[&PACIFIED, &ATTACK_TARGET],
    requires: &[Cap::Brain, Cap::Targets],
    one_shot: true,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub ttl: u64,
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    set(w, e, &PACIFIED, MemValue::Bool(true), Some(c.get::<Params>().ttl));
    erase(w, e, &ATTACK_TARGET);
    // `BecomePassiveIfMemoryPresent` erases the attack target: the goals' own target goes with it.
    crate::targeting::clear_target(w, e);
}
