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
    /// `SmoothSwimmingMoveControl(.., maxTurnY, .., outsideWater, ..)` out of the water (the frog: its attribute speed is 1.0, the control's 0.1 keeps its pace).
    pub smooth_land: Option<(f32, f32)>,
}
impl Params {
    pub const DEFAULT: Params = Params { climber: false, smooth_land: None };
}
pub static MODULE: Module = Module { name: "walk", requires: &[Cap::Locomotion], equip, tick: Some(super::climb::tick), ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: crate::behaviours::Inst, p: &'static (dyn std::any::Any + Sync)) {
    if let (Some(s), Some(mut d)) = (crate::behaviours::params::<Params>(p).smooth_land, w.get_mut::<crate::cow::Drive>(e)) {
        d.smooth_land = Some(s);
    }
    // The walk is the ground physics every body has: its marker is put by `Modes` when it is the active mode.
    if crate::behaviours::params::<Params>(p).climber {
        w.entity_mut(e).insert(super::climb::Climber);
        if let Some(mut d) = w.get_mut::<crate::cow::Drive>(e) {
            d.climber = true;
        }
    }
}
