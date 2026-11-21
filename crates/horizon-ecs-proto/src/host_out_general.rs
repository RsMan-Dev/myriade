//! The general entities' outputs for the host: a mod's tick that was asked for, a click on an interaction, and a change of a
//! general entity's payload. They travel as `HostEvent::General(GeneralEvent)`, the variant the session adds to `host_out.rs`.
use bevy_ecs::entity::Entity;

/// One output of a general entity, named as the `HostEvent` it becomes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeneralEvent {
    /// The mod asked for a tick of `entity` (its type is `kind`): the host calls the mod's tick hook.
    ModTick { entity: Entity, kind: u16 },
    /// `player` clicked the interaction `entity`: an attack when `attack`, else a use.
    Clicked { entity: Entity, player: u32, attack: bool },
    /// The payload of `entity` changed: the host writes it to the tracked entity.
    Changed { entity: Entity },
}
