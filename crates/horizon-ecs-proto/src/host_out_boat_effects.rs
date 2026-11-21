//! The effects a boat's blocks send the host (`boat_effects`) for what touches a player or is seen: a column's throw, a lava's
//! announcement, a snowflake's particle. The damage of fire and lava is P's (`boat_hurt::hit`), not sent.
use bevy_ecs::entity::Entity;

#[derive(Clone, Debug, PartialEq)]
pub enum BoatEffectEvent {
    Bubble { entity: Entity, down: bool },
    LavaHurt { at: [f64; 3] },
    Snowflakes { at: [f64; 3], velocity: [f64; 3] },
}
