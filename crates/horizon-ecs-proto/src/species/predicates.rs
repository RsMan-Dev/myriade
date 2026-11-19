//! The named predicates of the base (`Cond::Is("horizon:...")`): each reads components of the entity, never a species.
//! A mod adds its own with `Builder::predicate("mymod:name", f)` (a plain `fn(&World, Entity) -> bool`).
use crate::behaviours::Pred;
use crate::cow::{Burning, Contact, FluidState, GrowAt};
use crate::hostile::Daylight;

/// `horizon:is_baby`: it has not grown up yet.
fn is_baby(w: &World, e: Entity) -> bool {
    w.get::<GrowAt>(e).is_some()
}
fn is_adult(w: &World, e: Entity) -> bool {
    w.get::<GrowAt>(e).is_none()
}
/// `horizon:is_day` / `is_night`: the world's day (`Daylight`).
fn is_day(w: &World, _: Entity) -> bool {
    w.get_resource::<Daylight>().is_some_and(|d| d.0)
}
fn is_night(w: &World, _: Entity) -> bool {
    !is_day(w, Entity::PLACEHOLDER)
}
fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}
fn on_ground(w: &World, e: Entity) -> bool {
    w.get::<Contact>(e).is_some_and(|c| c.on_ground)
}
fn on_fire(w: &World, e: Entity) -> bool {
    w.get::<Burning>(e).is_some_and(|b| b.until > crate::cow::now_of(w, e))
}
/// `horizon:is_tame`: it has an owner (`tame::Owner`).
fn is_tame(w: &World, e: Entity) -> bool {
    w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some())
}
/// `horizon:is_sitting`: it is ordered to sit (the occupation `SITTING`).
fn is_sitting(w: &World, e: Entity) -> bool {
    crate::goals::is_occupied(w, e, crate::goals::OCC_SITTING)
}
use bevy_ecs::prelude::*;

pub static BUILTIN: &[(&str, Pred)] = &[
    ("horizon:is_baby", is_baby),
    ("horizon:is_adult", is_adult),
    ("horizon:is_day", is_day),
    ("horizon:is_night", is_night),
    ("horizon:in_water", in_water),
    ("horizon:on_ground", on_ground),
    ("horizon:on_fire", on_fire),
    ("horizon:is_tame", is_tame),
    ("horizon:is_sitting", is_sitting),
];
