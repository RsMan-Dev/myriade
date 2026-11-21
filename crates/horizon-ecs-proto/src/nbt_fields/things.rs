//! The NBT keys of the objects and projectiles the origin saves with their chunk (`persist.rs` `save_item`, `save_orb`, `save_tnt`,
//! `save_trident`, `save_projectile`, and `entity_chunks` for the end crystal). Each key is declared by the component that holds its
//! state (`FIELDS`); a key with no component to hold it (the owners by UUID, `Health`, `Fire`, `Thrower`) stays a free key.
//! Ticks become durations here: `Age` and `PickupDelay` are read against the entity's own clock (`now_of`), not stored.
use crate::behaviours::homing_projectile::Homing;
use crate::blocks::world_of;
use crate::cow::now_of;
use crate::effect_cloud::EffectCloud;
use crate::end_crystal::{self, EndCrystal};
use crate::falling_block::{self, FallingBlock};
use crate::firework::Rocket;
use crate::item::{drop_item, rearm_expiry, Item};
use crate::nbt::{accept, owner, Field};
use crate::ominous_spawner::{self, OminousSpawner};
use crate::orb::{spawn_orb, Orb};
use crate::projectile::{Flight, OnHit, Order, Projectile};
use crate::tnt::{spawn_primed, Primed};
use crate::trident::{self, Pickup, Trident};
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use std::borrow::Cow;

/// The keys of the things, in the origin's order of writing.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("Item"), write: write_item, read: read_item, accepts: compound, owner: Some(owner::<Item>) },
    Field { key: Cow::Borrowed("Age"), write: write_item_age, read: read_item_age, accepts: accept::short, owner: Some(owner::<Item>) },
    Field { key: Cow::Borrowed("PickupDelay"), write: write_pickup_delay, read: read_pickup_delay, accepts: accept::short, owner: Some(owner::<Item>) },
    Field { key: Cow::Borrowed("Owner"), write: write_item_owner, read: read_item_owner, accepts: int_array, owner: Some(owner::<Item>) },
    Field { key: Cow::Borrowed("Value"), write: write_value, read: read_value, accepts: accept::short, owner: Some(owner::<Orb>) },
    Field { key: Cow::Borrowed("Count"), write: write_count, read: read_count, accepts: accept::int, owner: Some(owner::<Orb>) },
    Field { key: Cow::Borrowed("Age"), write: write_orb_age, read: read_orb_age, accepts: accept::short, owner: Some(owner::<Orb>) },
    Field { key: Cow::Borrowed("fuse"), write: write_fuse, read: read_fuse, accepts: accept::short, owner: Some(owner::<Primed>) },
    Field { key: Cow::Borrowed("explosion_power"), write: write_power, read: read_power, accepts: accept::float, owner: Some(owner::<Primed>) },
    Field { key: Cow::Borrowed("ShowBottom"), write: write_show_bottom, read: read_show_bottom, accepts: byte, owner: Some(owner::<EndCrystal>) },
    Field { key: Cow::Borrowed("DealtDamage"), write: write_dealt, read: read_dealt, accepts: byte, owner: Some(owner::<Trident>) },
    Field { key: Cow::Borrowed("pickup"), write: write_pickup, read: read_pickup, accepts: byte, owner: Some(owner::<Trident>) },
    Field { key: Cow::Borrowed("item"), write: write_trident_item, read: read_trident_item, accepts: compound, owner: Some(owner::<Trident>) },
    Field { key: Cow::Borrowed("ExplosionPower"), write: write_explosion_power, read: read_explosion_power, accepts: byte, owner: Some(owner::<Flight>) },
    Field { key: Cow::Borrowed("acceleration_power"), write: write_acceleration, read: read_acceleration, accepts: double, owner: Some(owner::<Flight>) },
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
    Field { key: Cow::Borrowed("Age"), write: write_cloud_age, read: read_cloud_age, accepts: accept::int, owner: Some(owner::<EffectCloud>) },
    Field { key: Cow::Borrowed("Duration"), write: write_cloud_duration, read: read_cloud_duration, accepts: accept::int, owner: Some(owner::<EffectCloud>) },
    Field { key: Cow::Borrowed("Radius"), write: write_radius, read: read_radius, accepts: accept::float, owner: Some(owner::<EffectCloud>) },
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

fn int_array(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(_))
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

/// `ItemEntity.age = now - age_base`, a short clamped to its range.
fn write_item_age(w: &World, e: Entity, c: &mut Compound) {
    if let Some(it) = w.get::<Item>(e) {
        let age = now_of(w, e).saturating_sub(it.age_base);
        c.insert("Age".into(), Tag::Short(age.min(i16::MAX as u64) as i16));
    }
}

fn read_item_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let now = now_of(w, e);
    let Some(mut it) = w.get_mut::<Item>(e) else { return false };
    it.age_base = now.saturating_sub(i64::from(*v).max(0) as u64);
    drop(it);
    rearm_expiry(w, e);
    true
}

/// The ticks left before the pickup (`ItemEntity.pickupDelay`): 32767 when it is never picked up (no `ready_at`).
fn write_pickup_delay(w: &World, e: Entity, c: &mut Compound) {
    if let Some(it) = w.get::<Item>(e) {
        let delay = it.ready_at.map_or(i16::MAX as u64, |r| r.saturating_sub(now_of(w, e)).min(i16::MAX as u64));
        c.insert("PickupDelay".into(), Tag::Short(delay as i16));
    }
}

fn read_pickup_delay(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let now = now_of(w, e);
    let Some(mut it) = w.get_mut::<Item>(e) else { return false };
    it.ready_at = (*v != i16::MAX).then(|| now + i64::from(*v).max(0) as u64);
    true
}

/// `ItemEntity.target`: the UUID of the only player who may pick the stack up.
fn write_item_owner(w: &World, e: Entity, c: &mut Compound) {
    if let Some(Some(target)) = w.get::<Item>(e).map(|it| it.target) {
        c.insert("Owner".into(), Tag::IntArray(target.to_vec()));
    }
}

fn read_item_owner(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let Ok(uuid) = <[i32; 4]>::try_from(v.as_slice()) else { return false };
    let Some(mut it) = w.get_mut::<Item>(e) else { return false };
    it.target = Some(uuid);
    true
}

fn write_value(w: &World, e: Entity, c: &mut Compound) {
    if let Some(o) = w.get::<Orb>(e) {
        c.insert("Value".into(), Tag::Short(o.value as i16));
    }
}

fn read_value(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Short(v) = t else { return false };
    let Some(mut o) = w.get_mut::<Orb>(e) else { return false };
    o.value = i32::from(*v);
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
    let Tag::Short(v) = t else { return false };
    let now = now_of(w, e);
    let Some(mut p) = w.get_mut::<Primed>(e) else { return false };
    p.fuse_at = now + i64::from(*v).max(0) as u64;
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

/// `AreaEffectCloud`: its age, its duration and its radius.
fn write_cloud_age(w: &World, e: Entity, c: &mut Compound) {
    if let Some(k) = w.get::<EffectCloud>(e) {
        c.insert("Age".into(), Tag::Int(k.age));
    }
}

fn read_cloud_age(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut k) = w.get_mut::<EffectCloud>(e) else { return false };
    k.age = *v;
    true
}

fn write_cloud_duration(w: &World, e: Entity, c: &mut Compound) {
    if let Some(k) = w.get::<EffectCloud>(e) {
        c.insert("Duration".into(), Tag::Int(k.duration));
    }
}

fn read_cloud_duration(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut k) = w.get_mut::<EffectCloud>(e) else { return false };
    k.duration = *v;
    true
}

fn write_radius(w: &World, e: Entity, c: &mut Compound) {
    if let Some(k) = w.get::<EffectCloud>(e) {
        c.insert("Radius".into(), Tag::Float(k.radius.to_bits()));
    }
}

fn read_radius(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(bits) = t else { return false };
    let Some(mut k) = w.get_mut::<EffectCloud>(e) else { return false };
    k.radius = f32::from_bits(*bits);
    true
}

/// `ominous_item_spawner`: the item it drops (`item`) and the ticks before the drop (`spawn_item_after_ticks`).
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
    let Some(f) = w.get::<FallingBlock>(e) else { return };
    if let Some(s) = world_of(w).registry.state(f.state) {
        c.insert("BlockState".into(), s.to_nbt());
    }
}

fn read_block_state(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(state) = world_of(w).registry.saved_state_id(t) else { return false };
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    f.state = state;
    true
}

/// `FallingBlockEntity.dropItem`: a drop is made when the item is set; `DropItem` 0 removes it.
fn write_drop_item(w: &World, e: Entity, c: &mut Compound) {
    if let Some(f) = w.get::<FallingBlock>(e) {
        c.insert("DropItem".into(), Tag::Byte(i8::from(f.drop.is_some())));
    }
}

fn read_drop_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Byte(v) = t else { return false };
    let Some(mut f) = w.get_mut::<FallingBlock>(e) else { return false };
    if *v == 0 {
        f.drop = None;
    }
    true
}

fn tag<'a>(c: &'a Compound, key: &str) -> Option<&'a Tag> {
    c.get(&JavaString::from(key))
}

/// The constructors: the creation function of each sort, given what the NBT holds. None when a datum it needs is missing.
fn build_item(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let stack = item_from_nbt(tag(c, "Item")?)?;
    Some(drop_item(w, stack, at, Vec3::ZERO, 0))
}

fn build_orb(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let (Some(Tag::Short(value)), Some(Tag::Int(count))) = (tag(c, "Value"), tag(c, "Count")) else { return None };
    Some(spawn_orb(w, i32::from(*value), u32::try_from(*count).ok()?, at, Vec3::ZERO))
}

fn build_tnt(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let (Some(Tag::Short(fuse)), Some(Tag::Float(power))) = (tag(c, "fuse"), tag(c, "explosion_power")) else { return None };
    Some(spawn_primed(w, at, Vec3::ZERO, i32::from(*fuse), f32::from_bits(*power)))
}

fn build_end_crystal(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let Some(Tag::Byte(show)) = tag(c, "ShowBottom") else { return None };
    Some(end_crystal::spawn(w, at, *show != 0))
}

/// The block state is read against the registry; the cell is the block `at` is in.
fn build_falling_block(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let state = world_of(w).registry.saved_state_id(tag(c, "BlockState")?)?;
    let cell = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
    // `DropItem` defaults to true: the block's own item is what it drops when it cannot land (`read_drop_item` clears it for 0).
    let name = world_of(w).registry.state(state).map(|s| s.name.clone())?;
    Some(falling_block::fall(w, cell, state, Some(ItemStack::new(&name, 1))))
}

/// A stand made by its own creation (`stand::spawn`: the flags, the fire intake, the physics), the keys then set the flags and the pose.
fn build_stand(w: &mut World, at: Vec3, _: &Compound) -> Option<Entity> {
    let sp = crate::cow::species_table().0.into_iter().find(|sp| sp.name == crate::species::armor_stand::ITEM)?;
    Some(crate::stand::spawn(w, &sp, at, 0, 0))
}

fn build_trident(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let stack = item_from_nbt(tag(c, "item")?)?;
    Some(trident::throw(w, stack, at, Vec3::ZERO, None, 0, Pickup::Allowed))
}

fn build_ominous(w: &mut World, at: Vec3, c: &Compound) -> Option<Entity> {
    let item = item_from_nbt(tag(c, "item")?)?;
    let Some(Tag::Long(after)) = tag(c, "spawn_item_after_ticks") else { return None };
    Some(ominous_spawner::spawn(w, item, at, u32::try_from(*after).ok()?))
}
