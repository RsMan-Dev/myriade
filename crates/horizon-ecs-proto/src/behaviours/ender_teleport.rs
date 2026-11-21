//! The teleport service of the creatures that teleport (`LivingEntity.randomTeleport`, `Enderman.teleport`): `teleport_to` (a target spot: the
//! first block below it that `entities_can_teleport_to` is the ground, the box must be free, out of liquids and of the blocks the creature
//! does not teleport onto), `random` (a spot within 32 blocks, the draws of `Enderman.teleportRandom`), `towards` (16 blocks toward a spot),
//! `repeatedly` (up to `n` tries). The module only holds the parameters and the reaction to a hurt that is not a creature's: nine in ten
//! teleport (`Enderman.hurtServer`). A projectile that meets a `Dodges` creature does no harm and makes it try 64 times (`projectile.rs`).
//! The callers are the goals that ask for a teleport (`look_for_player`'s near and far counts) and the projectile flight.
use super::*;
use crate::cow::*;
use crate::rules::ender_teleport::{random_offset, spot_box, spread_offset, towards_spot};
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The tag of the blocks it does not teleport onto or into (`#enderman_does_not_teleport_to`).
    pub avoid: Option<&'static str>,
    /// Not onto a ground that holds water (a waterlogged slab).
    pub dry_ground: bool,
    /// A projectile is dodged: no harm and 64 tries.
    pub dodges: bool,
    /// The water hurts it (`isSensitiveToWater`).
    pub water: bool,
}
impl Params {
    pub const ENDERMAN: Self = Self { avoid: Some("minecraft:enderman_does_not_teleport_to"), dry_ground: true, dodges: true, water: true };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Teleporter(pub &'static Params);
/// Projectiles do it no harm (`Enderman.hurtServer`: a projectile is dodged unless it rides).
#[derive(Component, Clone, Copy, Debug)]
pub struct Dodges;
/// The water hurts it (`Enderman.isSensitiveToWater`); read by the breathing system of the creatures in a fluid.
#[derive(Component, Clone, Copy, Debug)]
pub struct WaterSensitive;

pub static MODULE: Module = Module { name: "ender_teleport", flags: 0, equip, can_use: never, damaged, requires: &[Cap::Body, Cap::Living], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Teleporter(p));
    if p.dodges {
        w.entity_mut(e).insert(Dodges);
    }
    if p.water {
        w.entity_mut(e).insert(WaterSensitive);
    }
}
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
/// `Enderman.hurtServer`: a hurt with no creature behind it teleports unless one chance in ten.
fn damaged(w: &mut World, e: Entity, _: Inst, d: &Damaged) {
    if !matches!(d.who, Some(crate::targeting::Who::Entity(_))) && crate::goals::with_rng(w, e, |r| r.next_int(10)) != 0 {
        random(w, e);
    }
}

/// `LivingEntity.randomTeleport(x, y, z, true, avoid)`.
pub fn teleport_to(w: &mut World, e: Entity, target: Vec3) -> bool {
    let Some(Teleporter(p)) = w.get::<Teleporter>(e).copied() else { return false };
    teleport_avoiding(w, e, target, p.avoid, p.dry_ground)
}

/// The same for any creature, the tag to avoid and the dry-ground rule given (a mod's `randomTeleport`).
pub fn teleport_avoiding(w: &mut World, e: Entity, target: Vec3, avoid: Option<&str>, dry_ground: bool) -> bool {
    let (Some(b), Some(_)) = (w.get::<Body>(e).copied(), w.get::<Pos>(e)) else { return false };
    let level = *w.resource::<Level>();
    let mw = level.mw.0;
    let (bx, bz) = (target.x.floor() as i32, target.z.floor() as i32);
    if !mw.world.loaded(bx, bz) {
        return false;
    }
    let refused = |name: &str| avoid.is_some_and(|tag| mw.has_tag(tag, name));
    let mut by = target.y.floor() as i32;
    let mut ny = target.y;
    let ground = loop {
        if by <= mw.world.bottom_y() {
            return false;
        }
        by -= 1;
        let name = mw.block_name(bx, by, bz);
        if mw.has_tag("minecraft:entities_can_teleport_to", name) {
            break name;
        }
        ny -= 1.0;
    };
    if refused(ground) {
        return false;
    }
    let bounds = spot_box(target.x, ny, target.z, f64::from(b.width), f64::from(b.height));
    if !mw.no_collision(bounds) {
        return false;
    }
    let (lo, hi) = (bounds.min_position(), bounds.max_position());
    for cx in lo.x.floor() as i32..=hi.x.floor() as i32 {
        for cy in lo.y.floor() as i32..=hi.y.floor() as i32 {
            for cz in lo.z.floor() as i32..=hi.z.floor() as i32 {
                if mw.fluid(cx, cy, cz) != horizon_client_lib::blocks::Fluid::None || refused(mw.block_name(cx, cy, cz)) {
                    return false;
                }
            }
        }
    }
    if dry_ground && mw.fluid(bx, by, bz) == horizon_client_lib::blocks::Fluid::Water {
        return false;
    }
    land(w, e, Vec3::new(target.x, ny, target.z));
    true
}
/// The move of every teleport (the enderman's, the pets'): the entity is put at the spot and its path dropped.
pub fn land(w: &mut World, e: Entity, at: Vec3) {
    teleport(w, e, at);
    crate::goals::stop_nav(w, e);
}
/// `Enderman.teleportRandom`: a spot within 32 blocks, 32 up or down.
pub fn random(w: &mut World, e: Entity) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let (dx, dy, dz) = crate::goals::with_rng(w, e, |r| random_offset(r.next_f64(), r.next_int(64), r.next_f64()));
    teleport_to(w, e, Vec3::new(pos.x + dx, pos.y + f64::from(dy), pos.z + dz))
}
/// `Enderman.teleportTowards`: 16 blocks along the line to `at`, with a spread.
pub fn towards(w: &mut World, e: Entity, at: Vec3, eye_y: f64) -> bool {
    let (Some(pos), Some(h)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Body>(e).map(|b| f64::from(b.height))) else { return false };
    let offset = crate::goals::with_rng(w, e, |r| spread_offset(r.next_f64(), r.next_int(16), r.next_f64()));
    teleport_to(w, e, towards_spot(pos, at, h, eye_y, offset))
}
/// `repeatedlyTryToTeleport`: up to `tries` random teleports, until one succeeds.
pub fn repeatedly(w: &mut World, e: Entity, tries: u32) -> bool {
    (0..tries).any(|_| random(w, e))
}
