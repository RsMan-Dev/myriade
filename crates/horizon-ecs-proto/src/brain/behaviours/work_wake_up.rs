//! `WakeUp` du villageois (`villager_brain.rs:1399`, core prio 0) : un villageois couché qui n'est plus dans l'activité de repos
//! se réveille (`stop_sleeping`). Le lit est libéré par le retrait de `Sleeping`, comme `sleep_in_bed::stop`.
use crate::behaviours::bed::Sleeping;
use crate::brain::*;
use bevy_ecs::prelude::*;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:villager_wake_up", one_shot: true, can_start, start, ..Behaviour::DEFAULT };

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    w.get::<Sleeping>(e).is_some() && !is_active(w, e, "rest")
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<Sleeping>();
}
