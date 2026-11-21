//! What the vehicles' pushes write for the host: a push of a player (the host sends it to the client).
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// What a vehicle's push wrote for the host: `vehicle` pushed `player` by `impulse`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VehiclePushEvent {
    PushPlayer { vehicle: Entity, player: Who, impulse: [f64; 3] },
}

/// The events of the pushes since the last drain.
#[derive(Resource, Default)]
pub struct VehiclePushEvents(pub Vec<VehiclePushEvent>);
