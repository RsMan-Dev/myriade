//! The health side of a sonic boom: the target takes the damage, then is pushed along the boom's direction.
use crate::area::{hurt_creature, hurt_player, push_creature, push_player};
use crate::cow::Resist;
use crate::rules::health::{boom_push, BOOM_DAMAGE};
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// The boom hurts `target` (by `e`), then pushes it along `direction`: 2.5 horizontally and 0.5 vertically, both scaled by
/// `1 - knockback resistance`. A player has no resistance here (its attribute is not kept by the bench).
pub fn sonic_boom(w: &mut World, e: Entity, target: Who, direction: [f64; 3]) {
    let resistance = match target {
        Who::Player(_) => 0.0,
        Who::Entity(c) => w.get::<Resist>(c).map_or(0.0, |r| r.0),
    };
    let velocity = boom_push(direction, resistance);
    match target {
        Who::Player(id) => {
            hurt_player(w, id, BOOM_DAMAGE, e);
            push_player(w, id, velocity);
        }
        Who::Entity(c) => {
            hurt_creature(w, c, e, BOOM_DAMAGE);
            push_creature(w, c, velocity);
        }
    }
}
