//! The boats' events for the host (`boat_tick.rs`): a stroke's sound. The bridge drains them with `take_boat_events`; the session puts them
//! in `HostEvent::Boat`. A boat's fall has no event: `AbstractBoat.checkFallDamage` hurts nothing.
use bevy_ecs::entity::Entity;

/// What a boat tells the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoatEvent {
    /// A paddle's stroke top (`boat_tick.rs` `step_one`): the row `side` (0 left, 1 right), the sound's `pitch` and whether the sound is the
    /// water's (`water`) or the land's. Not emitted in the air (`getPaddleSound` is null there).
    Paddle { entity: Entity, side: u8, pitch: f32, water: bool },
}
