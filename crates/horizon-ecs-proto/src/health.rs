//! The health side of a sonic boom: the target takes the damage, then is pushed along the boom's direction.
use crate::area::{hurt_creature, hurt_player, push_creature, push_player};
use crate::cow::Resist;
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// The damage of a boom, whatever the armour.
const BOOM_DAMAGE: f32 = 10.0;

/// The boom hurts `target` (by `e`), then pushes it along `direction`: 2.5 horizontally and 0.5 vertically, both scaled by
/// `1 - knockback resistance`. A player has no resistance here (its attribute is not kept by the bench).
pub fn sonic_boom(w: &mut World, e: Entity, target: Who, direction: [f64; 3]) {
    let resistance = match target {
        Who::Player(_) => 0.0,
        Who::Entity(c) => w.get::<Resist>(c).map_or(0.0, |r| r.0),
    };
    let (horizontal, vertical) = (2.5 * (1.0 - resistance), 0.5 * (1.0 - resistance));
    let velocity = [direction[0] * horizontal, direction[1] * vertical, direction[2] * horizontal];
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
