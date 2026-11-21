//! The minecarts' outputs toward the host: a cart off its track, a powered rail's push. They queue in `CartMoveOut` as they happen (an event,
//! never a scan) and the host drains them with `bridge::cart_move::drain_cart_events`.
use bevy_ecs::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CartMoveEvent {
    /// The cart rolls off its track (no rail under it while it moves).
    Derail { entity: Entity },
    /// A powered rail pushed the cart this tick.
    PoweredBoost { entity: Entity },
}

/// The queue of the carts' events, drained by the host.
#[derive(Resource, Default)]
pub struct CartMoveOut(pub Vec<CartMoveEvent>);
