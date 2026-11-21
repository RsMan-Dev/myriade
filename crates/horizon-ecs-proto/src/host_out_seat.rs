//! The seat events the host hears (`Entity.startRiding` and `stopRiding`, the hooks `entity_start_riding` and `stop_riding`): a rider is an
//! entity or a player of the host. The bridge drains them (`EcsWorld::drain_seat_events`); they join `HostEvent` at the merge.
use crate::seat::Rider;
use bevy_ecs::entity::Entity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatEvent {
    Boarded { rider: Rider, vehicle: Entity },
    Left { rider: Rider, vehicle: Entity },
}
