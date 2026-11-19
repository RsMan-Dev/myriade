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
/// `panda:acts` (`Panda.canPerformAction`): not on its back, rolling, sitting, eating or scared (`panda_state`).
fn panda_acts(w: &World, e: Entity) -> bool {
    crate::behaviours::panda_state::acts(w, e)
}
/// `panda:avoids` (`panda.rs:678`): a worried panda that acts.
fn panda_avoids(w: &World, e: Entity) -> bool {
    crate::behaviours::panda_state::gene(w, e) == crate::behaviours::panda_state::Gene::Worried && panda_acts(w, e)
}
/// `armadillo:idle` (`armadillo.rs:574`): not rolled up or scared (the state machine of `armadillo_state`).
fn armadillo_idle(w: &World, e: Entity) -> bool {
    crate::behaviours::armadillo_state::of(w, e) == crate::behaviours::armadillo_state::IDLE
}
/// `neutral:angry` (`tame::angry_at_player`): the entity is angry (`neutral_anger`).
fn neutral_angry(w: &World, e: Entity) -> bool {
    crate::behaviours::neutral_anger::is_angry(w, e)
}
/// `rabbit:killer`: the killer bunny of `Rabbit.Variant.EVIL`. The ECS has no coats yet: no animal is one (DOUTE: a coat component).
fn rabbit_killer(_: &World, _: Entity) -> bool {
    false
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
    ("panda:acts", panda_acts),
    ("panda:avoids", panda_avoids),
    ("armadillo:idle", armadillo_idle),
    ("neutral:angry", neutral_angry),
    ("rabbit:killer", rabbit_killer),
];
