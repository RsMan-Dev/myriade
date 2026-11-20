//! `AxolotlAttackablesSensor` (`Sensor::AxolotlAttackables`, `animal_brain.rs:2436-2452`): every scan, the nearest visible creature that
//! is within `range` blocks, in water, that is alive and not playing dead, and that is either always hostile to the species (the tag
//! `always`) or a hunted prey (the tag `hunt`) while there is no hunting cooldown, is the `nearest_attackable`; erased otherwise.
//! The tags and the range are the species' (`Params`). Players are never taken (the `Mob` reads `Who::Mob` only).
use crate::behaviours::Cap;
use crate::brain::sensors::adult::species_has;
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::targeting::{info, Who};

pub use super::attackable::NEAREST_ATTACKABLE;
/// `HAS_HUNTING_COOLDOWN`: after a hunt, the prey tag is not taken for a while (written by the behaviours that end a fight).
pub static HAS_HUNTING_COOLDOWN: MemKey = MemKey::new("minecraft:has_hunting_cooldown", Shape::Bool);
pub static SENSOR: Sensor = Sensor { name: "minecraft:nearest_attackable", mems: &[&NEAREST_ATTACKABLE, &HAS_HUNTING_COOLDOWN], scan, requires: &[Cap::Targets], ..Sensor::DEFAULT };

pub struct Params {
    pub always: &'static str,
    pub hunt: &'static str,
    pub range: f64,
}
impl Params {
    pub const DEFAULT: Params = Params { always: "minecraft:axolotl_always_hostiles", hunt: "minecraft:axolotl_hunt_targets", range: 8.0 };
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let cooldown = present(w, e, &HAS_HUNTING_COOLDOWN);
    let Some(me) = w.get::<Pos>(e).map(|q| q.0) else { return };
    let found = find_closest(w, e, |w, who| {
        let Who::Entity(t) = who else { return false };
        let (Some(i), Some(f)) = (info(w, who), w.get::<FluidState>(t)) else { return false };
        d2(me, i.pos) <= p.range * p.range
            && f.in_water
            && !crate::brain::behaviours::play_dead::is_playing_dead(w, t)
            && (species_has(w, p.always, i.species) || species_has(w, p.hunt, i.species) && !cooldown)
    });
    match found {
        Some(who) => set(w, e, &NEAREST_ATTACKABLE, MemValue::Who(who), None),
        None => erase(w, e, &NEAREST_ATTACKABLE),
    }
}
