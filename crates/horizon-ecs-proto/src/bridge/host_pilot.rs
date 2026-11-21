//! The pilot's input of the host's vehicles, by `Entity` (`pilot.rs`): the host sets it when the keys change and clears it when the rider
//! leaves or disconnects.
use crate::bridge::host::EcsWorld;
use crate::pilot::{self, PilotInput};
use crate::targeting::Who;
use bevy_ecs::prelude::*;

impl EcsWorld {
    /// `who` steers `vehicle` with this input. False, and nothing written, when `vehicle` already holds it.
    #[allow(clippy::too_many_arguments)]
    pub fn pilot_input(&mut self, vehicle: Entity, who: Who, forward: f32, strafe: f32, yaw: f32, pitch: f32, jump: f32, sprint: bool, sneak: bool, steer: bool) -> bool {
        pilot::set(self.world_mut(), vehicle, PilotInput { who, forward, strafe, yaw, pitch, jump, sprint, sneak, steer, keys: pilot::keys_of(forward, strafe) })
    }

    /// The pilot left `vehicle`: its input goes and its keys are back to rest.
    pub fn pilot_clear(&mut self, vehicle: Entity) {
        pilot::clear(self.world_mut(), vehicle);
    }
}
