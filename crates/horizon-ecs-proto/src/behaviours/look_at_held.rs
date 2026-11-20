//! `LookAtLoved` (`mobs/piglin.rs:1693-1728`): the nearest player within `range` blocks that holds one of the `items` in its
//! main hand is looked at. The search runs when the selector asks this goal; the look is the head's (`goals::look_at`).
use crate::goals;
use super::*;
use crate::{cow::*, goals::*};

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
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let p = w.get::<Held>(e).unwrap().0;
    goals::nearest_player_holding(w, e, p.items, p.range).is_some()
}
/// The head turns toward the player that holds the item.
fn start(w: &mut World, e: Entity, _: Inst) {
    let p = w.get::<Held>(e).unwrap().0;
    if let Some(at) = goals::nearest_player_holding(w, e, p.items, p.range) {
        goals::look_at(w, e, at);
    }
}
