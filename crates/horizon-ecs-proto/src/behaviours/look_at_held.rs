//! `LookAtLoved` (`mobs/piglin.rs:1693-1728`): the nearest player within `range` blocks that holds one of the `items` in its
//! main hand is looked at. The search runs when the selector asks this goal; the look is the head's (`goals::look_at`).
//! The reach is 8 blocks while riding (`Activity::Ride`), and the goal is off while an `Avoid` is remembered (its activity).
use crate::goals;
use super::*;
use crate::{cow::*, goals::*};
use crate::mount::Riding;
use crate::behaviours::walk_away::Avoid;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub items: &'static [&'static str],
    pub range: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { items: &[], range: 14.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Held(pub &'static Params);

pub static MODULE: Module = Module {
    name: "look_at_held",
    flags: LOOK,
    equip,
    can_use,
    start,
    requires: &[Cap::Body, Cap::Goals, Cap::Head],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Held(params::<Params>(p)));
}
/// The eye of the player to watch: none while avoiding (`Avoid`), 8 blocks while riding, `range` otherwise.
fn watched(w: &World, e: Entity) -> Option<[f64; 3]> {
    if w.get::<Avoid>(e).is_some_and(|a| now_of(w, e) < a.until) {
        return None;
    }
    let p = w.get::<Held>(e).unwrap().0;
    let range = if w.get::<Riding>(e).is_some() { 8.0 } else { p.range };
    goals::nearest_player_holding(w, e, p.items, range)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    watched(w, e).is_some()
}
/// The head turns toward the player that holds the item.
fn start(w: &mut World, e: Entity, _: Inst) {
    if let Some(at) = watched(w, e) {
        goals::look_at(w, e, at);
    }
}
