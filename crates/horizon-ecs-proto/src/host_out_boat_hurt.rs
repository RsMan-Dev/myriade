//! What a boat's hurt, break and use tell the host (`boat_hurt.rs`): a hit to play its sound and count it, a break to place the boat's item,
//! its contents and the lead it held.
use crate::targeting::Who;
use bevy_ecs::prelude::Entity;

#[derive(Clone, Debug, PartialEq)]
pub enum BoatHurtEvent {
    Hit { entity: Entity, damage: f32, by: Option<Who> },
    /// The boat is gone and its drops are already on the ground (placed by the ECS); `at` is where it stood, `by` the attacker.
    Broke { at: [f64; 3], by: Option<Who> },
}
