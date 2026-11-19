//! Capteur : les bébés d'une espèce visibles (`VISIBLE_MOBS` filtrée par `baby`), pour le jeu et la réunion des parents.
//! Lit la liste du capteur `living` (même cadence) ; ne refait aucun parcours.
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::targeting;

pub struct Params {
    pub species: u8,
}

pub static NEARBY_BABIES: MemKey = MemKey::new("minecraft:nearby_babies", Shape::Whos);

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let visible = match get(w, e, &VISIBLE_MOBS) {
        Some(MemValue::Whos(l)) => l.clone(),
        _ => Vec::new(),
    };
    let babies: Vec<Who> = visible
        .into_iter()
        .filter(|&who| targeting::info(w, who).is_some_and(|i| i.baby && i.species == p.species))
        .collect();
    set(w, e, &NEARBY_BABIES, MemValue::Whos(babies), None);
}

pub static SENSOR: Sensor = Sensor { name: "nearby_babies", mems: &[&NEARBY_BABIES], scan, ..Sensor::DEFAULT };
