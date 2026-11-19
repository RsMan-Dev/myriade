//! Flight from repellents (`PiglinAvoidRepellent`, `mobs/piglin.rs:1640-1690`): a block of the list, or a creature of the
//! list, within `horizontal` blocks (`vertical` up and down) makes the animal remember that spot as its `Avoid` point, and
//! hands the walk to `walk_away`. The search runs when the selector asks this goal, and `walk_away` is started by request.
use crate::goals;
use super::walk_away::{Avoid, MODULE as WALK_AWAY};
use super::*;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The blocks and the species that repel, by name.
    pub blocks: &'static [&'static str],
    pub species: &'static [&'static str],
    pub horizontal: f64,
    pub vertical: i32,
    /// Ticks the spot stays remembered (the avoid ttl).
    pub ttl: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { blocks: &[], species: &[], horizontal: 8.0, vertical: 4, ttl: 120 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Flee(pub Params);

pub static MODULE: Module = Module {
    name: "flee_repellent",
    flags: 0,
    equip,
    can_use,
    start,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Flee(*params::<Params>(p)));
}
/// `canUse`: a repellent within reach.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let f = w.get::<Flee>(e).unwrap().0;
    repellent_near(w, e, &f).is_some()
}
/// The spot to keep away from is remembered, the walk away is requested, and this goal ends.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let f = w.get::<Flee>(e).unwrap().0;
    let Some(at) = repellent_near(w, e, &f) else { return };
    let until = now_of(w, e) + f.ttl;
    w.entity_mut(e).insert(Avoid { at, until });
    if let Some(slot) = w.get::<GoalSet>(e).and_then(|g| g.slot_of(&WALK_AWAY)) {
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
/// The point of the nearest repellent (a block of the list, or a creature of the list) within reach.
/// Socle absent: `goals::repellent_near` reads the blocks of the level and the species of the creatures.
fn repellent_near(w: &World, e: Entity, f: &Params) -> Option<[f64; 3]> {
    goals::repellent_near(w, e, f.blocks, f.species, f.horizontal, f.vertical)
}
