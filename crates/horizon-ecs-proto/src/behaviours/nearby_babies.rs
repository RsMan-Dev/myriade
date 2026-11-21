//! Capteur : les bébés d'une espèce visibles (`VISIBLE_MOBS` filtrée par `baby`), pour le jeu et la réunion des parents.
//! Lit la liste du capteur `living` (même cadence) ; ne refait aucun parcours.
use crate::targeting::Who;
use crate::brain::*;
use crate::cow::SpeciesId;
use bevy_ecs::prelude::*;
use crate::targeting;

pub struct Params {
    /// The species of the babies; `None`: the entity's own species (`SAME`, the villager's play).
    pub species: Option<u8>,
}
impl Params {
    pub const SAME: Params = Params { species: None };
}

pub static NEARBY_BABIES: MemKey = MemKey::new("minecraft:nearby_babies", Shape::Whos);

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let own = w.get::<SpeciesId>(e).map(|s| s.0);
    let want = c.get::<Params>().species.or(own);
    let visible = match get(w, e, &VISIBLE_MOBS) {
        Some(MemValue::Whos(l)) => l.clone(),
        _ => Vec::new(),
    };
    let babies: Vec<Who> = visible
        .into_iter()
        .filter(|&who| targeting::info(w, who).is_some_and(|i| i.baby && Some(i.species) == want))
        .collect();
    set(w, e, &NEARBY_BABIES, MemValue::Whos(babies), None);
}

pub static SENSOR: Sensor = Sensor { name: "nearby_babies", mems: &[&NEARBY_BABIES], scan, ..Sensor::DEFAULT };
