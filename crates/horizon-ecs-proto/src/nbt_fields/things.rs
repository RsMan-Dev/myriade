//! The NBT keys of the objects and projectiles the origin saves with their chunk (`persist.rs` `save_item`, `save_orb`, `save_tnt`,
//! `save_trident`, `save_projectile`, and `entity_chunks` for the end crystal). Each key is declared by the component that holds its
//! state (`FIELDS`); a key with no component to hold it (`Health`, `Fire`) stays a free key. The owners by UUID (`Owner`, `Thrower`) are in `owners.rs`.
//! Ticks become durations here: `Age` and `PickupDelay` are read against the entity's own clock (`now_of`), not stored.
use crate::behaviours::charged_shot::ExplosionPower;
use crate::behaviours::homing_projectile::Homing;
use crate::blocks::world_of;
use crate::cow::now_of;
use crate::end_crystal::{self, EndCrystal};
use crate::falling_block::{self, FallingBlock};
use crate::firework::Rocket;
use crate::item::{drop_item, rearm_expiry, Item};
use crate::nbt::{accept, owner, Field};
use crate::nbt_fields::owners;
use crate::ominous_spawner::{self, OminousSpawner};
use crate::orb::{spawn_orb, Orb};
use crate::projectile::{Flight, OnHit, Order, Projectile};
use crate::tnt::{spawn_primed, Primed};
use crate::trident::{self, Pickup, Trident};
use crate::arrow_more::{FiredFrom, PierceLevel, ThrownItem};
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use std::borrow::Cow;

/// The keys of the things, in the origin's order of writing.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Item"), write: write_item, read: read_item, accepts: compound, owner: Some(owner::<Item>) },
    Field { key: Cow::Borrowed("Age"), write: write_item_age, read: read_item_age, accepts: accept::numeric, owner: Some(owner::<Item>) },
    Field { key: Cow::Borrowed("PickupDelay"), write: write_pickup_delay, read: read_pickup_delay, accepts: accept::numeric, owner: Some(owner::<Item>) },
    Field { key: Cow::Borrowed("Owner"), write: owners::write_owner, read: owners::read_owner, accepts: owners::uuid_tag, owner: None },
    Field { key: Cow::Borrowed("Value"), write: write_value, read: read_value, accepts: accept::numeric, owner: Some(owner::<Orb>) },
    Field { key: Cow::Borrowed("Count"), write: write_count, read: read_count, accepts: accept::int, owner: Some(owner::<Orb>) },
    Field { key: Cow::Borrowed("Age"), write: write_orb_age, read: read_orb_age, accepts: accept::numeric, owner: Some(owner::<Orb>) },
    Field { key: Cow::Borrowed("fuse"), write: write_fuse, read: read_fuse, accepts: accept::numeric, owner: Some(owner::<Primed>) },
    Field { key: Cow::Borrowed("explosion_power"), write: write_power, read: read_power, accepts: accept::float, owner: Some(owner::<Primed>) },
    Field { key: Cow::Borrowed("ShowBottom"), write: write_show_bottom, read: read_show_bottom, accepts: byte, owner: Some(owner::<EndCrystal>) },
    Field { key: Cow::Borrowed("DealtDamage"), write: write_dealt, read: read_dealt, accepts: byte, owner: Some(owner::<Trident>) },
    Field { key: Cow::Borrowed("pickup"), write: write_pickup, read: read_pickup, accepts: byte, owner: Some(owner::<Trident>) },
    Field { key: Cow::Borrowed("item"), write: write_trident_item, read: read_trident_item, accepts: compound, owner: Some(owner::<Trident>) },
    Field { key: Cow::Borrowed("PierceLevel"), write: write_pierce_level, read: read_pierce_level, accepts: byte, owner: Some(owner::<PierceLevel>) },
    Field { key: Cow::Borrowed("Item"), write: write_thrown_item, read: read_thrown_item, accepts: compound, owner: Some(owner::<ThrownItem>) },
    Field { key: Cow::Borrowed("weapon"), write: write_weapon, read: read_weapon, accepts: compound, owner: Some(owner::<FiredFrom>) },
    Field { key: Cow::Borrowed("ExplosionPower"), write: write_explosion_power, read: read_explosion_power, accepts: byte, owner: Some(owner::<Flight>) },
    Field { key: Cow::Borrowed("acceleration_power"), write: write_acceleration, read: read_acceleration, accepts: double, owner: Some(owner::<Flight>) },
    Field { key: Cow::Borrowed("ExplosionPower"), write: write_ghast_power, read: read_ghast_power, accepts: integer, owner: Some(owner::<ExplosionPower>) },
    Field { key: Cow::Borrowed("dangerous"), write: write_dangerous, read: read_dangerous, accepts: byte, owner: Some(owner::<Flight>) },
    Field { key: Cow::Borrowed("LeftOwner"), write: write_left_owner, read: read_left_owner, accepts: byte, owner: Some(owner::<Projectile>) },
    Field { key: Cow::Borrowed("Life"), write: write_life, read: read_life, accepts: accept::int, owner: Some(owner::<Projectile>) },
    Field { key: Cow::Borrowed("ShotAtAngle"), write: write_angled, read: read_angled, accepts: byte, owner: Some(owner::<Rocket>) },
    Field { key: Cow::Borrowed("Silent"), write: write_silent, read: read_silent, accepts: byte, owner: Some(owner::<Rocket>) },
    Field { key: Cow::Borrowed("Steps"), write: write_steps, read: read_steps, accepts: accept::int, owner: Some(owner::<Homing>) },
    Field { key: Cow::Borrowed("TXD"), write: write_delta::<0>, read: read_delta::<0>, accepts: double, owner: Some(owner::<Homing>) },
    Field { key: Cow::Borrowed("TYD"), write: write_delta::<1>, read: read_delta::<1>, accepts: double, owner: Some(owner::<Homing>) },
    Field { key: Cow::Borrowed("TZD"), write: write_delta::<2>, read: read_delta::<2>, accepts: double, owner: Some(owner::<Homing>) },
    Field { key: Cow::Borrowed("Dir"), write: write_dir, read: read_dir, accepts: byte, owner: Some(owner::<Homing>) },
    Field { key: Cow::Borrowed("item"), write: write_ominous_item, read: read_ominous_item, accepts: compound, owner: Some(owner::<OminousSpawner>) },
    Field { key: Cow::Borrowed("spawn_item_after_ticks"), write: write_after, read: read_after, accepts: accept::long, owner: Some(owner::<OminousSpawner>) },
    Field { key: Cow::Borrowed("BlockState"), write: write_block_state, read: read_block_state, accepts: accept::any, owner: Some(owner::<FallingBlock>) },
    Field { key: Cow::Borrowed("DropItem"), write: write_drop_item, read: read_drop_item, accepts: byte, owner: Some(owner::<FallingBlock>) },
];

/// The constructors of the sorts with a bespoke creation: `restore` calls the one of its id (no species spawn), then distributes the keys.
pub fn builders() -> &'static [(&'static str, crate::nbt::Builder)] {
    &[
        (crate::species::lists::ITEM, build_item),
        (crate::species::lists::EXPERIENCE_ORB, build_orb),
        (crate::species::lists::TNT, build_tnt),
        (crate::species::lists::END_CRYSTAL, build_end_crystal),
        (crate::species::lists::FALLING_BLOCK, build_falling_block),
        (crate::species::armor_stand::ITEM, build_stand),
        (crate::species::lists::TRIDENT, build_trident),
        (crate::species::lists::OMINOUS_ITEM_SPAWNER, build_ominous),
        (crate::species::lists::CUSHION, build_cushion),
        (crate::species::lists::ITEM_FRAME, build_frame),
        (crate::species::lists::GLOW_ITEM_FRAME, build_frame),
        (crate::species::lists::PAINTING, build_painting),
        (crate::species::lists::AREA_EFFECT_CLOUD, build_breath_cloud),
        (crate::species::lists::EVOKER_FANGS, build_fangs),
        (crate::species::lists::FIREWORK_ROCKET, build_rocket),
    ]
}

fn compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(_))
}

fn double(t: &Tag) -> bool {
    matches!(t, Tag::Double(_))
}

fn byte(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_))
}

/// The dropped stack, saved as `ItemStack.CODEC` writes it (its components without the prototype's).
fn write_item(w: &World, e: Entity, c: &mut Compound) {
    if let Some(it) = w.get::<Item>(e) {
        c.insert("Item".into(), item_to_nbt(&it.stack));
    }
}

fn read_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let (Some(stack), Some(mut it)) = (item_from_nbt(t), w.get_mut::<Item>(e)) else { return false };
    it.stack = stack;
    true
}

/// `ItemEntity.age`: a signed int saved narrowed to a short (`-32768` is the frozen sentinel).
fn write_item_age(w: &World, e: Entity, c: &mut Compound) {
    if let Some(it) = w.get::<Item>(e) {
        c.insert("Age".into(), Tag::Short(crate::item::age_of(it, now_of(w, e)) as i16));
    }
}

fn read_item_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(v) = crate::nbt::short_of(t) else { return false };
    let now = now_of(w, e);
    let Some(mut it) = w.get_mut::<Item>(e) else { return false };
    it.age_base = crate::item::base_for(now, i32::from(v));
    drop(it);
    rearm_expiry(w, e);
    true
}

/// The ticks left before the pickup (`ItemEntity.pickupDelay`): 32767 when it is never picked up (no `ready_at`), a negative one as read.
fn write_pickup_delay(w: &World, e: Entity, c: &mut Compound) {
    if let Some(it) = w.get::<Item>(e) {
        c.insert("PickupDelay".into(), Tag::Short(crate::item::delay_of(it, now_of(w, e)) as i16));
    }
}

fn read_pickup_delay(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(v) = crate::nbt::short_of(t) else { return false };
    let now = now_of(w, e);
    let Some(mut it) = w.get_mut::<Item>(e) else { return false };
    it.ready_at = (v != i16::MAX).then(|| now + i64::from(v).max(0) as u64);
    it.neg_delay = if v < 0 { v } else { 0 };
    true
}

fn write_value(w: &World, e: Entity, c: &mut Compound) {
    if let Some(o) = w.get::<Orb>(e) {
        c.insert("Value".into(), Tag::Short(o.value as i16));
    }
}

fn read_value(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(v) = crate::nbt::short_of(t) else { return false };
    let Some(mut o) = w.get_mut::<Orb>(e) else { return false };
    o.value = i32::from(v);
    true
}

fn write_count(w: &World, e: Entity, c: &mut Compound) {
    if let Some(o) = w.get::<Orb>(e) {
        c.insert("Count".into(), Tag::Int(o.count.min(i32::MAX as u32) as i32));
    }
}

fn read_count(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Ok(count) = u32::try_from(*v) else { return false };
    let Some(mut o) = w.get_mut::<Orb>(e) else { return false };
    o.count = count;
    true
}

fn write_orb_age(w: &World, e: Entity, c: &mut Compound) {
    if let Some(o) = w.get::<Orb>(e) {
        let age = now_of(w, e).saturating_sub(o.age_base);
        c.insert("Age".into(), Tag::Short(age.min(i16::MAX as u64) as i16));
    }
}

fn read_orb_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let now = now_of(w, e);
    let Some(mut o) = w.get_mut::<Orb>(e) else { return false };
    o.age_base = now.saturating_sub(i64::from(*v).max(0) as u64);
    true
}

/// `PrimedTnt.fuse`: the ticks left, from the tick the fuse ends at.
fn write_fuse(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Primed>(e) {
        let left = p.fuse_at.saturating_sub(now_of(w, e));
        c.insert("fuse".into(), Tag::Short(left.min(i16::MAX as u64) as i16));
    }
}

fn read_fuse(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(v) = crate::nbt::short_of(t) else { return false };
    let now = now_of(w, e);
    let Some(mut p) = w.get_mut::<Primed>(e) else { return false };
    p.fuse_at = now + i64::from(v).max(0) as u64;
    true
}

fn write_power(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Primed>(e) {
        c.insert("explosion_power".into(), Tag::Float(p.power.to_bits()));
    }
}

fn read_power(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(bits) = t else { return false };
    let Some(mut p) = w.get_mut::<Primed>(e) else { return false };
    p.power = f32::from_bits(*bits);
    true
}

fn write_show_bottom(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<EndCrystal>(e) {
        c.insert("ShowBottom".into(), Tag::Byte(i8::from(s.show_bottom)));
    }
}

fn read_show_bottom(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut s) = w.get_mut::<EndCrystal>(e) else { return false };
    s.show_bottom = *v != 0;
    true
}

fn write_dealt(w: &World, e: Entity, c: &mut Compound) {
    if let Some(t) = w.get::<Trident>(e) {
        c.insert("DealtDamage".into(), Tag::Byte(i8::from(t.dealt)));
    }
}

fn read_dealt(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut tr) = w.get_mut::<Trident>(e) else { return false };
    tr.dealt = *v != 0;
    true
}

/// `AbstractArrow.Pickup`: 0 disallowed, 1 allowed, 2 creative only.
fn write_pickup(w: &World, e: Entity, c: &mut Compound) {
    if let Some(t) = w.get::<Trident>(e) {
        c.insert("pickup".into(), Tag::Byte(t.pickup as u8 as i8));
    }
}

fn read_pickup(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let pickup = match *v {
        0 => Pickup::Disallowed,
        1 => Pickup::Allowed,
        2 => Pickup::CreativeOnly,
        _ => return false,
    };
    let Some(mut tr) = w.get_mut::<Trident>(e) else { return false };
    tr.pickup = pickup;
    true
}

/// `AbstractArrow.PierceLevel`: the level only; the creatures met are not saved.
fn write_pierce_level(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<PierceLevel>(e) {
        c.insert("PierceLevel".into(), Tag::Byte(p.level as i8));
    }
}

fn read_pierce_level(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Ok(mut m) = w.get_entity_mut(e) else { return false };
    m.insert(PierceLevel { level: *v as u8, hit: Vec::new() });
    true
}

/// `ThrowableItemProjectile.Item`: the stack the projectile carries.
fn write_thrown_item(w: &World, e: Entity, c: &mut Compound) {
    if let Some(t) = w.get::<ThrownItem>(e) {
        c.insert("Item".into(), item_to_nbt(&t.0));
    }
}

fn read_thrown_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(stack): Option<ItemStack> = item_from_nbt(t) else { return false };
    let Ok(mut m) = w.get_entity_mut(e) else { return false };
    m.insert(ThrownItem(stack));
    true
}

/// `AbstractArrow.firedFromWeapon`: the stack of the weapon the arrow was shot from.
fn write_weapon(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<FiredFrom>(e) {
        c.insert("weapon".into(), item_to_nbt(&f.0));
    }
}

fn read_weapon(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(stack): Option<ItemStack> = item_from_nbt(t) else { return false };
    let Ok(mut m) = w.get_entity_mut(e) else { return false };
    m.insert(FiredFrom(stack));
    true
}

fn write_trident_item(w: &World, e: Entity, c: &mut Compound) {
    if let Some(t) = w.get::<Trident>(e) {
        c.insert("item".into(), item_to_nbt(&t.stack));
    }
}

fn read_trident_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(stack): Option<ItemStack> = item_from_nbt(t) else { return false };
    let Some(mut tr) = w.get_mut::<Trident>(e) else { return false };
    tr.stack = stack;
    true
}

/// `Entity.load` writes `LeftOwner` only when it is set.
fn write_left_owner(w: &World, e: Entity, c: &mut Compound) {
    if w.get::<Projectile>(e).is_some_and(|p| p.left_owner) {
        c.insert("LeftOwner".into(), Tag::Byte(1));
    }
}

fn read_left_owner(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut p) = w.get_mut::<Projectile>(e) else { return false };
    p.left_owner = *v != 0;
    true
}

/// `Entity.age` of a projectile: the ticks in the air (`Life`).
/// `LargeFireball`: the power of its blast is a byte (`putByte`), read back with `getByteOr(1)`. It lives in the `Flight` of the shot.
fn is_large_fireball(f: &Flight) -> bool {
    f.id == Flight::large_fireball(1).id
}

fn write_explosion_power(w: &World, e: Entity, c: &mut Compound) {
    if let Some(Flight { id, hit: OnHit::Explode { power, .. }, .. }) = w.get::<Flight>(e) {
        if *id == Flight::large_fireball(1).id {
            c.insert("ExplosionPower".into(), Tag::Byte(*power as i8));
        }
    }
}

fn read_explosion_power(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Flight>(e) else { return false };
    if !is_large_fireball(&f) {
        return true;
    }
    if let OnHit::Explode { power, .. } = &mut f.hit {
        *power = i32::from(*v);
    }
    true
}

/// `Ghast.addAdditionalSaveData`: the ghast's `ExplosionPower` as a byte; `readAdditionalSaveData` reads any number, narrowed to a byte
/// (`getByteOr`, 1 when absent: the component's creation value).
fn write_ghast_power(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<ExplosionPower>(e) {
        c.insert("ExplosionPower".into(), Tag::Byte(p.0 as i8));
    }
}
fn read_ghast_power(w: &mut World, e: Entity, t: &Tag) -> bool {
    let v = match t {
        Tag::Byte(v) => i64::from(*v),
        Tag::Short(v) => i64::from(*v),
        Tag::Int(v) => i64::from(*v),
        Tag::Long(v) => *v,
        _ => return false,
    };
    if let Some(mut p) = w.get_mut::<ExplosionPower>(e) {
        p.0 = i32::from(v as i8);
    }
    true
}
fn integer(t: &Tag) -> bool {
    matches!(t, Tag::Byte(_) | Tag::Short(_) | Tag::Int(_) | Tag::Long(_))
}

/// `AbstractHurtingProjectile.accelerationPower` (a double): the fireballs and the wind charges, whose flight is the hurting one.
fn write_acceleration(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Flight>(e).filter(|f| f.order == Order::Hurting) {
        c.insert("acceleration_power".into(), Tag::Double(f.accel.to_bits()));
    }
}

fn read_acceleration(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Double(bits) = t else { return false };
    let Some(mut f) = w.get_mut::<Flight>(e) else { return false };
    if f.order == Order::Hurting {
        f.accel = f64::from_bits(*bits);
    }
    true
}

/// `WitherSkull.dangerous` (a byte): the blue skull, which keeps 0.73 of its speed a tick instead of 0.95.
fn write_dangerous(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<Flight>(e).filter(|f| f.wither) {
        c.insert("dangerous".into(), Tag::Byte(i8::from(f.blue)));
    }
}

fn read_dangerous(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<Flight>(e) else { return false };
    if f.wither {
        let blue = Flight::wither_skull(*v != 0);
        f.blue = blue.blue;
        f.inertia = blue.inertia;
    }
    true
}

fn write_life(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<Projectile>(e).filter(|p| p.age > 0) {
        c.insert("Life".into(), Tag::Int(p.age.min(i32::MAX as u32) as i32));
    }
}

fn read_life(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Ok(age) = u32::try_from(*v) else { return false };
    let Some(mut p) = w.get_mut::<Projectile>(e) else { return false };
    p.age = age;
    true
}

fn write_angled(w: &World, e: Entity, c: &mut Compound) {
    if let Some(r) = w.get::<Rocket>(e) {
        c.insert("ShotAtAngle".into(), Tag::Byte(i8::from(r.angled)));
    }
}

fn read_angled(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut r) = w.get_mut::<Rocket>(e) else { return false };
    r.angled = *v != 0;
    true
}

fn write_silent(w: &World, e: Entity, c: &mut Compound) {
    if let Some(r) = w.get::<Rocket>(e) {
        c.insert("Silent".into(), Tag::Byte(i8::from(r.silent)));
    }
}

fn read_silent(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut r) = w.get_mut::<Rocket>(e) else { return false };
    r.silent = *v != 0;
    true
}

/// `Homing`: the steps taken on the current axis (`Steps`), the move's three deltas (`TXD`, `TYD`, `TZD`), and the direction (`Dir`).
fn write_steps(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Homing>(e) {
        c.insert("Steps".into(), Tag::Int(h.steps));
    }
}

fn read_steps(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut h) = w.get_mut::<Homing>(e) else { return false };
    h.steps = *v;
    true
}

const AXES: [&str; 3] = ["TXD", "TYD", "TZD"];

fn write_delta<const AXIS: usize>(w: &World, e: Entity, c: &mut Compound) {
    if let Some(h) = w.get::<Homing>(e) {
        c.insert(AXES[AXIS].into(), Tag::Double(h.delta[AXIS].to_bits()));
    }
}

fn read_delta<const AXIS: usize>(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Double(bits) = t else { return false };
    let Some(mut h) = w.get_mut::<Homing>(e) else { return false };
    h.delta[AXIS] = f64::from_bits(*bits);
    true
}

/// `ShulkerBullet.Dir`: read as the origin does, `% 6`.
fn write_dir(w: &World, e: Entity, c: &mut Compound) {
    if let Some(Some(d)) = w.get::<Homing>(e).map(|h| h.dir) {
        c.insert("Dir".into(), Tag::Byte(d as i8));
    }
}

fn read_dir(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut h) = w.get_mut::<Homing>(e) else { return false };
    h.dir = Some((*v % 6).unsigned_abs());
    true
}

fn write_ominous_item(w: &World, e: Entity, c: &mut Compound) {
    if let Some(o) = w.get::<OminousSpawner>(e) {
        c.insert("item".into(), item_to_nbt(&o.item));
    }
}

fn read_ominous_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(stack) = item_from_nbt(t) else { return false };
    let Some(mut o) = w.get_mut::<OminousSpawner>(e) else { return false };
    o.item = stack;
    true
}

fn write_after(w: &World, e: Entity, c: &mut Compound) {
    if let Some(o) = w.get::<OminousSpawner>(e) {
        c.insert("spawn_item_after_ticks".into(), Tag::Long(i64::from(o.after)));
    }
}

fn read_after(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Long(v) = t else { return false };
    let Ok(after) = u32::try_from(*v) else { return false };
    let Some(mut o) = w.get_mut::<OminousSpawner>(e) else { return false };
    o.after = after;
    true
}

/// `FallingBlock.BlockState`: the registry's state, as the block state codec writes it (a name or `{Name, Properties}`).
fn write_block_state(w: &World, e: Entity, c: &mut Compound) {
    if let Some(nbt) = w.get::<FallingBlock>(e).and_then(|f| f.state_nbt.clone()) {
        c.insert("BlockState".into(), nbt);
    }
}

/// `BlockState.CODEC` of 26.3: a block name, or a compound keyed by `id` (the legacy `Name` compound is refused).
fn modern_state(t: &Tag) -> Option<Tag> {
    match t {
        Tag::String(_) => Some(t.clone()),
        Tag::Compound(c) if c.get(&horizon_core::nbt::JavaString::from("id")).is_some() => Some(t.clone()),
        _ => None,
    }
}

fn read_block_state(w: &mut World, e: Entity, t: &Tag) -> bool {
    // A form the 26.3 codec refuses (the `Name` compound of earlier saves) leaves the default, sand (`Optional.orElse`).
    let modern = modern_state(t).unwrap_or_else(|| Tag::String("minecraft:sand".into()));
    let Some(state) = world_of(w).registry.saved_state_id(&modern) else { return false };
    let nbt = world_of(w).registry.state(state).map(|s| s.to_nbt());
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.state = state;
    f.state_nbt = nbt;
    true
}

/// `FallingBlockEntity.dropItem`: the item drops when the block cannot land, as `settings.drop_item` says.
fn write_drop_item(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<FallingBlock>(e) {
        c.insert("DropItem".into(), Tag::Byte(i8::from(f.settings.drop_item)));
    }
}

fn read_drop_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.settings.drop_item = *v != 0;
    true
}

fn tag<'a>(c: &'a Compound, key: &str) -> Option<&'a Tag> {
    c.get(&JavaString::from(key))
}

/// `CompoundTag.getShortOr`: any numeric tag reads as a short (a summon's `{fuse:40}` is an int); `Err` for a tag that is no number.
fn short_or(c: &Compound, key: &str, default: i32) -> Result<i32, ()> {
    match tag(c, key) {
        None => Ok(default),
        Some(t) => crate::nbt::short_of(t).map(i32::from).ok_or(()),
    }
}

/// The constructors: the creation function of each sort, given what the NBT holds. None when a datum it needs is missing.
fn build_item(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let stack = item_from_nbt(tag(c, "Item")?)?;
    Some(drop_item(w, stack, at, Vec3::ZERO, 0))
}

fn build_orb(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    // `ExperienceOrb.readAdditionalSaveData`: Value defaults to 0, Count to 1.
    let value = short_or(c, "Value", 0).ok()?;
    let count = match tag(c, "Count") { Some(Tag::Int(n)) => u32::try_from(*n).ok()?, None => 1, _ => return None };
    Some(spawn_orb(w, value, count, at, Vec3::ZERO))
}

/// `new EvokerFangs`: a summon without data has `warmup` 0, so the fangs bite and end on their own deadlines.
fn build_fangs(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let warmup = match tag(c, "Warmup") { Some(Tag::Int(v)) => *v, _ => 0 };
    Some(crate::fangs::spawn_fang(w, at, warmup, None))
}

/// A summoned rocket without data: flight 1, no explosion; its life is drawn and its end armed as a launched one's.
fn build_rocket(w: &mut World, at: Vec3, _: &Compound) -> Option<Entity> {
    let shot = crate::firework::Shot { at, owner: None, flight: 1, explosions: 0, silent: false, carrier: None, angled: None };
    Some(crate::firework::launch(w, shot))
}

fn build_tnt(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    // `PrimedTnt`: `DEFAULT_FUSE` 80 and `DEFAULT_EXPLOSION_POWER` 4.0 when the keys are absent (a summon without data).
    let fuse = short_or(c, "fuse", 80).ok()?;
    let power = match tag(c, "explosion_power") { Some(Tag::Float(p)) => f32::from_bits(*p), None => 4.0, _ => return None };
    Some(spawn_primed(w, at, Vec3::ZERO, fuse, power))
}

/// The compound with a `block_pos` (a summoned frame or painting has none: it hangs in the block `at` is in, as `HangingEntity` does).
fn with_block_pos(at: Vec3, c: &Compound) -> Compound {
    let mut c = c.clone();
    if !matches!(c.get(&JavaString::from("block_pos")), Some(Tag::IntArray(_))) {
        c.insert("block_pos".into(), Tag::IntArray(vec![at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32]));
    }
    c
}

/// An item frame or a glow frame hung on its saved face (`frame_item::load`).
fn build_frame(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    // `ItemFrame.readAdditionalSaveData`: an absent `Facing` is `Direction.DOWN` (data value 0), not the hanging default (south).
    let mut c = with_block_pos(at, c);
    if !c.contains_key(&JavaString::from("Facing")) {
        c.insert("Facing".into(), Tag::Byte(0));
    }
    crate::frame_item::load(w, &c)
}

/// A painting hung on its saved face with its saved variant (`painting::load`).
fn build_painting(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    // `Painting`'s constructor takes a variant of the registry and `readAdditionalSaveData` defaults the facing to south.
    let mut c = with_block_pos(at, c);
    if !c.contains_key(&JavaString::from("facing")) {
        c.insert("facing".into(), Tag::Byte(0));
    }
    if !c.contains_key(&JavaString::from("variant")) {
        let first = w.get_resource::<crate::painting::Motives>()?.0.first()?.0.to_string();
        c.insert("variant".into(), Tag::String(first.as_str().into()));
    }
    crate::painting::load(w, &c)
}

fn build_end_crystal(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let show = match tag(c, "ShowBottom") {
        Some(Tag::Byte(show)) => *show != 0,
        None => true,
        Some(_) => return None,
    };
    Some(end_crystal::spawn(w, at, show))
}

fn build_breath_cloud(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    crate::breath_cloud::build(w, at, c)
}

/// The block state is read against the registry; the cell is the block `at` is in.
fn build_falling_block(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    // `FallingBlockEntity.DEFAULT_BLOCK_STATE` is sand when `BlockState` is absent.
    let state = match tag(c, "BlockState") {
        Some(t) => world_of(w).registry.saved_state_id(&modern_state(t).unwrap_or_else(|| Tag::String("minecraft:sand".into())))?,
        None => world_of(w).registry.default_state("minecraft:sand")?,
    };
    let cell = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
    // `DropItem` defaults to true: the block's own item is what it drops when it cannot land (`read_drop_item` sets it from the key).
    let name = world_of(w).registry.state(state).map(|s| s.name.clone())?;
    Some(falling_block::fall(w, cell, state, Some(ItemStack::new(&name, 1))))
}

/// A stand made by its own creation (`stand::spawn`: the flags, the fire intake, the physics), the keys then set the flags and the pose.
fn build_stand(w: &mut World, at: Vec3, _: &Compound) -> Option<Entity> {
    let sp = crate::cow::species_table().0.into_iter().find(|sp| sp.name == crate::species::armor_stand::ITEM)?;
    Some(crate::stand::spawn(w, &sp, at, 0, 0))
}

fn build_trident(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    // AbstractArrow's pickupItemStack starts as `getDefaultPickupItem()`, one trident.
    let stack = match tag(c, "item") {
        Some(item) => item_from_nbt(item)?,
        None => ItemStack::new("minecraft:trident", 1),
    };
    Some(trident::throw(w, stack, at, Vec3::ZERO, None, 0, Pickup::Allowed))
}

/// `OminousItemSpawner.addAdditionalSaveData` writes `item` only when it is not empty, and `spawn_item_after_ticks` as a long; the
/// load reads an absent `item` as empty (air: no drop) and an absent delay as 0.
fn build_ominous(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let item = tag(c, "item").and_then(item_from_nbt).unwrap_or_else(|| ItemStack::new("minecraft:air", 1));
    let after = match tag(c, "spawn_item_after_ticks") {
        Some(Tag::Long(after)) => *after,
        _ => 0,
    };
    Some(ominous_spawner::spawn(w, item, at, u32::try_from(after).ok()?))
}

/// A cushion put back on its saved block (`block_pos`), with its `color`, its `Rotation` yaw and its `CustomName`.
fn build_cushion(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let block = match tag(c, "block_pos") {
        Some(Tag::IntArray(v)) if v.len() == 3 => [v[0], v[1], v[2]],
        _ => [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32],
    };
    let color = match tag(c, "color") {
        Some(Tag::String(s)) => crate::species::cushion::DYES.iter().position(|d| *d == s.to_string_lossy()).unwrap_or(0),
        _ => 0,
    };
    let yaw = match tag(c, "Rotation") {
        Some(Tag::List(l)) => match l.as_slice() {
            [Tag::Float(y), _] => f32::from_bits(*y),
            _ => 0.0,
        },
        _ => 0.0,
    };
    let name = tag(c, "CustomName").cloned().map(Box::new);
    crate::bridge::misc::make_cushion(w, block, at, color as u8, yaw, name)
}
