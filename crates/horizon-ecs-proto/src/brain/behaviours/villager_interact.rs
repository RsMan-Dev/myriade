//! `InteractWith` (`villager_brain.rs:1963-2005`): with no walk target and a visible mob of the kind the entry names, one such mob is
//! wanted; the nearest within 8 blocks becomes the interaction target (the breeding target when `breed`), is looked at and walked to
//! (speed 0.5, close 2). The kind is the entry's `Params`: a species by name, and in love for a breeding pair.
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::brain::behaviours::look_at_entity::species_id;
use crate::brain::*;
use crate::cow::*;
use crate::goals::d2;
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:interact_with",
    mems: &[&WALK_TARGET, &LOOK_TARGET, &VISIBLE_MOBS, &INTERACTION_TARGET, &BREED_TARGET],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub species: &'static str,
    /// Only a mob in love (`BreedVillager`).
    pub in_love: bool,
    /// The target goes to `breed_target` (`BreedVillager`), not `interaction_target`.
    pub breed: bool,
}

fn wanted(w: &World, who: Who, p: &Params, id: Option<u8>) -> bool {
    let Who::Entity(k) = who else { return false };
    info(w, who).is_some_and(|i| Some(i.species) == id) && (!p.in_love || crate::brain::behaviours::work_breed::ready(w, k))
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) || !present(w, e, &VISIBLE_MOBS) {
        return false;
    }
    let p = c.get::<Params>();
    let id = species_id(w, p.species);
    find_closest(w, e, |w, who| wanted(w, who, p, id)).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let p = c.get::<Params>();
    let id = species_id(w, p.species);
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let found = find_closest(w, e, |w, who| wanted(w, who, p, id) && info(w, who).is_some_and(|i| d2(pos, i.pos) <= 64.0));
    let Some(who) = found else { return };
    if p.breed {
        set(w, e, &BREED_TARGET, MemValue::Who(who), None);
    } else {
        set(w, e, &INTERACTION_TARGET, MemValue::Who(who), None);
    }
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Entity(who, false), speed: 0.5, close: 2 }), None);
}
