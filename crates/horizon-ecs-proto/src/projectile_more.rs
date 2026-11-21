//! The projectiles of `projectile.rs` that differ only by their data: an egg and an ender pearl fly as a snowball, an experience
//! bottle falls at 0.07, a dragon's fireball and a player's wind charge fly as the hurting kinds. The effects at the end (`OnHit::Hatch`,
//! `Teleport`, `Orbs`) are applied by `ends`; the species and the numbers are the configuration of the flight.
use crate::cow::{box_at, Body, Bounds, Contact, Pos};
use crate::projectile::{Damage, Flight, OnHit, Order};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};

/// The random source of the thrown items' ends (the level's, in vanilla).
#[derive(Resource)]
pub struct ThrownRng(pub LegacyRandom);

/// `ThrownEgg.onHit`: with chance 1 in `one_in` a hatchling, and then 1 in `many_in` that `many` hatch; each is born a baby.
pub const EGG_HATCH: (i32, i32, u32) = (8, 32, 4);
/// `ThrownEnderpearl.onHit`: the chance of an endermite for a player, the damage the player takes (a fall).
pub const PEARL_ENDERMITE: f32 = 0.05;
pub const PEARL_DAMAGE: f32 = 5.0;
/// `ThrownExperienceBottle.onHit`: `3 + nextInt(5) + nextInt(5)` of experience.
pub const BOTTLE_XP: (i32, i32) = (3, 5);

/// `ThrownEgg`: a thrown item, gravity 0.03, no damage.
pub const EGG: Flight = Flight { id: crate::species::lists::EGG, ..Flight::SNOWBALL };
/// The egg's end, its hatchling being the species `into`.
pub const fn egg_hit(into: &'static str) -> OnHit {
    OnHit::Hatch { into, one_in: EGG_HATCH.0, many_in: EGG_HATCH.1, many: EGG_HATCH.2, baby: 24_000 }
}
/// `ThrownEnderpearl`: a thrown item, gravity 0.03, no damage.
pub const ENDER_PEARL: Flight = Flight { id: crate::species::lists::ENDER_PEARL, hit: OnHit::Teleport { endermite: PEARL_ENDERMITE, damage: PEARL_DAMAGE }, ..Flight::SNOWBALL };
/// `ThrownExperienceBottle`: gravity 0.07, no damage.
pub const EXPERIENCE_BOTTLE: Flight = Flight { id: crate::species::lists::EXPERIENCE_BOTTLE, gravity: 0.07, order: Order::DragThenMove, hit: OnHit::Orbs { base: BOTTLE_XP.0, bound: BOTTLE_XP.1 }, ..Flight::SNOWBALL };
/// `DragonFireball`: `AbstractHurtingProjectile` with the default inertia and acceleration; no direct damage (the doubt is in the notes).
pub const DRAGON_FIREBALL: Flight = Flight { id: crate::species::lists::DRAGON_FIREBALL, inertia: 0.95_f32 as f64, damage: Damage::Fixed(0.0), size: 1.0, order: Order::Hurting, accel: 0.1, max_age: 1200, hit: OnHit::DragonBurst, ..Flight::SNOWBALL };
/// `WindCharge` of a player: the breeze's straight flight, no damage, a burst of radius 1.2 (`WindCharge.RADIUS`).
pub const WIND_CHARGE: Flight = Flight { id: crate::species::lists::WIND_CHARGE, damage: Damage::Fixed(0.0), hit: OnHit::WindBurst { radius: 1.2 }, ..Flight::BREEZE_WIND_CHARGE };

/// The end of a thrown item at `at` (the hit point); `previous` is where it was at the start of its last tick (`oldPosition`).
pub fn ends(w: &mut World, hit: OnHit, at: Vec3, previous: Vec3, owner: Option<Who>) {
    ensure_rng(w);
    match hit {
        OnHit::Hatch { into, one_in, many_in, many, baby } => {
            let (n, ) = {
                let mut r = w.resource_mut::<ThrownRng>();
                if r.0.next_int_bound(one_in).unwrap_or(1) != 0 { return }
                (if r.0.next_int_bound(many_in).unwrap_or(1) == 0 { many } else { 1 },)
            };
            let Some(sp) = crate::cow::species_table().0.into_iter().find(|s| s.name == into) else { return };
            for _ in 0..n {
                let e = crate::cow::spawn_species(w, &sp, at.x, at.z, 1);
                if let Some(mut p) = w.get_mut::<Pos>(e) {
                    p.0 = at;
                }
                crate::spawn_finalize::set_baby(w, e, baby);
            }
        }
        OnHit::Teleport { endermite, damage } => {
            match owner {
                Some(Who::Entity(o)) => {
                    // Not a player: moved to the old position, the fall forgotten, no damage and no endermite.
                    if w.get::<crate::cow::Health>(o).is_none_or(|h| h.0 <= 0.0) {
                        return;
                    }
                    let size = w.get::<Body>(o).map(|b| (b.width, b.height));
                    if let Some(mut p) = w.get_mut::<Pos>(o) {
                        p.0 = previous;
                    }
                    if let Some((wd, ht)) = size
                        && let Some(mut b) = w.get_mut::<Bounds>(o)
                    {
                        b.0 = box_at(previous, wd, ht);
                    }
                    if let Some(mut c) = w.get_mut::<Contact>(o) {
                        c.fall_distance = 0.0;
                    }
                }
                Some(Who::Player(player)) => {
                    let roll = w.resource_mut::<ThrownRng>().0.next_float() < endermite;
                    w.resource_mut::<crate::goals::Events>().host.pearl(player, [previous.x, previous.y, previous.z], damage, roll);
                }
                None => {}
            }
        }
        OnHit::Orbs { base, bound } => {
            let amount = {
                let mut r = w.resource_mut::<ThrownRng>();
                base + r.0.next_int_bound(bound).unwrap_or(0) + r.0.next_int_bound(bound).unwrap_or(0)
            };
            w.resource_mut::<crate::goals::Events>().host.xp([at.x, at.y, at.z], amount as u32);
        }
        _ => {}
    }
}

/// The random source, made at the first use.
pub fn ensure_rng(w: &mut World) {
    if !w.contains_resource::<ThrownRng>() {
        w.insert_resource(ThrownRng(LegacyRandom::new(0x7468_726f)));
    }
}
