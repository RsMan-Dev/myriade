//! The ender dragon's species and its save: a dragon born by `/summon` or an egg hovers (the jar's constructor, `EnderDragon`), with
//! 200 health from its type; its save writes the three fields the jar writes (`DragonPhase`, `DragonDeathTime`,
//! `sitting_damage_received`) and a load gives them back.
use crate::common;
use common::*;
use horizon_ecs_proto::{
    cow::*,
    dragon::{death::DeathTime, perch::SittingDamage, phases::PhaseId, save},
    species::ender_dragon::START_PHASE,
};
use horizon_core::nbt::Tag;
use std::collections::HashMap;

#[test]
fn a_dragon_is_born_hovering_with_its_type_health() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:ender_dragon").expect("the dragon's species");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    assert_eq!(h.world.get::<PhaseId>(e).expect("a phase").0, START_PHASE);
    assert_eq!(START_PHASE, 10, "EnderDragonPhase.HOVERING");
    assert_eq!(h.world.get::<Health>(e).expect("a health").0, 200.0);
}

#[test]
fn the_dragon_save_keys_are_the_jars() {
    let mut w = bevy_ecs::world::World::new();
    let e = w.spawn((PhaseId(3), DeathTime(42), SittingDamage(12.5))).id();
    let saved: HashMap<&str, Tag> = save::save(&w, e).into_iter().collect();
    let keys: Vec<&str> = {
        let mut k: Vec<&str> = saved.keys().copied().collect();
        k.sort_unstable();
        k
    };
    assert_eq!(keys, ["DragonDeathTime", "DragonPhase", "sitting_damage_received"]);
    assert!(matches!(saved.get("DragonPhase"), Some(Tag::Int(3))));
    assert!(matches!(saved.get("DragonDeathTime"), Some(Tag::Int(42))));
    assert!(matches!(saved.get("sitting_damage_received"), Some(Tag::Float(b)) if f32::from_bits(*b) == 12.5));
}

#[test]
fn the_dragon_fields_survive_a_save_and_a_load() {
    let mut w = bevy_ecs::world::World::new();
    let e = w.spawn((PhaseId(10), DeathTime(0), SittingDamage(0.0))).id();
    let fields = save::save(&w, e);
    let read = save::read(|k| fields.iter().find(|(n, _)| *n == k).map(|(_, t)| t));
    assert_eq!(read, save::Saved { phase: Some(10), death_time: 0, sitting_damage: 0.0 });

    let f = w.spawn_empty().id();
    save::load(&mut w, f, save::Saved { phase: None, death_time: 150, sitting_damage: 33.0 });
    assert_eq!(w.get::<DeathTime>(f).map(|d| d.0), Some(150));
    assert_eq!(w.get::<SittingDamage>(f).map(|s| s.0), Some(33.0));
}

#[test]
fn a_save_without_the_fields_reads_as_absent() {
    let read = save::read(|_| None);
    assert_eq!(read, save::Saved { phase: None, death_time: 0, sitting_damage: 0.0 });
}
