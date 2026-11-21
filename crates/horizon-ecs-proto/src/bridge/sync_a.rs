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

/// SnowGolem.DATA_PUMPKIN_ID: 16 while the pumpkin is on, the default; sent as 0 once it is cut.
fn pumpkin(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::species::snow_golem::Pumpkin>(e).filter(|p| !p.0).map(|_| SyncValue::Byte(0))
}

/// Bogged.DATA_SHEARED (a bool): sent once the mushrooms are off.
fn sheared(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::dispense::Sheared>(e).map(|_| SyncValue::Bool(true))
}

/// Allay.DATA_DANCING (index 16, a bool): sent while the allay dances.
fn dancing(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::behaviours::event_listen::Dancing>(e).map(|_| SyncValue::Bool(true))
}

/// AbstractCubeMob.ID_SIZE: sent above 1, the default.
fn size(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Size>(e).filter(|s| s.0 > 1).map(|s| SyncValue::Int(s.0))
}

/// Fox.DATA_TYPE_ID, Rabbit.DATA_TYPE_ID (the default variant is id 0) and MushroomCow.DATA_TYPE: the variant, sent when not 0.
fn variant(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Variant>(e).filter(|v| v.0 != 0).map(|v| SyncValue::Int(i32::from(v.0)))
}

/// Salmon's variant (index 17, an int): sent when not 1, the default (medium).
fn salmon_type(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Variant>(e).filter(|v| v.0 != 1).map(|v| SyncValue::Int(i32::from(v.0)))
}

/// AbstractVillager.DATA_UNHAPPY_COUNTER: the ticks left of the shake (40 when it starts or restarts, `setUnhappyCounter(40)`), 0 by default.
fn unhappy(w: &World, e: Entity) -> Option<SyncValue> {
    let u = w.get::<crate::behaviours::unhappy::Unhappy>(e)?;
    let left = u.until.saturating_sub(crate::cow::now_of(w, e));
    (left > 0).then_some(SyncValue::Int(left as i32))
}

/// ZombieVillager.DATA_CONVERTING_ID: the cure's shaking, while the conversion timer runs.
fn converting(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Converting>(e).map(|_| SyncValue::Bool(true))
}

/// Wolf.DATA_INTERESTED_ID: BegGoal's flag, the head tilt while it begs.
fn interested(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Begging>(e).map(|_| SyncValue::Bool(true))
}

/// Wolf.DATA_COLLAR_COLOR (21) and Cat.DATA_COLLAR_COLOR (23), an int: sent once a dye was used (`Collar`).
fn collar(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::dye::Collar>(e).map(|c| SyncValue::Int(i32::from(c.0)))
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
/// Creaking.IS_TEARING_DOWN (index 18, a bool): sent while its death runs (`tear_down`).
fn tearing_down(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::tear_down::Crumbling>(e).filter(|c| c.sound.is_some()).map(|_| SyncValue::Bool(true))
}

/// Creaking.CAN_MOVE (index 16, a bool, true by default): sent as false while it stands still under a stare (`Stilled`).
fn can_move(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::cow::Stilled>(e).map(|_| SyncValue::Bool(false))
}

/// Creaking.IS_ACTIVE (index 17, a bool): sent while a stare has roused it (`Roused`).
fn is_active(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::behaviours::stare_freeze::Roused>(e).map(|_| SyncValue::Bool(true))
}

/// Sans équivalent (pas de composant dans P) : guardian, bat, piglin, parrot and cat variants, wolf (collar, variant,
/// anger), horse family, happy_ghast, villager and zombie_villager data, phantom, creeper, sulfur_cube fuse and from_bucket.
pub static TABLE: &[(&str, &[SyncSlot])] = &[
    ("minecraft:allay", &[SyncSlot { index: 16, read: dancing }]),
    ("minecraft:sheep", &[SyncSlot { index: 18, read: wool }]),
    ("minecraft:creaking", &[SyncSlot { index: 16, read: can_move }, SyncSlot { index: 17, read: is_active }, SyncSlot { index: 18, read: tearing_down }]),
    ("minecraft:snow_golem", &[SyncSlot { index: 16, read: pumpkin }]),
    ("minecraft:bogged", &[SyncSlot { index: 16, read: sheared }]),
    ("minecraft:slime", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:magma_cube", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:sulfur_cube", &[SyncSlot { index: 18, read: size }]),
    ("minecraft:fox", &[SyncSlot { index: 18, read: variant }]),
    ("minecraft:mooshroom", &[SyncSlot { index: 18, read: variant }]),
    ("minecraft:rabbit", &[SyncSlot { index: 18, read: variant }]),
    ("minecraft:zombie_villager", &[SyncSlot { index: 19, read: converting }]),
    ("minecraft:villager", &[SyncSlot { index: 18, read: unhappy }]),
    ("minecraft:wandering_trader", &[SyncSlot { index: 18, read: unhappy }]),
    (
        "minecraft:wolf",
        &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }, SyncSlot { index: 20, read: interested }, SyncSlot { index: 21, read: collar }],
    ),
    ("minecraft:cat", &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }, SyncSlot { index: 23, read: collar }]),
    ("minecraft:parrot", &[SyncSlot { index: 18, read: tame_flags }, SyncSlot { index: 19, read: tame_owner }, SyncSlot { index: 20, read: variant }]),
    ("minecraft:llama", &[SyncSlot { index: 21, read: variant }]),
    ("minecraft:salmon", &[SyncSlot { index: 17, read: salmon_type }]),
];
