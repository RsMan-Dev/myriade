//! `VillagerPanicTrigger` (`VillagerGoalPackages.getCorePackage`): while the villager is hurt or sees a hostile (the panic condition) it
//! holds `PANIC_ON`, which keeps the panic activity; every 100 ticks of the game clock it asks for an iron golem with 3 wanting villagers
//! (`spawnGolemIfNeeded(.., 3)`). It runs only while the condition holds (60 ticks per entry), so nothing is read at rest.
use crate::behaviours::summon_defender::{check, PANIC};
use crate::brain::sensors::village_hostile;
use crate::brain::*;
use crate::cow::now_of;

pub static PANIC_ON: MemKey = MemKey::new("horizon:panic_on", Shape::Unit);

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_panic_golem",
    mems: &[&HURT_BY, &village_hostile::NEAREST_HOSTILE, &PANIC_ON, &WALK_TARGET],
    can_start: condition,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

fn condition(w: &mut World, e: Entity, _: &Ctx) -> bool {
    present(w, e, &HURT_BY) || present(w, e, &village_hostile::NEAREST_HOSTILE)
}

/// `VillagerPanicTrigger.start`: the panic activity takes the walk away (`setActiveActivityIfPossible`).
fn start(w: &mut World, e: Entity, _: &Ctx) {
    set(w, e, &PANIC_ON, MemValue::Unit, None);
    erase(w, e, &WALK_TARGET);
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

/// The panic lasts while the condition holds (`canStillUse`); the calm of `CalmDown` ends it.
fn stop(w: &mut World, e: Entity, c: &Ctx) {
    if !condition(w, e, c) {
        erase(w, e, &PANIC_ON);
    }
}
