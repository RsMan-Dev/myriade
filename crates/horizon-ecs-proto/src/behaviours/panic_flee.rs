//! Le villageois panique quand il est touché (`HURT_BY`) ou quand une menace proche est en mémoire (`panic_condition`,
//! `villager_brain.rs:3208`, `PanicTrigger` `1485-1500`). Ce module écrit `IS_PANICKING` ; la fuite elle-même est
//! `brain::behaviours::walk_away`, configurée par l'espèce. La mémoire de menace est un paramètre (la condition Mob est un OU).
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_panic",
    mems: &[&IS_PANICKING, &HURT_BY],
    passive: true,
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub threat: &'static MemKey,
}

fn threatened(w: &World, e: Entity, c: &Ctx) -> bool {
    present(w, e, &HURT_BY) || present(w, e, c.get::<Params>().threat)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    threatened(w, e, c)
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    set(w, e, &IS_PANICKING, MemValue::Bool(true), None);
}

fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    threatened(w, e, c)
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &IS_PANICKING);
}
