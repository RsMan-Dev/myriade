//! The fishing hook's outputs for the host: a bite (its splash and bubbles) and a hook that takes hold of a target. They travel as
//! `HostEvent::Hook(HookEvent)`, the variant the session adds to `host_out.rs`.
use crate::fishing_hook::Target;
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// One output of a hook, named as the `HostEvent` it becomes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HookEvent {
    /// The hook got a bite: the host plays its splash (at `pitch`) and particles.
    Bite { hook: Entity, pitch: f32 },
    /// The hook holds `target`: a creature, an item, or a player by id.
    /// `by` is the owner's player.
    Attached { hook: Entity, target: Target, by: Who },
}

/// The hooks' outputs of the tick, read by `hook_events` (the host reads them apart from `HostEvent`).
#[derive(Resource, Default)]
pub struct HookOut(pub Vec<HookEvent>);
