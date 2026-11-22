//! `PiglinBruteAi.wasHurtBy` (core, a damage hook): an attacker the species does not skip becomes the anger target for `ttl` ticks, and
//! the walk that gave up on it is forgotten. The skipped kinds are the species' (`skip`), so no species name is written here.
use crate::behaviours::{Cap, Damaged};
use crate::brain::*;
use crate::targeting::Who;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:anger_at_attacker",
    mems: &[&ANGRY_AT, &CANT_REACH_WALK_TARGET_SINCE],
    requires: &[Cap::Brain, Cap::Targets],
    can_start: never,
    damaged,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// The attackers that are not angered (the piglin kinds, `AbstractPiglin`).
    pub skip: fn(&World, Who) -> bool,
    pub ttl: u64,
}

fn never(_: &mut World, _: Entity, _: &Ctx) -> bool {
    false
}

fn damaged(w: &mut World, e: Entity, c: &Ctx, d: &Damaged) {
    let p = c.get::<Params>();
    let Some(who) = d.who else { return };
    if (p.skip)(w, who) {
        return;
    }
    set(w, e, &ANGRY_AT, MemValue::Who(who), Some(p.ttl));
    erase(w, e, &CANT_REACH_WALK_TARGET_SINCE);
}
