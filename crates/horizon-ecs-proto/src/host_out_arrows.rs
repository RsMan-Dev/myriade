//! The arrows' outputs toward the host: the hit (on a creature, a player or a block), the pickup asked by a player (the host answers
//! whether it took the stack), the mod impact of its kind and the potion effects its hit gives. They queue in `ArrowOut`; the host
//! drains them with `arrow_more::drain`.
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;

#[derive(Clone, Debug, PartialEq)]
pub enum ArrowEvent {
    /// The arrow hit `victim` (None: a block) for `damage` of `damage_type`, at `at`, shot by `owner`. `knockback` is the push of the
    /// hit: the final push, already weighted in P (Punch x 0.6 x (1 - resistance)), in blocks per tick. `dx`, `dz` is the unit flat
    /// direction of the arrow: the host pushes a player by `knockback` along it. `fire` the ticks the victim burns (5 s when the arrow is
    /// on fire), `sound` the hit's sound (`id`, volume, pitch: `1.2 / (0.9 + 0.2 x random)`). The host applies a player's blow.
    Hit { entity: Entity, victim: Option<Who>, owner: Option<Who>, at: [f64; 3], damage: f32, damage_type: &'static str, knockback: f32, dx: f64, dz: f64, fire: u64, sound: &'static str, pitch: f32 },
    /// A player `by` asks for the stuck arrow: `stack` is what it gives. The host answers with `EcsWorld::arrow_taken`.
    PickedUp { entity: Entity, by: Who, stack: ItemStack },
    /// A mod impact of the projectile kind `kind` (its identifier), at `at`, on `target` (None: a block, its cell `block` and the normal
    /// `face` of the side hit), shot by `owner`.
    ModImpact { entity: Entity, kind: &'static str, at: [f64; 3], target: Option<Who>, owner: Option<Who>, block: Option<[i32; 3]>, face: Option<[i32; 3]> },
    /// A player's arrow `owner` ended after killing creatures of the kinds `kinds`, fired from `weapon` when known (`KILLED_BY_ARROW`).
    Killed { owner: Who, kinds: Vec<&'static str>, weapon: Option<ItemStack> },
    /// A potion effect the hit gives `target`: its registry name (`minecraft:poison`), its amplifier and its ticks.
    PotionEffect { target: Who, effect: &'static str, amplifier: i32, ticks: i32 },
}

/// The queue of the arrows' events, drained by the host.
#[derive(Resource, Default)]
pub struct ArrowOut(pub Vec<ArrowEvent>);
