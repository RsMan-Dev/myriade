//! `VillagerPanicTrigger` golem call (`villager_brain.rs:2695`): while the villager is hurt or sees a hostile (the panic condition), every
//! 100 ticks of the game clock it asks for an iron golem with 3 wanting villagers (`spawnGolemIfNeeded(.., 3)`). It runs only while the
//! condition holds (60 ticks per entry), so nothing is read at rest.
use crate::behaviours::summon_defender::{check, PANIC};
use crate::brain::sensors::village_hostile;
use crate::brain::*;
use crate::cow::now_of;
use bevy_ecs::prelude::*;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_panic_golem",
    mems: &[&HURT_BY, &village_hostile::NEAREST_HOSTILE],
    can_start: condition,
    tick,
    ..Behaviour::DEFAULT
};

fn condition(w: &mut World, e: Entity, _: &Ctx) -> bool {
    present(w, e, &HURT_BY) || present(w, e, &village_hostile::NEAREST_HOSTILE)
}

fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if !condition(w, e, c) {
        return false;
    }
    if now_of(w, e) % 100 == 0 {
        check(w, e, &PANIC);
    }
    true
}
