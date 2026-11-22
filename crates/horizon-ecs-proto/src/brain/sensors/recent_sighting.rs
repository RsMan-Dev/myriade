//! `recent_sighting` (`GolemSensor`, `villager_brain.rs` `sense_golem`): a mob the species' predicate accepts among the `MOBS` sets
//! `GOLEM_RECENT` for 599 ticks. Scanned every 200 ticks.
use crate::behaviours::golem_recent::GOLEM_RECENT;
use crate::brain::*;
use crate::targeting::Who;

/// The predicate of the species' configuration: the seen mobs it accepts.
pub struct Params {
    pub seen: fn(&World, Entity) -> bool,
}

pub static SENSOR: Sensor = Sensor { name: "minecraft:recent_sighting", mems: &[&GOLEM_RECENT], scan, ..Sensor::DEFAULT };

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let seen = match get(w, e, &MOBS) {
        Some(MemValue::Whos(l)) => l.iter().any(|who| matches!(who, Who::Entity(o) if (p.seen)(w, *o))),
        _ => false,
    };
    if seen {
        set(w, e, &GOLEM_RECENT, MemValue::Unit, Some(599));
    }
}
