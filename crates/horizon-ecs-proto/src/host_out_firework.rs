//! The fireworks' outputs toward the host: a burst to show, the damage to a creature or a player it reaches, and a rocket that is gone.
//! The bridge drains them with `take_firework_events`; the session puts them in `HostEvent::Firework`.
use crate::firework_burst::FireworkStar;
use crate::targeting::Who;
use bevy_ecs::entity::Entity;

/// One output of the fireworks, in the order they happened.
#[derive(Clone, Debug, PartialEq)]
pub enum FireworkEvent {
    /// The rocket's launch sound at `at` (not silent). The rocket's `GameEvent.EXPLODE` is the `Burst` the host shows: there is none apart.
    Launched { at: [f64; 3] },
    /// The stars shown at `at`, moving at `velocity` (the status 17 goes apart in `HostOut`); `sound` when the rocket is not silent.
    Burst { at: [f64; 3], velocity: [f64; 3], sound: bool, stars: Vec<FireworkStar> },
    /// `damage` of damage type `kind` to a creature, from the explosion at `at`, caused by the rocket's owner (`cause`: a creature or a player).
    Hurt { victim: Entity, damage: f32, at: [f64; 3], kind: &'static str, cause: Option<Who> },
    /// `damage` to a player, from the explosion at `at`, of damage type `kind` and caused by `cause`: the holder takes the full blast, the others the share of their distance.
    PlayerBlast { player: u32, damage: f32, at: [f64; 3], kind: &'static str, cause: Option<Who> },
    /// The rocket is gone.
    Expired { entity: Entity },
}
