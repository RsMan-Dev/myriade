//! The boats' effects at the bridge: the fire a boat is set on, and the stuck multiplier a host gives. The burns are queued in
//! `boat_effects` and taken by the host, which applies them through `boat_hurt::hit`: the only path that inflicts them.
use crate::boat_effects;
use crate::boat_state::Hull;
use crate::bridge::host::EcsWorld;
use bevy_ecs::entity::Entity;

impl EcsWorld {
    /// Sets the fire of boat `e` to `ticks`.
    pub fn boat_set_fire(&mut self, e: Entity, ticks: i32) {
        boat_effects::set_fire(self.world_mut(), e, ticks);
    }

    /// The stuck multiplier the next move of boat `e` spends.
    pub fn boat_stuck(&mut self, e: Entity, multiplier: [f64; 3]) {
        if let Some(mut h) = self.world_mut().get_mut::<Hull>(e) {
            h.stuck = multiplier;
        }
    }
}
