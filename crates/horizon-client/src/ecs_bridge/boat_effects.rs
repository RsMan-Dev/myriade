//! The boats' effects of a dimension's ECS level, by network id: the fire a boat is set on, the stuck multiplier.
//! Inert on a client, where each answers as an empty level does.
use super::level::EcsLevel;

impl EcsLevel {
    /// Sets the fire of boat `id` to `ticks`. Does nothing when inert or when `id` is unknown.
    pub fn boat_set_fire(&mut self, id: u32, ticks: i32) {
        if let Some(w) = self.ecs_mut() {
            if let Some(e) = w.entity_of(id) {
                w.boat_set_fire(e, ticks);
            }
        }
    }

    /// The stuck multiplier of boat `id`'s next move. Does nothing when inert or when `id` is unknown.
    pub fn boat_stuck(&mut self, id: u32, multiplier: [f64; 3]) {
        if let Some(w) = self.ecs_mut() {
            if let Some(e) = w.entity_of(id) {
                w.boat_stuck(e, multiplier);
            }
        }
    }
}
