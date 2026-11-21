//! `VisibleVillagerBabies` (`villager_brain.rs`, `sense_babies`): the babies of this species among the visible mobs, the sight test of
//! `Sensor.isEntityTargetable` included, written to `NEARBY_BABIES` (erased when empty). Scanned every 20 ticks.
use crate::behaviours::nearby_babies::NEARBY_BABIES;
use crate::brain::*;
use crate::cow::SpeciesId;
use crate::targeting;
use bevy_ecs::prelude::*;

pub static SENSOR: Sensor = Sensor { name: "minecraft:villager_babies", mems: &[&NEARBY_BABIES], scan, ..Sensor::DEFAULT };

fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let own = w.get::<SpeciesId>(e).map(|s| s.0);
    let babies = find_all(w, e, |w, who| targeting::info(w, who).is_some_and(|i| i.baby && Some(i.species) == own));
    set(w, e, &NEARBY_BABIES, MemValue::Whos(babies), None);
}
