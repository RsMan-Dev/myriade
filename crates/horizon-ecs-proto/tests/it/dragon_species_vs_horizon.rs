//! The ender dragon's species and its save: a dragon born by `/summon` or an egg hovers (the jar's constructor, `EnderDragon`), with
//! 200 health from its type; its save writes the three fields the jar writes (`DragonPhase`, `DragonDeathTime`,
//! `sitting_damage_received`) and a load gives them back.
use crate::common;
use common::*;
use horizon_ecs_proto::{
    cow::*,
    dragon::{death::DeathTime, perch::SittingDamage, phases::PhaseId, save},
    nbt::{self},
    species::ender_dragon::START_PHASE,
};
use horizon_core::nbt::{Compound, Tag};

#[test]
fn a_dragon_is_born_hovering_with_its_type_health() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
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
    nbt::register(&mut w, save::FIELDS);
    let e = w.spawn((PhaseId(3), DeathTime(42), SittingDamage(12.5), TypeName("minecraft:ender_dragon"), nbt::Saved)).id();
    let saved = nbt::save(&w, e).expect("an NBT");
    let keys: Vec<String> = saved.iter().filter(|(k, v)| !horizon_ecs_proto::nbt_fields::common::drop_default(&k.to_string_lossy(), v)).map(|(k, _)| k.to_string_lossy()).collect();
    assert_eq!(keys, ["DragonDeathTime", "DragonPhase", "sitting_damage_received"]);
    assert!(matches!(nbt::get(&w, e, "DragonPhase"), Some(Tag::Int(3))));
    assert!(matches!(nbt::get(&w, e, "DragonDeathTime"), Some(Tag::Int(42))));
    assert!(matches!(nbt::get(&w, e, "sitting_damage_received"), Some(Tag::Float(b)) if f32::from_bits(b) == 12.5));
}

#[test]
fn the_dragon_fields_survive_a_save_and_a_load() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:ender_dragon").expect("the ender dragon");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.entity_mut(e).insert((DeathTime(42), SittingDamage(12.5)));
    let saved = nbt::save(&h.world, e).expect("an NBT");

    let f = spawn_species(&mut h.world, &sp, 4.5, 4.5, 8);
    h.world.entity_mut(f).insert((DeathTime(0), SittingDamage(0.0)));
    nbt::load(&mut h.world, f, &saved);
    assert_eq!(h.world.get::<PhaseId>(f).map(|p| p.0), h.world.get::<PhaseId>(e).map(|p| p.0));
    assert_eq!(h.world.get::<DeathTime>(f).map(|d| d.0), Some(42));
    assert_eq!(h.world.get::<SittingDamage>(f).map(|s| s.0), Some(12.5));
}

#[test]
fn a_load_without_the_fields_leaves_the_dragon_as_it_is() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:ender_dragon").expect("the ender dragon");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    let before = h.world.get::<DeathTime>(e).map(|d| d.0);
    nbt::load(&mut h.world, e, &Compound::new());
    assert_eq!(h.world.get::<DeathTime>(e).map(|d| d.0), before);
}
