//! The walk: `LivingEntity.travel` on the ground, `GroundPathNavigation`, `MoveControl`. Its systems are those of `cow.rs`
//! (`travel`, `move_control`, `pathfind`, `follow`), filtered by `WalkMode`; the entity data it reads (`NavParams`, `FluidParams`,
//! `Body`) are the capabilities of the body, so the mode has no state of its own.
use crate::behaviours::{Cap, Module};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Params = Params {};
}
pub static MODULE: Module = Module { name: "walk", requires: &[Cap::Locomotion], equip, ..Module::DEFAULT };
fn equip(_: &mut World, _: Entity, _: crate::behaviours::Inst, _: &(dyn std::any::Any + Sync)) {
    // The walk is the ground physics every body has: its marker is put by `Modes` when it is the active mode.
}
