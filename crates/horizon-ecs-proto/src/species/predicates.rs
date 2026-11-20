//! The named predicates of the base (`Cond::Is("horizon:...")`): each reads components of the entity, never a species.
//! A mod adds its own with `Builder::predicate("mymod:name", f)` (a plain `fn(&World, Entity) -> bool`).
use crate::behaviours::Pred;
use crate::cow::{Bounds, Burning, Contact, FluidState, GrowAt, Index, Pos, SpeciesId};
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
/// `ocelot:trusting`: a player fed it (`cat_state::Trusting`).
fn ocelot_trusting(w: &World, e: Entity) -> bool {
    w.get::<crate::behaviours::cat_state::Trusting>(e).is_some()
}
/// `horizon:is_tame`: it has an owner (`tame::Owner`).
fn is_tame(w: &World, e: Entity) -> bool {
    w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0.is_some())
}
/// `horizon:in_village`: at most one section from a village point (`ServerLevel.isVillage`).
fn in_village(w: &World, e: Entity) -> bool {
    let (Some(p), Some(r)) = (w.get::<crate::cow::Pos>(e), w.get_resource::<crate::poi::PoiRegistry>()) else { return false };
    r.is_village([p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32])
}
/// `horizon:has_rider`: something sits on it (`mount::Mount`).
fn has_rider(w: &World, e: Entity) -> bool {
    w.get::<crate::mount::Mount>(e).is_some_and(|m| m.rider.is_some())
}
/// `horizon:is_mounted`: it rides something (`mount::Riding`).
fn is_mounted(w: &World, e: Entity) -> bool {
    w.get::<crate::mount::Riding>(e).is_some()
}
/// `horizon:is_leashed`: it is on a lead (`leash::Leash`).
fn is_leashed(w: &World, e: Entity) -> bool {
    crate::leash::is_leashed(w, e)
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
/// `bee:calm` (`BaseBeeGoal`): not angry. `bee:can_target` (`Bee.canTarget`): angry and not yet stung.
fn bee_calm(w: &World, e: Entity) -> bool {
    !crate::behaviours::neutral_anger::is_angry(w, e)
}
fn bee_can_target(w: &World, e: Entity) -> bool {
    crate::behaviours::neutral_anger::is_angry(w, e) && !w.get::<crate::behaviours::bee::Bee>(e).is_some_and(|b| b.stung)
}
/// `polar_bear:cub_nearby` (`polar_bear.rs:63-69`): a cub of the same species (a baby, the bear excluded) whose box meets the bear's box
/// inflated by (8, 4, 8). The species is compared by id, not by name.
fn polar_bear_cub_nearby(w: &World, e: Entity) -> bool {
    let (Some(p), Some(b), Some(s)) = (w.get::<Pos>(e), w.get::<Bounds>(e), w.get::<SpeciesId>(e)) else { return false };
    let area = b.0.inflate(horizon_core::geometry::Vec3::new(8.0, 4.0, 8.0));
    let mut found = false;
    w.resource::<Index>().for_radius(p.0.x, p.0.z, 10.0, |c| {
        found |= c != e
            && w.get::<SpeciesId>(c).is_some_and(|x| x.0 == s.0)
            && w.get::<GrowAt>(c).is_some()
            && w.get::<Bounds>(c).is_some_and(|cb| cb.0.intersects(area));
    });
    found
}
/// `rabbit:killer`: the killer bunny of `Rabbit.Variant.EVIL`. The ECS has no coats yet: no animal is one (DOUTE: a coat component).
fn rabbit_killer(_: &World, _: Entity) -> bool {
    false
}
use bevy_ecs::prelude::*;

/// The feet of a point stand in water (`Entity.isInWater` read from the fluid of the block).
fn water_at(w: &World, p: horizon_core::geometry::Vec3) -> bool {
    let mw = w.resource::<crate::cow::Level>().mw.0;
    mw.facts.get(mw.world.block(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)).fluid == horizon_client_lib::blocks::Fluid::Water
}
/// `drowned:ok_target` (`Drowned.okTarget`): at night, or when the target is in water.
fn drowned_ok_target(w: &World, e: Entity) -> bool {
    if !is_day(w, e) {
        return true;
    }
    w.get::<crate::targeting::Target>(e).and_then(|t| crate::targeting::info(w, t.who)).is_some_and(|i| water_at(w, i.pos))
}
/// `drowned:player_ok`: the same rule for the nearest player (the `Mob` reads `selected_player`): night, or that player in water.
fn drowned_player_ok(w: &World, e: Entity) -> bool {
    if !is_day(w, e) {
        return true;
    }
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    w.resource::<crate::goals::Online>()
        .players()
        .map(|p| (((p.x - me.x).powi(2) + (p.y - me.y).powi(2) + (p.z - me.z).powi(2)), p))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .is_some_and(|(_, p)| water_at(w, horizon_core::geometry::Vec3::new(p.x, p.y, p.z)))
}
pub static BUILTIN: &[(&str, Pred)] = &[
    ("horizon:is_baby", is_baby),
    ("horizon:is_adult", is_adult),
    ("horizon:is_day", is_day),
    ("horizon:is_night", is_night),
    ("horizon:in_water", in_water),
    ("horizon:on_ground", on_ground),
    ("horizon:on_fire", on_fire),
    ("horizon:is_tame", is_tame),
    ("horizon:in_village", in_village),
    ("horizon:has_rider", has_rider),
    ("horizon:is_mounted", is_mounted),
    ("horizon:is_sitting", is_sitting),
    ("horizon:is_leashed", is_leashed),
    ("ocelot:trusting", ocelot_trusting),
    ("panda:acts", panda_acts),
    ("panda:avoids", panda_avoids),
    ("armadillo:idle", armadillo_idle),
    ("neutral:angry", neutral_angry),
    ("bee:calm", bee_calm),
    ("bee:can_target", bee_can_target),
    ("polar_bear:cub_nearby", polar_bear_cub_nearby),
    ("rabbit:killer", rabbit_killer),
    ("drowned:ok_target", drowned_ok_target),
    ("drowned:player_ok", drowned_player_ok),
];
