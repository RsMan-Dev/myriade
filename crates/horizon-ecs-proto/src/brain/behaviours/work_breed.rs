//! `canBreed` du villageois (`villager.rs:980`) en condition `ready` de `make_love` (`VillagerMakeLove`, `villager_brain.rs:1969`) :
//! un adulte, pas endormi, qui tient 12 points de nourriture (son niveau et son inventaire).
use crate::behaviours::bed::Sleeping;
use crate::behaviours::food_stock;
use crate::behaviours::panda_state;
use crate::brain::*;
use crate::cow::Level;

pub fn ready(w: &World, e: Entity) -> bool {
    let Some(items) = w.resource::<Level>().mw.0.items else { return false };
    !panda_state::is_baby(w, e) && w.get::<Sleeping>(e).is_none() && food_stock::can_breed(w, e, items)
}
