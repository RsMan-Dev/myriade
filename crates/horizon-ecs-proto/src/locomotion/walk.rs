//! The walk: `LivingEntity.travel` on the ground, `GroundPathNavigation`, `MoveControl`. Its systems are those of `cow.rs`
//! (`travel`, `move_control`, `pathfind`, `follow`), filtered by `WalkMode`; the entity data it reads (`NavParams`, `FluidParams`,
//! `Body`) are the capabilities of the body, so the mode has no state of its own.
use crate::behaviours::{Cap, Module};
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// A wall climber (`WallClimberNavigation`, `Spider.isClimbing`): pressed against a wall it climbs it, and the climb goes on straight at the
    /// target when the path is over (`locomotion/climb.rs`).
    pub climber: bool,
}
impl Params {
    pub const DEFAULT: Params = Params { climber: false };
}
pub static MODULE: Module = Module { name: "walk", requires: &[Cap::Locomotion], equip, ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: crate::behaviours::Inst, p: &(dyn std::any::Any + Sync)) {
    // The walk is the ground physics every body has: its marker is put by `Modes` when it is the active mode.
    if crate::behaviours::params::<Params>(p).climber {
        w.entity_mut(e).insert(super::climb::Climber);
        if let Some(mut d) = w.get_mut::<crate::cow::Drive>(e) {
            d.climber = true;
        }
    }
}
