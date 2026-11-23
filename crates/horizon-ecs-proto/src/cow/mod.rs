//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.

//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use horizon_client_lib::mobs::{
    MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;

mod blocks_world;
mod fluids;
mod frame;
mod health;
mod herd;
mod index;
mod motion;
mod nav;
mod ordered;
mod skips;
mod spawn;
mod species;
mod travel;
mod wheel;
pub use blocks_world::*;
pub use fluids::*;
pub use frame::*;
pub use health::*;
pub use herd::*;
pub use index::*;
pub use motion::*;
pub use nav::*;
pub use ordered::*;
pub use skips::*;
pub use spawn::*;
pub use species::*;
pub use travel::*;
pub use wheel::*;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod hazards_tests;
#[cfg(test)]
mod index_tests;
pub mod helpers;
pub use helpers::*;
