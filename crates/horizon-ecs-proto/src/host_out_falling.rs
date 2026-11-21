//! Outputs of the falling blocks toward the host, as the `FallingEvent` variant `HostEvent::Falling` (merged into `host_out.rs` by the
//! session, with `HostOut::falling`). The placed block is a `HostEvent::SetBlock`, not one of these.
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use horizon_core::nbt::Compound;
use horizon_world::registry::BlockStateId;

#[derive(Clone, Debug, PartialEq)]
pub enum FallingEvent {
    /// A falling block touched the ground after `fall` blocks: the host runs `fallOn` of the block under it, then HIT_GROUND. `data` is the
    /// block entity data the block lands with (`TileEntityData`), none when it has none.
    Landed { entity: Entity, at: [f64; 3], fall: f64, data: Option<Compound> },
    /// A block that could not be placed (or an anvil destroyed by its fall) broke at `at`: its item drops when `drop`, its sound plays.
    Broke { at: [f64; 3], state: BlockStateId, drop: bool },
    /// A falling block hurt `who` by `amount` at `at`, the block being `state`; `kind` is the damage type's name.
    Hurt { who: Who, state: BlockStateId, at: [f64; 3], amount: f32, kind: &'static str },
}
