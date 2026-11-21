//! The pilot's input of the level's vehicles (`P bridge/host_pilot.rs`): ids in, the bridge's calls out. An inert level (a client) takes
//! nothing.
use super::level::EcsLevel;
use horizon_ecs_proto::targeting::Who;

impl EcsLevel {
    /// The player `player` steers the vehicle `vehicle` with this input. False when `vehicle` is not in the level or already holds it.
    #[allow(clippy::too_many_arguments)]
    pub fn pilot_input(&mut self, vehicle: u32, player: u32, forward: f32, strafe: f32, yaw: f32, pitch: f32, jump: f32, sprint: bool, sneak: bool, steer: bool) -> bool {
        let Some(e) = self.entity_of(vehicle) else { return false };
        self.ecs_mut().is_some_and(|w| w.pilot_input(e, Who::Player(player), forward, strafe, yaw, pitch, jump, sprint, sneak, steer))
    }

    /// The pilot left the vehicle `vehicle`.
    pub fn pilot_clear(&mut self, vehicle: u32) {
        let Some(e) = self.entity_of(vehicle) else { return };
        if let Some(w) = self.ecs_mut() {
            w.pilot_clear(e);
        }
    }
}
