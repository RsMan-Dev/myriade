//! What the cart variants ask of the host (S58): read by `EcsWorld::drain_cart` from `CartOut`, each entry in the order it happened.
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// What the carts ask of the host, in order. `Broke` comes with the drop and the attacker; `Explode` is the blast the host applies at `at`.
#[derive(Clone, Debug, PartialEq)]
pub enum CartEvent {
    Explode { at: [f64; 3], power: f32, entity: Entity },
    Broke { entity: Entity, at: [f64; 3], kind: u8, drop: bool, attacker: Option<Who> },
    HopperPulled { entity: Entity, item: Entity },
    SpawnerTick { entity: Entity },
    CommandRun { entity: Entity, command: String },
    OpenContents { entity: Entity, player: u32 },
    Fuse { entity: Entity },
    Signal { entity: Entity, level: i32 },
}

/// The events of the carts since the host last read them: one queue, empty at rest.
#[derive(Resource, Default)]
pub struct CartOut(pub Vec<CartEvent>);
