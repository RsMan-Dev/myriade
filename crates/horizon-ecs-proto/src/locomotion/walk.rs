//! The walk: `LivingEntity.travel` on the ground, `GroundPathNavigation`, `MoveControl`. Its systems are those of `cow.rs`
//! (`travel`, `move_control`, `pathfind`, `follow`), filtered by `WalkMode`; the entity data it reads (`NavParams`, `FluidParams`,
//! `Body`) are the capabilities of the body, so the mode has no state of its own.
use crate::behaviours::{Cap, Module};
use crate::locomotion::WalkMode;
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Params = Params {};
}
pub static MODULE: Module = Module { name: "walk", requires: &[Cap::Locomotion], equip, ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: crate::behaviours::Inst, _: &(dyn std::any::Any + Sync)) {
    // Active from the start (`locomotion_bundle` has it already: the first tick of a new animal is not changed).
    if w.get::<WalkMode>(e).is_none() && w.get::<crate::locomotion::SwimMode>(e).is_none() {
        w.entity_mut(e).insert(WalkMode);
    }
}
