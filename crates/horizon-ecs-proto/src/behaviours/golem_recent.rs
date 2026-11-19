//! Capteur : un golem d'une espèce donnée vu parmi les `MOBS` ; la mémoire dure 599 ticks après la dernière vue.
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::SpeciesId;

pub struct Params {
    pub species: SpeciesId,
}

pub static GOLEM_RECENT: MemKey = MemKey::new("minecraft:golem_detected_recently", Shape::Unit);

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let seen = match get(w, e, &MOBS) {
        Some(MemValue::Whos(l)) => l.iter().any(|who| matches!(who, Who::Entity(o) if w.get::<SpeciesId>(*o).is_some_and(|s| s.0 == p.species.0))),
        _ => false,
    };
    if seen {
        set(w, e, &GOLEM_RECENT, MemValue::Unit, Some(599));
    }
}

pub static SENSOR: Sensor = Sensor { name: "golem_recent", mems: &[&GOLEM_RECENT], scan, ..Sensor::DEFAULT };
