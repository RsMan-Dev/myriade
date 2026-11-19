//! `RepellentAndAdult` sensor: every scan, the nearest block of a tag within reach is `NEAREST_REPELLENT` (`find_hoglin_repellent`,
//! `mobs/piglin.rs:1826`), and the nearest visible adult of a species is `NEAREST_VISIBLE_ADULT` (`NEAREST_VISIBLE_ADULT_PIGLIN`).
//! Nothing found: the memory is erased. The labels (the tag, the species name, the reach) come from the entry's `Params`.
use crate::behaviours::Cap;
use crate::blocks::world_of;
use crate::brain::*;
use crate::cow::Pos;
use crate::targeting::{info, Who};

/// `MemoryModuleType.NEAREST_REPELLENT`: the position of the nearest block of the tag.
pub static NEAREST_REPELLENT: MemKey = MemKey::new("minecraft:nearest_repellent", Shape::Block);
/// The nearest visible adult of the species named by the entry's `Params`.
pub static NEAREST_VISIBLE_ADULT: MemKey = MemKey::new("minecraft:nearest_visible_adult", Shape::Who);

pub static SENSOR: Sensor = Sensor {
    name: "minecraft:repellent_and_adult",
    mems: &[&NEAREST_REPELLENT, &NEAREST_VISIBLE_ADULT],
    scan,
    requires: &[Cap::Targets],
    ..Sensor::DEFAULT
};

/// `block_tag` and `species` are empty to turn that half off. `horizontal` and `vertical` are the block search's reach.
pub struct Params {
    pub block_tag: &'static str,
    pub species: &'static str,
    pub horizontal: i32,
    pub vertical: i32,
}
impl Params {
    pub const DEFAULT: Params = Params { block_tag: "", species: "", horizontal: 8, vertical: 4 };
}

fn scan(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let at = [me.x.floor() as i32, me.y.floor() as i32, me.z.floor() as i32];
    if p.block_tag.is_empty() {
        erase(w, e, &NEAREST_REPELLENT);
    } else {
        let mw = world_of(w);
        match mw.nearest_block(at, p.horizontal, p.vertical, |_, name| mw.block_tag(p.block_tag, name)) {
            Some(spot) => set(w, e, &NEAREST_REPELLENT, MemValue::Block(spot), None),
            None => erase(w, e, &NEAREST_REPELLENT),
        }
    }
    if p.species.is_empty() {
        erase(w, e, &NEAREST_VISIBLE_ADULT);
        return;
    }
    let id = w.resource::<SpeciesTable>().0.iter().find(|s| s.name == p.species).map(|s| s.id);
    match find_closest(w, e, |w, who| adult_of(w, who, id)) {
        Some(who) => set(w, e, &NEAREST_VISIBLE_ADULT, MemValue::Who(who), None),
        None => erase(w, e, &NEAREST_VISIBLE_ADULT),
    }
}

fn adult_of(w: &World, who: Who, species: Option<u8>) -> bool {
    if matches!(who, Who::Player(_)) {
        return false;
    }
    let Some(i) = info(w, who) else { return false };
    !i.baby && Some(i.species) == species
}
