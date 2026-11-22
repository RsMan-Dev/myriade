//! The entries every bridged creature shares, at vanilla's indices (Entity 0..=7, LivingEntity 8..=14, Mob 15), read from the
//! components. Indices with no slot have no generic source component in P: 6 (pose, the species' own), 8 (using an item), 10 (effect particles: the effects' colours are not in P), 12 and 13 (stuck
//! arrows and stingers), and Mob 15's aggressive and left-handed bits.
use super::sync::{SyncSlot, SyncValue};
use crate::behaviours::bed::Sleeping;
use crate::behaviours::elder_curse::Silent;
use crate::cow::{Air, AirLow, Burning, Frozen, GrowAt, Health, now_of};
use crate::dragon::flight::NoAi;
use crate::effects::{AMBIENT, Effects, Invisible};
use bevy_ecs::prelude::*;
use horizon_core::nbt::Tag;

/// Entity.DATA_SHARED_FLAGS_ID: on fire 0x01, invisible 0x20, glowing 0x40 (the saved `Glowing` key, `setGlowingTag`).
fn shared(w: &World, e: Entity) -> Option<SyncValue> {
    let fire = w.get::<Burning>(e).is_some_and(|b| b.until > now_of(w, e) as i64);
    let glow = w.get::<crate::nbt::NbtData>(e).is_some_and(|d| matches!(d.0.get(&horizon_core::nbt::JavaString::from("Glowing")), Some(horizon_core::nbt::Tag::Byte(v)) if *v != 0));
    let bits = i8::from(fire) | i8::from(w.entity(e).contains::<Invisible>()) << 5 | i8::from(glow) << 6;
    (bits != 0).then_some(SyncValue::Byte(bits))
}

/// Entity.DATA_AIR_SUPPLY: sent while the air is below its maximum (`AirLow`).
fn air(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<AirLow>(e)?;
    w.get::<Air>(e).map(|a| SyncValue::Int(a.0))
}

/// Entity.DATA_SILENT: the marker of a creature born silent.
fn silent(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Silent>(e).map(|_| SyncValue::Bool(true))
}

/// Entity.DATA_TICKS_FROZEN: the count, 0 is the default.
fn frozen(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Frozen>(e).filter(|f| f.0 != 0).map(|f| SyncValue::Int(f.0))
}

/// LivingEntity.DATA_HEALTH_ID: defined as 1, so 1 is not sent.
fn health(w: &World, e: Entity) -> Option<SyncValue> {
    // An item and an orb carry a `Health` too (`ItemEntity.health`), which is not synced: their index 9 does not exist.
    if w.get::<crate::item::Item>(e).is_some() || w.get::<crate::orb::Orb>(e).is_some() {
        return None;
    }
    w.get::<Health>(e).map(|h| h.0.max(0.0)).filter(|h| *h != 1.0).map(SyncValue::Float)
}

/// LivingEntity.DATA_EFFECT_AMBIENCE_ID: sent when there are effects and all of them are ambient.
fn ambient(w: &World, e: Entity) -> Option<SyncValue> {
    let list = &w.get::<Effects>(e)?.list;
    (!list.is_empty() && list.iter().all(|f| f.flags & AMBIENT != 0)).then_some(SyncValue::Bool(true))
}

/// LivingEntity.DATA_SLEEPING_POS_ID: the bed a sleeper lies in.
fn sleeping(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Sleeping>(e).map(|s| SyncValue::OptPos(Some(s.0)))
}

/// Mob.DATA_MOB_FLAGS_ID: no AI 1, left-handed 2.
fn mob_flags(w: &World, e: Entity) -> Option<SyncValue> {
    let bits = i8::from(w.get::<NoAi>(e).is_some()) | (i8::from(w.get::<crate::left_handed::LeftHanded>(e).is_some()) << 1);
    (bits != 0).then_some(SyncValue::Byte(bits))
}

/// AgeableMob.DATA_BABY_ID (index 16): the baby flag, while the entity has its growing timer (`GrowAt`).
fn baby(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<GrowAt>(e).map(|_| SyncValue::Bool(true))
}

/// Entity.DATA_CUSTOM_NAME (index 2, an optional component): the saved name, a plain string tag as the old mob sent it.
fn custom_name(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::custom_name::CustomName>(e).map(|n| SyncValue::OptText(Some(Tag::String(n.0.as_str().into()))))
}

/// Entity.DATA_CUSTOM_NAME_VISIBLE (index 3): the `NameVisible` marker.
fn name_visible(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::custom_name::NameVisible>(e).map(|_| SyncValue::Bool(true))
}

/// Entity.DATA_NO_GRAVITY (index 5): the `NoGravity` marker.
fn no_gravity(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::no_gravity::NoGravity>(e).map(|_| SyncValue::Bool(true))
}

/// AgeableMob.AGE_LOCKED (index 17, after the baby flag): the golden dandelion's lock (`AgeLocked`).
fn age_locked(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::age_lock::AgeLocked>(e).map(|_| SyncValue::Bool(true))
}

/// The core's slots, in index order.
pub static SLOTS: &[SyncSlot] = &[
    SyncSlot { index: 0, read: shared },
    SyncSlot { index: 1, read: air },
    SyncSlot { index: 2, read: custom_name },
    SyncSlot { index: 3, read: name_visible },
    SyncSlot { index: 4, read: silent },
    SyncSlot { index: 5, read: no_gravity },
    SyncSlot { index: 7, read: frozen },
    SyncSlot { index: 9, read: health },
    SyncSlot { index: 11, read: ambient },
    SyncSlot { index: 14, read: sleeping },
    SyncSlot { index: 15, read: mob_flags },
    SyncSlot { index: 16, read: baby },
    SyncSlot { index: 17, read: age_locked },
];
