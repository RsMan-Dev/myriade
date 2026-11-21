//! The synched entries of the species whose state has a component of P (greffe L0, lot 05): one row per registry id, its slots
//! read those components. A slot gives an entry only when its value differs from the default. The state without a component
//! (guardian target, horse and cat variants...) has no row yet: see the lot's report.
use super::sync::{SyncSlot, SyncValue};
use super::uuids::EntityUuid;
use crate::behaviours::{beg::Begging, convert_timer::Converting, posture, variant::Variant};
use crate::farm::Wool;
use crate::size::Size;
use crate::tame::Owner;
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// Sheep.DATA_WOOL_ID: the colour in the low nibble, 16 when sheared; sent when not 0.
fn wool(w: &World, e: Entity) -> Option<SyncValue> {
    let wool = w.get::<Wool>(e)?;
    let bits = (wool.color & 15) | if wool.sheared { 16 } else { 0 };
    (bits != 0).then_some(SyncValue::Byte(bits as i8))
}

/// AbstractCubeMob.ID_SIZE: sent above 1, the default.
fn size(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Size>(e).filter(|s| s.0 > 1).map(|s| SyncValue::Int(s.0))
}

/// Fox.DATA_TYPE_ID and MushroomCow.DATA_TYPE: the variant, sent when not 0.
fn variant(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Variant>(e).filter(|v| v.0 != 0).map(|v| SyncValue::Int(i32::from(v.0)))
}

/// ZombieVillager.DATA_CONVERTING_ID: the cure's shaking, while the conversion timer runs.
fn converting(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Converting>(e).map(|_| SyncValue::Bool(true))
}

/// Wolf.DATA_INTERESTED_ID: BegGoal's flag, the head tilt while it begs.
fn interested(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Begging>(e).map(|_| SyncValue::Bool(true))
}

/// TamableAnimal.DATA_FLAGS_ID (index 18 after AgeableMob's 17): sitting 0x01, tame 0x04 (the tame state is `Owner`).
fn tame_flags(w: &World, e: Entity) -> Option<SyncValue> {
    let bits = i8::from(posture::is_sitting(w, e)) | i8::from(w.get::<Owner>(e).is_some_and(|o| o.0.is_some())) << 2;
    (bits != 0).then_some(SyncValue::Byte(bits))
}

/// TamableAnimal.DATA_OWNERUUID_ID (index 19): the owner's uuid. Only an owner entity's uuid is in P (`EntityUuid`); a player
/// owner's uuid is the host's, so it sends nothing yet (see the lot's report).
fn tame_owner(w: &World, e: Entity) -> Option<SyncValue> {
    let Who::Entity(owner) = w.get::<Owner>(e)?.0? else {
        return None;
    };
    w.get::<EntityUuid>(owner).map(|u| SyncValue::OptUuid(Some(u.0)))
}

/// The species' rows, by registry id: the slots of the components they own (the core's are in `sync_core`).
/// Sans équivalent (pas de composant dans P) : guardian, bat, snow_golem, piglin, parrot and cat variants, wolf (collar, variant,
/// anger), horse family, happy_ghast, villager and zombie_villager data, phantom, creeper, sulfur_cube fuse and from_bucket.
pub static TABLE: &[(&str, &[SyncSlot])] = &[
    ("minecraft:sheep", &[SyncSlot { index: 18, read: wool }]),
    ("minecraft:slime", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:magma_cube", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:sulfur_cube", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:fox", &[SyncSlot { index: 18, read: variant }]),
    ("minecraft:mooshroom", &[SyncSlot { index: 18, read: variant }]),
    ("minecraft:zombie_villager", &[SyncSlot { index: 19, read: converting }]),
    (
        "minecraft:wolf",
        &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }, SyncSlot { index: 20, read: interested }],
    ),
    ("minecraft:cat", &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }]),
    ("minecraft:parrot", &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }]),
];
