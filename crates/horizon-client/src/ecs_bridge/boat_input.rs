//! The rider's keys and the boat's status on the client's side (`P bridge/boat_input.rs`): ids in, the bridge's calls out. An inert level
//! (a client) answers as an empty world: no key is taken, no boat is under water.
use super::level::EcsLevel;
use horizon_ecs_proto::bridge::boat_input::BoatStatus;

impl EcsLevel {
    /// The creature in `id`'s head seat holds `keys` (left, right, up, down); false when `id` is not a boat in the level.
    pub fn boat_input(&mut self, id: u32, keys: [bool; 4]) -> bool {
        let [left, right, up, down] = keys;
        self.ecs_mut().is_some_and(|w| w.boat_input(id, left, right, up, down))
    }

    /// The rider of the boat `id` left.
    pub fn boat_release(&mut self, id: u32) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.boat_release(e);
        }
    }

    /// The status of the boat `id`, None when it is not a boat in the level.
    pub fn boat_status(&self, id: u32) -> Option<BoatStatus> {
        let e = self.entity_of(id)?;
        self.ecs()?.boat_status(e)
    }
}
