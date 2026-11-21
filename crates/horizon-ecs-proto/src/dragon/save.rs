//! The dragon's own save fields (`EnderDragon.addAdditionalSaveData` / `readAdditionalSaveData`): `DragonPhase`, `DragonDeathTime`,
//! `sitting_damage_received`, declared to the NBT registry (`FIELDS`). The Mob's fields (health, persistence, NoAI, equipment, attributes)
//! belong to the framework.
use crate::dragon::{death::DeathTime, perch::SittingDamage, phases::{set_phase, PhaseId}};
use crate::nbt::{owner, Field};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

/// The dragon's three keys, in the jar's order of writing.
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("DragonPhase"), write: write_phase, read: read_phase, accepts: crate::nbt::accept::int, owner: Some(owner::<PhaseId>) },
    Field { key: Cow::Borrowed("DragonDeathTime"), write: write_death_time, read: read_death_time, accepts: crate::nbt::accept::int, owner: Some(owner::<DeathTime>) },
    Field { key: Cow::Borrowed("sitting_damage_received"), write: write_sitting, read: read_sitting, accepts: crate::nbt::accept::float, owner: Some(owner::<SittingDamage>) },
];

fn write_phase(w: &World, e: Entity, c: &mut Compound) {
    if let Some(p) = w.get::<PhaseId>(e) {
        c.insert("DragonPhase".into(), Tag::Int(i32::from(p.0)));
    }
}

/// As the jar does, the phase is set through `set_phase` (its begin runs).
fn read_phase(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Ok(p) = u8::try_from(*v) else { return false };
    set_phase(w, e, p);
    true
}

fn write_death_time(w: &World, e: Entity, c: &mut Compound) {
    if let Some(d) = w.get::<DeathTime>(e) {
        c.insert("DragonDeathTime".into(), Tag::Int(d.0));
    }
}

fn read_death_time(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(DeathTime(*v));
    true
}

fn write_sitting(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<SittingDamage>(e) {
        c.insert("sitting_damage_received".into(), Tag::Float(s.0.to_bits()));
    }
}

fn read_sitting(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Float(bits) = t else { return false };
    w.entity_mut(e).insert(SittingDamage(f32::from_bits(*bits)));
    true
}
