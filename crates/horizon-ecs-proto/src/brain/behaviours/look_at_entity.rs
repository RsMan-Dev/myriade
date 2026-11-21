//! `LookAtEntity` (`villager_brain.rs:2084-2110`, the kids of `getMinimalLookBehavior` and `getFullLookBehavior`): with no look target
//! and a visible entity within 8 blocks that the entry names (by species, or a player), the nearest becomes the look target at once.
//! The species names and the player flag are the entry's `Params`.
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::rules::look_at_entity as rules;
use crate::targeting::info;

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:look_at_entity", mems: &[&LOOK_TARGET, &VISIBLE_MOBS], one_shot: true, can_start, ..Behaviour::DEFAULT };

pub struct Params {
    /// The species looked at, by name (`minecraft:villager`...).
    pub species: &'static [&'static str],
    pub players: bool,
}

/// The id of a species by its name, from the table of the world.
pub fn species_id(w: &World, name: &str) -> Option<u8> {
    w.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).map(|s| s.id)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    if present(w, e, &LOOK_TARGET) {
        return false;
    }
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let ids: Vec<u8> = p.species.iter().filter_map(|n| species_id(w, n)).collect();
    let found = find_closest(w, e, |w, who| {
        info(w, who).is_some_and(|i| (if i.player { p.players } else { ids.contains(&i.species) }) && rules::in_reach(d2(pos, i.pos)))
    });
    let Some(who) = found else { return false };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    true
}
