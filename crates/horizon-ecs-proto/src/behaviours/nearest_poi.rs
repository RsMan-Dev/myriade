//! Capteur : garde dans une mémoire le point libre le plus proche d'un type accepté (ex. lit libre pour le repos),
//! dans un rayon. Planifié par `SensorEntry::new(&SENSOR, période)` ; il n'écrit que sur résultat.
use super::poi_claim::block_of;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::poi::PoiRegistry;
use horizon_world::poi::PoiType;

pub struct Params {
    pub memory: &'static MemKey,
    pub accept: fn(PoiType) -> bool,
    pub range: i32,
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let found = w.resource::<PoiRegistry>().in_range(block_of(w, e), p.range, true, p.accept);
    match found.first() {
        Some(&(poi, _)) => set(w, e, p.memory, MemValue::Block(poi), None),
        None => erase(w, e, p.memory),
    }
}

pub static SENSOR: Sensor = Sensor { name: "nearest_poi", scan, ..Sensor::DEFAULT };
