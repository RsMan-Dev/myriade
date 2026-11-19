//! Capteur : l'hostile le plus proche qui se trouve dans la distance de fuite de son espèce. La distance vient de la
//! table `map` (ex. `neoforge:acceptable_villager_distances`), chargée depuis les données : rien n'est recopié ici.
//! Une entité sans distance dans la table n'est pas une menace.
use crate::targeting::Who;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::{Index, Pos, SpeciesId};
use std::collections::BTreeMap;

pub struct Params {
    pub memory: &'static MemKey,
    pub map: &'static str,
}

/// Les tables de distances par nom de table, puis par nom d'espèce (`minecraft:player` pour les joueurs).
#[derive(Default)]
#[derive(Resource)]
pub struct DistanceMaps(pub BTreeMap<String, BTreeMap<String, f64>>);
impl DistanceMaps {
    pub fn get(&self, map: &str, species: &str) -> Option<f64> {
        self.0.get(map)?.get(species).copied()
    }
    pub fn max(&self, map: &str) -> f64 {
        self.0.get(map).and_then(|m| m.values().copied().reduce(f64::max)).unwrap_or(0.0)
    }
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(at) = w.get::<Pos>(e).map(|q| q.0) else { return };
    let maps = w.resource::<DistanceMaps>();
    let reach = maps.max(p.map);
    let species = &crate::species::registry().species;
    let mut found: Vec<(f64, Entity)> = Vec::new();
    w.resource::<Index>().for_radius(at.x, at.z, reach, |o| {
        let (Some(kind), Some(q)) = (w.get::<SpeciesId>(o), w.get::<Pos>(o)) else { return };
        let Some(limit) = maps.get(p.map, species[kind.0 as usize].name) else { return };
        let d2 = (q.0.x - at.x).powi(2) + (q.0.y - at.y).powi(2) + (q.0.z - at.z).powi(2);
        if d2 <= limit * limit {
            found.push((d2, o));
        }
    });
    match found.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)) {
        Some((_, o)) => set(w, e, p.memory, MemValue::Who(Who::Entity(o)), None),
        None => erase(w, e, p.memory),
    }
}

pub static SENSOR: Sensor = Sensor { name: "hostiles_near", scan, ..Sensor::DEFAULT };
