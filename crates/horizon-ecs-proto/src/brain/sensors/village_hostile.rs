//! `VillagerHostiles` (`hostile_sensor.rs`): the nearest visible creature whose type is in `types`, within that type's distance.
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::targeting::{info, Who};

pub static NEAREST_HOSTILE: MemKey = MemKey::new("minecraft:nearest_hostile", Shape::Who);

pub static SENSOR: Sensor = Sensor { name: "minecraft:villager_hostiles", mems: &[&NEAREST_HOSTILE], scan, ..Sensor::DEFAULT };

/// The creature types and their distance in blocks (`vanilla_villager_distance`).
pub struct Params {
    pub types: &'static [(&'static str, f64)],
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let types = c.get::<Params>().types;
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let hostile = find_closest(w, e, |w, who| match who {
        Who::Entity(t) => {
            let Some(name) = w.get::<TypeName>(t) else { return false };
            let Some(&(_, dist)) = types.iter().find(|(t, _)| *t == name.0) else { return false };
            info(w, who).is_some_and(|i| d2(pos, i.pos) <= dist * dist)
        }
        Who::Player(_) => false,
    });
    match hostile {
        Some(who) => set(w, e, &NEAREST_HOSTILE, MemValue::Who(who), None),
        None => erase(w, e, &NEAREST_HOSTILE),
    }
}
