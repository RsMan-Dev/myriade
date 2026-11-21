//! The registry of identifiers: `Saved` is posed only from the recipe of the id, an undeclared or unsaved entity carries none and is not
//! saved, and a saved passive entity round-trips through its recipe.
use bevy_ecs::prelude::*;
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, JavaString, Tag},
};
use horizon_ecs_proto::{
    cow::{spawn_species, species_table, Herd, Pos, TypeName},
    nbt::{self, Saved},
    passive,
    projectile::{launch, Flight},
};

#[test]
fn an_undeclared_id_poses_no_saved_and_is_not_saved() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let saved = nbt::is_saved(&h.world, "test:nothing").then_some(Saved);
    let e = h.world.spawn((Pos(Vec3::new(0.5, 1.0, 0.5)), TypeName("test:nothing"), saved)).id();
    assert!(!nbt::declared(&h.world, "test:nothing"));
    assert!(h.world.get::<Saved>(e).is_none(), "no recipe, no Saved");
    let mut saved_count = 0;
    nbt::save_all(&mut h.world, |_, _| saved_count += 1);
    assert_eq!(saved_count, 0, "an entity without Saved is not saved");
}

#[test]
fn a_species_declared_saved_false_poses_no_saved() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let knot = species_table().0.into_iter().find(|s| s.name == "minecraft:leash_knot").expect("the leash knot");
    assert!(!nbt::is_saved(&h.world, knot.name));
    let e = spawn_species(&mut h.world, &knot, 0.5, 0.5, 1);
    assert!(h.world.get::<Saved>(e).is_none(), "its recipe says not saved");
}

#[test]
fn a_declared_saved_species_poses_saved_at_creation() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    assert!(nbt::is_saved(&h.world, cow.name));
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    assert!(h.world.get::<Saved>(e).is_some());
    assert_eq!(h.world.get::<TypeName>(e).map(|t| t.0), Some("minecraft:cow"), "the id is the type name");
}

#[test]
fn a_passive_entity_round_trips_through_its_recipe() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let e = passive::spawn(&mut h.world, "minecraft:block_display", Vec3::new(1.0, 2.0, 3.0), Tag::Compound(Compound::new()));
    assert!(h.world.get::<Saved>(e).is_some(), "the passive recipe is saved");
    let c = nbt::save(&h.world, e).expect("saved");

    let mut back_world = Herd::new(0, 1.0, true, false, 1);
    let back = nbt::restore(&mut back_world.world, &c).expect("restored");
    assert!(back_world.world.get::<Saved>(back).is_some());
    assert_eq!(back_world.world.get::<TypeName>(back).map(|t| t.0), Some("minecraft:block_display"));
}

fn made_late(w: &mut World, at: Vec3, _: &Compound) -> Option<Entity> {
    Some(w.spawn((Pos(at), Saved, TypeName("minecraft:late"))).id())
}

fn count_of(w: &mut World, id: &str) -> usize {
    w.query::<&TypeName>().iter(w).filter(|t| t.0 == id).count()
}

#[test]
fn a_launched_arrow_is_saved_under_its_id_and_comes_back_with_it() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let e = launch(&mut h.world, Flight::ARROW, Vec3::new(0.5, 2.0, 0.5), Vec3::new(0.0, 0.0, 1.0), None);
    assert!(h.world.get::<Saved>(e).is_some(), "the arrow's id is declared saved");
    let c = nbt::save(&h.world, e).expect("saved");
    assert_eq!(c.get(&JavaString::from("id")), Some(&Tag::String(JavaString::from("minecraft:arrow"))));

    let mut back_world = Herd::new(0, 1.0, true, false, 1);
    let back = nbt::restore(&mut back_world.world, &c).expect("restored");
    assert!(back_world.world.get::<Saved>(back).is_some());
    assert_eq!(back_world.world.get::<Flight>(back).map(|f| f.id), Some("minecraft:arrow"));
}

#[test]
fn a_thrown_trident_is_saved_by_its_own_recipe() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let e = launch(&mut h.world, Flight::TRIDENT, Vec3::new(0.5, 2.0, 0.5), Vec3::new(0.0, 0.0, 1.0), None);
    assert!(h.world.get::<Saved>(e).is_some(), "the origin saves a thrown trident (`save_trident`)");
    let mut saved = 0;
    nbt::save_all(&mut h.world, |_, _| saved += 1);
    assert_eq!(saved, 1);
}

#[test]
fn populate_builds_only_the_just_declared_id_and_clears_the_flag() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    let cow_nbt = nbt::save(&h.world, e).expect("saved");
    let mut late = cow_nbt.clone();
    late.insert(JavaString::from("id"), Tag::String(JavaString::from("minecraft:late")));
    assert!(nbt::restore(&mut h.world, &late).is_none());
    assert!(nbt::has_unknown(&h.world, 0, 0));

    let marked = nbt::declare(&mut h.world, "minecraft:late", nbt::Recipe { saved: true, build: nbt::Build::With(made_late) });
    assert_eq!(marked, vec![(0, 0)], "the late declaration names the flagged chunk");
    let made = nbt::populate_unknown(&mut h.world, 0, 0, "minecraft:late", vec![cow_nbt, late].into_iter());
    assert_eq!(made, 1, "only the late id is built");
    assert_eq!(count_of(&mut h.world, "minecraft:cow"), 1, "the living cow is not built again");
    assert_eq!(count_of(&mut h.world, "minecraft:late"), 1);
    assert!(!nbt::has_unknown(&h.world, 0, 0), "nothing unknown left: the flag goes");
}
