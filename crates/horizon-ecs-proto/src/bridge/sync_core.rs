//! The entries every bridged creature shares, at vanilla's indices (Entity 0..=7, LivingEntity 8..=14, Mob 15), read from the
//! components. Indices with no slot have no generic source component in P: 2 (custom name) and 3 (name visible), 5 (no gravity),
//! 6 (pose, the species' own), 8 (using an item), 10 (effect particles: the effects' colours are not in P), 12 and 13 (stuck
//! arrows and stingers), and Mob 15's aggressive and left-handed bits.
use super::sync::{SyncSlot, SyncValue};
use crate::behaviours::bed::Sleeping;
use crate::behaviours::elder_curse::Silent;
use crate::cow::{Air, AirLow, Burning, Frozen, GrowAt, Health, now_of};
use crate::dragon::flight::NoAi;
use crate::effects::{AMBIENT, Effects, Invisible};
use bevy_ecs::prelude::*;

/// Entity.DATA_SHARED_FLAGS_ID: on fire 0x01, invisible 0x20.
fn shared(w: &World, e: Entity) -> Option<SyncValue> {
    let fire = w.get::<Burning>(e).is_some_and(|b| b.until > now_of(w, e));
    let bits = i8::from(fire) | i8::from(w.entity(e).contains::<Invisible>()) << 5;
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

/// Mob.DATA_MOB_FLAGS_ID: no AI 1.
fn mob_flags(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<NoAi>(e).map(|_| SyncValue::Byte(1))
}

/// AgeableMob.DATA_BABY_ID (index 16): the baby flag, while the entity has its growing timer (`GrowAt`).
fn baby(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<GrowAt>(e).map(|_| SyncValue::Bool(true))
}

/// The core's slots, in index order.
pub static SLOTS: &[SyncSlot] = &[
    SyncSlot { index: 0, read: shared },
    SyncSlot { index: 1, read: air },
    SyncSlot { index: 4, read: silent },
    SyncSlot { index: 7, read: frozen },
    SyncSlot { index: 9, read: health },
    SyncSlot { index: 11, read: ambient },
    SyncSlot { index: 14, read: sleeping },
    SyncSlot { index: 15, read: mob_flags },
    SyncSlot { index: 16, read: baby },
];
