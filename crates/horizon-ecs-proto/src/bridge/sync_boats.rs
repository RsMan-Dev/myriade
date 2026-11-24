//! The boats' entries at vanilla's indices: VehicleEntity 8..=10 (the hurt shake) and AbstractBoat 11..=13 (the paddles and the bubble
//! time). Defined in that order by the jar (`javap`: three `defineId` each). A boat has no other data: its wood is its type.
use super::sync::{SyncSlot, SyncValue};
use crate::boat::{Boat, Hurt, Wobble};
use bevy_ecs::prelude::*;

/// VehicleEntity.DATA_ID_HURT: the shake's time, sent while it runs.
fn hurt_time(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Hurt>(e).filter(|h| h.time != 0).map(|h| SyncValue::Int(h.time))
}

/// VehicleEntity.DATA_ID_HURTDIR: sent while it is not the default 1.
fn hurt_dir(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Hurt>(e).filter(|h| h.dir != 1).map(|h| SyncValue::Int(h.dir))
}

/// VehicleEntity.DATA_ID_DAMAGE: sent while it is not 0.
fn damage(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Hurt>(e).filter(|h| h.damage != 0.0).map(|h| SyncValue::Float(h.damage))
}

/// AbstractBoat.DATA_ID_PADDLE_LEFT: the left paddle's stroke, sent while it runs (false is the default: the tracker resets to it).
fn paddle_left(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Boat>(e).filter(|b| b.paddling[0]).map(|_| SyncValue::Bool(true))
}

/// AbstractBoat.DATA_ID_PADDLE_RIGHT: the right paddle's stroke, sent while it runs.
fn paddle_right(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Boat>(e).filter(|b| b.paddling[1]).map(|_| SyncValue::Bool(true))
}

/// AbstractBoat.DATA_ID_BUBBLE_TIME: the wobble over a bubble column, back to 0 once the boat is off the column.
fn bubble_time(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Wobble>(e).filter(|o| o.time != 0).map(|o| SyncValue::Int(o.time))
}

/// The boats' slots, in index order.
pub static SLOTS: &[SyncSlot] = &[
    SyncSlot { index: 8, read: hurt_time },
    SyncSlot { index: 9, read: hurt_dir },
    SyncSlot { index: 10, read: damage },
    SyncSlot { index: 11, read: paddle_left },
    SyncSlot { index: 12, read: paddle_right },
    SyncSlot { index: 13, read: bubble_time },
];
