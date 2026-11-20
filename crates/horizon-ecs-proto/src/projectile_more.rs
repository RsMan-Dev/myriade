//! The projectiles of `projectile.rs` that differ only by their data: an egg and an ender pearl fly as a snowball, an experience
//! bottle falls at 0.07, a dragon's fireball and a player's wind charge fly as the hurting kinds. Not done, for lack of an
//! `OnHit` variant: the chicken of an egg, the teleport of a pearl, the orbs of a bottle.
use crate::projectile::{Damage, Flight, OnHit, Order};

/// `ThrownEgg`: a thrown item, gravity 0.03, no damage.
pub const EGG: Flight = Flight::SNOWBALL;
/// `ThrownEnderpearl`: a thrown item, gravity 0.03, no damage.
pub const ENDER_PEARL: Flight = Flight::SNOWBALL;
/// `ThrownExperienceBottle`: gravity 0.07, no damage.
pub const EXPERIENCE_BOTTLE: Flight = Flight { gravity: 0.07, order: Order::DragThenMove, ..Flight::SNOWBALL };
/// `DragonFireball`: `AbstractHurtingProjectile` with the default inertia and acceleration; no direct damage (the doubt is in the notes).
pub const DRAGON_FIREBALL: Flight = Flight { inertia: 0.95_f32 as f64, damage: Damage::Fixed(0.0), size: 1.0, order: Order::Hurting, accel: 0.1, max_age: 1200, hit: OnHit::DragonBurst, ..Flight::SNOWBALL };
/// `WindCharge` of a player: the breeze's straight flight, no damage, a burst of radius 1.2 (`WindCharge.RADIUS`).
pub const WIND_CHARGE: Flight = Flight { damage: Damage::Fixed(0.0), hit: OnHit::WindBurst { radius: 1.2 }, ..Flight::BREEZE_WIND_CHARGE };
