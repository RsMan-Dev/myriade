//! The primed TNT's outputs toward the host, read with `EcsWorld::drain_tnt`. Merged into `HostEvent::Tnt` by the session (host_out.rs is not edited here).
use crate::targeting::Who;
use bevy_ecs::entity::Entity;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TntEvent {
    /// The fuse is lit (the hiss), when a player or a mob lights it.
    Fuse { entity: Entity },
    /// The fuse ended: the host applies the blast at `at` (blocks, damage) if `tnt_explodes` is on, credited to `owner`.
    Explode { at: [f64; 3], power: f32, owner: Option<Who>, used_portal: bool },
    /// The TNT landed after `fall` blocks in the air (`Entity.checkFallDamage`: the host's fall on the block).
    Landed { entity: Entity, at: [f64; 3], fall: f64 },
}
