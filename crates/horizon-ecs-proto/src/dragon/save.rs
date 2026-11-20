//! The dragon's own save fields (`EnderDragon.addAdditionalSaveData` / `readAdditionalSaveData`): `DragonPhase`, `DragonDeathTime`,
//! `sitting_damage_received`. The Mob's fields (health, persistence, NoAI, equipment, attributes) belong to the framework.
use crate::dragon::{death::DeathTime, perch::SittingDamage, phases::{set_phase, PhaseId}};
use bevy_ecs::prelude::*;
use horizon_core::nbt::Tag;

/// The saved fields as read. `phase` is None when the save has no `DragonPhase`: the entity keeps the phase it was born in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Saved {
    pub phase: Option<u8>,
    pub death_time: i32,
    pub sitting_damage: f32,
}

/// The three fields, always written (`putInt`, `putInt`, `putFloat`), in the jar's order.
pub fn save(w: &World, e: Entity) -> Vec<(&'static str, Tag)> {
    let Some(phase) = w.get::<PhaseId>(e) else { return Vec::new() };
    vec![
        ("DragonPhase", Tag::Int(i32::from(phase.0))),
        ("DragonDeathTime", Tag::Int(w.get::<DeathTime>(e).map_or(0, |d| d.0))),
        ("sitting_damage_received", Tag::Float(w.get::<SittingDamage>(e).map_or(0.0, |s| s.0).to_bits())),
    ]
}

/// The fields of a save, by `get` (the key given, its tag); absent `DragonDeathTime` and `sitting_damage_received` read as 0.
pub fn read<'a>(get: impl Fn(&str) -> Option<&'a Tag>) -> Saved {
    let int = |k: &str| match get(k) {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    };
    Saved {
        phase: int("DragonPhase").and_then(|p| u8::try_from(p).ok()),
        death_time: int("DragonDeathTime").unwrap_or(0),
        sitting_damage: match get("sitting_damage_received") {
            Some(Tag::Float(bits)) => f32::from_bits(*bits),
            _ => 0.0,
        },
    }
}

/// Loads a save onto the dragon. As the jar does, the phase is set through `set_phase` (its begin runs), then the timers.
pub fn load(w: &mut World, e: Entity, s: Saved) {
    if let Some(p) = s.phase {
        set_phase(w, e, p);
    }
    w.entity_mut(e).insert((DeathTime(s.death_time), SittingDamage(s.sitting_damage)));
}
