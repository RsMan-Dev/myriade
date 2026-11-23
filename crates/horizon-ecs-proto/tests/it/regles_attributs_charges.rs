//! The attribute instances a creature's load and taming leave in its `AttrMods`, saved as `attributes` (the vanilla 26.3 lists of the
//! Mob's `finalize_load_tests`: `AttributeMap.pack` writes the instances that exist, with the base of the species where none is set).
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::{
    attr_mod::{touched, AttrMods, Modifier, Operation},
    cow::{base_max_health, max_health, spawn_species, species_table, Health, Herd},
    effects::{self, Kind},
    nbt,
    tame,
    targeting::Who,
};
use myriade_engine_lib::items::ItemStack;
use horizon_ecs_proto::{
    behaviours::Interaction,
    cow::{Entered, GrowAt, Online, Player},
    goals::interact,
    items,
    used_stack::UsedStack,
};

const SPEED: &str = "movement_speed";
const HEALTH: &str = "max_health";
const DAMAGE: &str = "attack_damage";

fn comp(entries: impl IntoIterator<Item = (&'static str, Tag)>) -> Compound {
    match Tag::compound(entries) {
        Tag::Compound(c) => c,
        _ => unreachable!("Tag::compound makes a compound"),
    }
}

fn spawn(w: &mut World, name: &str) -> Entity {
    let sp = species_table().0.into_iter().find(|s| s.name == name).expect("a known species");
    spawn_species(w, &sp, 0.5, 0.5, 1)
}

/// The creature of `name` loaded with the keys `extra`.
fn loaded(w: &mut World, name: &str, extra: Vec<(&'static str, Tag)>) -> Entity {
    let e = spawn(w, name);
    assert!(nbt::load(w, e, &comp(extra)).is_some(), "{name} is saved");
    e
}

/// The saved `attributes` as (id without `minecraft:`, base) sorted by id: none when the creature saves no list.
fn saved(w: &World, e: Entity) -> Vec<(String, f64)> {
    let Some(Tag::List(list)) = nbt::get(w, e, "attributes") else { return Vec::new() };
    let mut out: Vec<(String, f64)> = list
        .iter()
        .map(|a| {
            let Tag::Compound(c) = a else { panic!("an instance is a compound") };
            let Some(Tag::String(id)) = c.get(&JavaString::from("id")) else { panic!("an instance with no id") };
            let Some(Tag::Double(base)) = c.get(&JavaString::from("base")) else { panic!("an instance with no base") };
            (id.to_string_lossy().trim_start_matches("minecraft:").to_owned(), f64::from_bits(*base))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn of(v: &[(&str, f64)]) -> Vec<(String, f64)> {
    v.iter().map(|(k, b)| ((*k).to_owned(), *b)).collect()
}

/// A saved `attributes` list with one instance of `id` at `base`.
fn saved_base(id: &str, base: f64) -> Tag {
    Tag::List(vec![Tag::compound([("id", Tag::String(format!("minecraft:{id}").as_str().into())), ("base", Tag::Double(base.to_bits()))])])
}

fn owner() -> (&'static str, Tag) {
    ("Owner", Tag::IntArray(vec![1, 2, 3, 4]))
}

fn baby() -> (&'static str, Tag) {
    ("IsBaby", Tag::Byte(1))
}

#[test]
fn an_enderman_load_makes_its_speed_instance() {
    // Enderman.setTarget reads movement_speed, and the load ends with it (readPersistentAngerSaveData).
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = loaded(&mut a.world, "minecraft:enderman", vec![]);
    assert_eq!(saved(&a.world, e), of(&[(SPEED, 0.300_000_011_920_928_96)]));
}

#[test]
fn a_wolf_loaded_without_an_owner_makes_its_health_instance() {
    // TamableAnimal.readAdditionalSaveData: setTame(false, true) -> Wolf.applyTamingSideEffects sets 8.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = loaded(&mut a.world, "minecraft:wolf", vec![]);
    assert_eq!(saved(&a.world, e), of(&[(HEALTH, 8.0)]));
}

#[test]
fn a_wolf_loaded_with_an_owner_keeps_the_saved_max_health() {
    // TamableAnimal.readAdditionalSaveData: setTame(true, false), the flag alone: no instance, and a saved base stays.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = loaded(&mut a.world, "minecraft:wolf", vec![owner()]);
    assert_eq!(saved(&a.world, e), of(&[]));
    let mut b = Herd::new(0, 1.0, true, false, 1);
    let e = loaded(&mut b.world, "minecraft:wolf", vec![owner(), ("attributes", saved_base(HEALTH, 80.0))]);
    assert_eq!(saved(&b.world, e), of(&[(HEALTH, 80.0)]));
}

#[test]
fn a_wolf_tamed_in_game_saves_its_forty_health() {
    // Wolf.applyTamingSideEffects (setTame(true, true)) sets the base to 40.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:wolf");
    tame::tame(&mut a.world, e, Who::Player(1), 40.0);
    assert_eq!(saved(&a.world, e), of(&[(HEALTH, 40.0)]));
}

#[test]
fn a_wolf_with_no_stored_base_reads_forty_health_once_tamed() {
    // A wolf tamed in game before anything stored its max_health base reads the 40 the taming sets, and a refresh keeps it.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:wolf");
    a.world.entity_mut(e).remove::<AttrMods>();
    assert_eq!(base_max_health(&a.world, e), 8.0);
    tame::tame(&mut a.world, e, Who::Player(1), 40.0);
    assert_eq!(base_max_health(&a.world, e), 40.0);
    assert_eq!(max_health(&a.world, e), 40.0);
    assert_eq!(a.world.get::<Health>(e).map(|h| h.0), Some(40.0));
}

#[test]
fn a_baby_goat_or_hoglin_makes_its_attack_instance() {
    // AgeableMob.setAge crosses the baby boundary: Goat/Hoglin.ageBoundaryReached; Zoglin.setBaby(true) sets 0.5 (not when adult).
    let cases = [
        ("minecraft:goat", vec![], of(&[])),
        ("minecraft:goat", vec![baby()], of(&[(DAMAGE, 1.0)])),
        ("minecraft:hoglin", vec![], of(&[])),
        ("minecraft:hoglin", vec![baby()], of(&[(DAMAGE, 0.5)])),
        ("minecraft:zoglin", vec![], of(&[])),
        ("minecraft:zoglin", vec![baby()], of(&[(DAMAGE, 0.5)])),
    ];
    for (name, extra, want) in cases {
        let mut a = Herd::new(0, 1.0, true, false, 1);
        let e = loaded(&mut a.world, name, extra);
        assert_eq!(saved(&a.world, e), want, "{name}");
    }
}

#[test]
fn the_other_species_stay_as_they_were() {
    let cases = [
        ("minecraft:pig", vec![], of(&[])),
        ("minecraft:cat", vec![owner()], of(&[])),
        ("minecraft:zombie", vec![], of(&[(SPEED, 0.230_000_004_172_325_13)])),
    ];
    for (name, extra, want) in cases {
        let mut a = Herd::new(0, 1.0, true, false, 1);
        let e = loaded(&mut a.world, name, extra);
        assert_eq!(saved(&a.world, e), want, "{name}");
    }
}

#[test]
fn a_wolf_with_a_health_modifier_keeps_its_taming_base() {
    // A max_health modifier changes the value, never the saved base: 8 for a wild wolf, the saved 40 of a tame one.
    let boost = Modifier { id: "test:boost".to_owned(), amount: 10.0, operation: Operation::AddValue, permanent: true };
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let wild = loaded(&mut a.world, "minecraft:wolf", vec![]);
    a.world.entity_mut(wild).insert(AttrMods(vec![(HEALTH.to_owned(), 8.0, vec![boost.clone()])]));
    touched(&mut a.world, wild, HEALTH);
    assert_eq!(saved(&a.world, wild), of(&[(HEALTH, 8.0)]));
    assert_eq!(max_health(&a.world, wild), 18.0);

    let mut b = Herd::new(0, 1.0, true, false, 1);
    let tame_wolf = loaded(&mut b.world, "minecraft:wolf", vec![owner(), ("attributes", saved_base(HEALTH, 40.0))]);
    b.world.entity_mut(tame_wolf).insert(AttrMods(vec![(HEALTH.to_owned(), 40.0, vec![boost])]));
    touched(&mut b.world, tame_wolf, HEALTH);
    assert_eq!(saved(&b.world, tame_wolf), of(&[(HEALTH, 40.0)]));
    assert_eq!(max_health(&b.world, tame_wolf), 50.0);
}

#[test]
fn a_health_boost_adds_four_per_level_and_its_end_takes_it_back() {
    // MobEffects.HEALTH_BOOST: +4 max health per level (ADD_VALUE, minecraft:effect.health_boost); the tamed 40 is the base under it.
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let e = spawn(&mut a.world, "minecraft:wolf");
    effects::add(&mut a.world, e, Kind::HealthBoost, 1, 200);
    assert_eq!(max_health(&a.world, e), 16.0);
    tame::tame(&mut a.world, e, Who::Player(1), 40.0);
    assert_eq!(max_health(&a.world, e), 48.0);
    assert_eq!(saved(&a.world, e), of(&[(HEALTH, 40.0)]));
    effects::remove(&mut a.world, e, Kind::HealthBoost);
    assert_eq!(max_health(&a.world, e), 40.0);
    assert_eq!(a.world.get::<Health>(e).map(|h| h.0), Some(40.0));
}

#[test]
fn a_goat_egg_baby_makes_its_attack_instance() {
    // The egg path has no nbt::load: make_baby sets its growth, then apply_load makes the Baby instance (spawn_offspring.rs).
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:goat").expect("a goat");
    let parent = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.resource_mut::<Entered>().0.push(parent);
    let egg = items::item_id("minecraft:goat_spawn_egg");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: egg, off: 0, ..Default::default() });
    h.world.insert_resource(UsedStack { stack: ItemStack::new("minecraft:goat_spawn_egg", 1), creative: false });
    assert!(interact(&mut h.world, parent, &Interaction { player: 1, item: egg }));
    let baby = h.world.query_filtered::<Entity, With<GrowAt>>().iter(&h.world).next().expect("a baby");
    assert_eq!(saved(&h.world, baby), of(&[(DAMAGE, 1.0)]));
}
