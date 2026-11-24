//! The entries every bridged creature shares, at vanilla's indices (Entity 0..=7, LivingEntity 8..=14, Mob 15), read from the
//! components. Indices with no slot have no generic source component in P: 6 (pose, the species' own), 8 (using an item), 10 (effect particles: the effects' colours are not in P), 12 and 13 (stuck
//! arrows and stingers), and Mob 15's aggressive and left-handed bits.
use super::sync::{SyncSlot, SyncValue};
use crate::behaviours::bed::Sleeping;
use crate::behaviours::elder_curse::Silent;
use crate::cow::{Air, AirLow, Burning, Frozen, GrowAt, Health, now_of};
use crate::dragon::flight::NoAi;
use crate::effects::{Kind, AMBIENT, Effects, HIDDEN, Invisible};
use horizon_protocol::entity_data::{Particle, ParticleData};
use bevy_ecs::prelude::*;
use horizon_core::nbt::Tag;
use horizon_protocol::entity_data::serializer;

/// Entity.DATA_SHARED_FLAGS_ID: on fire 0x01, invisible 0x20, glowing 0x40 (the saved `Glowing` key, `setGlowingTag`).
fn shared(w: &World, e: Entity) -> Option<SyncValue> {
    let fire = w.get::<Burning>(e).is_some_and(|b| b.until > now_of(w, e) as i64);
    let glow = w.entity(e).contains::<crate::glowing::Glowing>() || w.get::<Effects>(e).is_some_and(|f| f.list.iter().any(|x| x.kind == Kind::Glowing));
    let bits = i8::from(fire) | i8::from(w.entity(e).contains::<Invisible>()) << 5 | i8::from(glow) << 6;
    (bits != 0).then_some(SyncValue::Byte(bits))
}

/// Entity.DATA_AIR_SUPPLY: sent while the air is below its maximum (`AirLow`).
fn air(w: &World, e: Entity) -> Option<SyncValue> {
    // A water animal out of the water (`Dry`) runs its air down on its own (`WaterAnimal.handleAirSupply`).
    if w.get::<AirLow>(e).is_none() && (w.get::<crate::locomotion::swim::Dry>(e).is_none() || w.get::<crate::locomotion::swim::NoDrown>(e).is_some()) {
        return None;
    }
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

/// `MobEffect`'s colour (`MobEffects` constructor); None for the effects with their own particle (weaving, infested).
fn effect_color(k: Kind) -> Option<i32> {
    Some(match k {
        Kind::Speed => 3402751,
        Kind::Slowness => 9154528,
        Kind::Weakness => 4738376,
        Kind::Poison => 8889187,
        Kind::Wither => 7561558,
        Kind::Regeneration => 13458603,
        Kind::FireResistance => 16750848,
        Kind::WaterBreathing => 10017472,
        Kind::Invisibility => 16185078,
        Kind::Levitation => 0xceffff,
        Kind::Hunger => 5797459,
        Kind::MiningFatigue => 4866583,
        Kind::Blindness => 2039587,
        Kind::JumpBoost => 16646020,
        Kind::Glowing => 0x94a061,
        Kind::Strength => 16762624,
        Kind::Resistance => 9520880,
        Kind::Absorption => 0x2552a5,
        Kind::HealthBoost => 16284963,
        Kind::ConduitPower => 0x1dc2d1,
        Kind::BreathOfTheNautilus => 0x00ffee,
        Kind::SlowFalling => 0xf3cfb9,
        Kind::Haste => 14270531,
        Kind::Nausea => 5578058,
        Kind::NightVision => 12779366,
        Kind::Saturation => 16262179,
        Kind::Luck => 5882118,
        Kind::Unluck => 12624973,
        Kind::DolphinsGrace => 8954814,
        Kind::InstantHealth => 16262179,
        Kind::InstantDamage => 11101546,
        Kind::BadOmen => 745784,
        Kind::HeroOfTheVillage => 4521796,
        Kind::Darkness => 2696993,
        Kind::Weaving | Kind::Infested | Kind::Oozing | Kind::WindCharged | Kind::TrialOmen | Kind::RaidOmen => return None,
    })
}

/// LivingEntity.DATA_EFFECT_PARTICLES (index 10): `ENTITY_EFFECT` (28) of each visible effect, ARGB with alpha 255, 38 when ambient
/// (`MobEffect.AMBIENT_ALPHA`); the list is in the creature's own order (vanilla's is its map's).
fn effect_particles(w: &World, e: Entity) -> Option<SyncValue> {
    let list = &w.get::<Effects>(e)?.list;
    let ps: Vec<Particle> = list
        .iter()
        .filter(|f| f.flags & HIDDEN == 0)
        .filter_map(|f| {
            let alpha = if f.flags & AMBIENT != 0 { 38 } else { 255 };
            match f.kind {
                // The effects with their own `ParticleOptions` (`MobEffect(category, colour, particle)`): item_cobweb, infested, item_slime, small_gust, trial_omen, raid_omen.
                Kind::Weaving => Some(Particle { id: 63, data: None }),
                Kind::Infested => Some(Particle { id: 40, data: None }),
                Kind::Oozing => Some(Particle { id: 62, data: None }),
                Kind::WindCharged => Some(Particle { id: 32, data: None }),
                Kind::TrialOmen => Some(Particle { id: 124, data: None }),
                Kind::RaidOmen => Some(Particle { id: 123, data: None }),
                k => effect_color(k).map(|c| Particle { id: 28, data: Some(ParticleData::Color((alpha << 24) | c)) }),
            }
        })
        .collect();
    (!ps.is_empty()).then_some(SyncValue::Particles(ps))
}

/// LivingEntity.DATA_EFFECT_AMBIENCE_ID: sent when there are effects and none is both visible and not ambient (`areAllEffectsAmbient`).
fn ambient(w: &World, e: Entity) -> Option<SyncValue> {
    let list = &w.get::<Effects>(e)?.list;
    (!list.is_empty() && list.iter().all(|f| f.flags & (AMBIENT | HIDDEN) != 0)).then_some(SyncValue::Bool(true))
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

/// The kinds with a baby flag that are not an `AgeableMob` (the age lock at 17 does not exist for them).
pub const NOT_AGEABLE: &[&str] = &[
    "minecraft:zombie", "minecraft:husk", "minecraft:drowned", "minecraft:zombie_villager", "minecraft:zombified_piglin", "minecraft:giant", "minecraft:piglin", "minecraft:zoglin",
];

/// `Pose.DYING` (network id 7): the pose of a body that died (`LivingEntity.die`); no other pose is synced here.
fn pose(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::tear_down::DyingPose>(e).map(|_| SyncValue::Id(serializer::POSE, 7))
}

/// The core's slots, in index order.
pub static SLOTS: &[SyncSlot] = &[
    SyncSlot { index: 0, read: shared },
    SyncSlot { index: 1, read: air },
    SyncSlot { index: 2, read: custom_name },
    SyncSlot { index: 3, read: name_visible },
    SyncSlot { index: 4, read: silent },
    SyncSlot { index: 5, read: no_gravity },
    SyncSlot { index: 6, read: pose },
    SyncSlot { index: 7, read: frozen },
    SyncSlot { index: 9, read: health },
    SyncSlot { index: 10, read: effect_particles },
    SyncSlot { index: 11, read: ambient },
    SyncSlot { index: 14, read: sleeping },
    SyncSlot { index: 15, read: mob_flags },
    SyncSlot { index: 16, read: baby },
    SyncSlot { index: 17, read: age_locked },
];
