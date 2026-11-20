//! What a hoglin sees (`hoglin_sense`, `mobs/piglin.rs:239`): the adult piglins counted and the adult hoglins listed, both visible. The
//! counts feed `HoglinAi.piglinsOutnumberHoglins` (the headbutt) and the list is `NEAREST_VISIBLE_ADULT_HOGLINS`, the receivers of
//! `broadcastRetreat`. Scanned every 20 ticks with the other hoglin sensors.
use crate::behaviours::Cap;
use crate::brain::*;
use crate::targeting::{info, Who};

/// The number of adult piglins seen.
pub static PIGLINS: MemKey = MemKey::new("horizon:visible_adult_piglins", Shape::Int);
/// The adult hoglins seen.
pub static HOGLINS: MemKey = MemKey::new("horizon:visible_adult_hoglins", Shape::Whos);

pub static SENSOR: Sensor = Sensor { name: "horizon:hoglin_sight", mems: &[&PIGLINS, &HOGLINS], scan, requires: &[Cap::Targets], ..Sensor::DEFAULT };

fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let table = w.resource::<SpeciesTable>().0.clone();
    let id = |name: &str| table.iter().find(|s| s.name == name).map(|s| s.id);
    let (piglin, hoglin) = (id("minecraft:piglin"), id("minecraft:hoglin"));
    let adult = |w: &World, who: Who, sp: Option<u8>| !matches!(who, Who::Player(_)) && info(w, who).is_some_and(|i| !i.baby && Some(i.species) == sp);
    let piglins = find_all(w, e, |w, who| adult(w, who, piglin)).len();
    let hoglins = find_all(w, e, |w, who| adult(w, who, hoglin));
    set(w, e, &PIGLINS, MemValue::Int(piglins as i64), None);
    set(w, e, &HOGLINS, MemValue::Whos(hoglins), None);
}

/// `HoglinAi.piglinsOutnumberHoglins`: an adult sees more than one adult piglin more than adult hoglins.
pub fn piglins_outnumber(w: &World, e: Entity) -> bool {
    let n = |k: &MemKey| match get(w, e, k) {
        Some(MemValue::Int(n)) => *n,
        Some(MemValue::Whos(l)) => l.len() as i64,
        _ => 0,
    };
    n(&PIGLINS) > n(&HOGLINS) + 1
}
