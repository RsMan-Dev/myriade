//! The keys of the stands, frames, carts and riders (`nbt_fields::rides`): each kind saves the type the jar writes, reads it back to the same
//! NBT, and refuses a wrong type; a mount with two riders, one of them mounted in turn, saves ONE root compound that restores the same, and
//! `save_all` does not write a rider twice.
use bevy_ecs::prelude::*;
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, JavaString, Tag},
};
use horizon_ecs_proto::{
    cow::{spawn_species, species_table, Herd, Pos, TypeName},
    hanging::{Attached, Frame},
    minecart::{Furnace, Fuse},
    mount::{ride, Mount, Riding},
    nbt::{self, accept, Field, NbtData, Saved},
    stand::{DisabledSlots, Pose, Stand, DEFAULT_POSE, MARKER, NO_BASE_PLATE, SHOW_ARMS, SMALL},
    targeting::Who,
};
use std::borrow::Cow;

fn key(k: &str) -> JavaString {
    JavaString::from(k)
}

fn float(v: f32) -> Tag {
    Tag::Float(v.to_bits())
}

fn double(v: f64) -> Tag {
    Tag::Double(v.to_bits())
}

#[test]
fn an_armor_stand_saves_its_flags_pose_and_slots_as_the_jar_does_and_reads_them_back() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let mut pose = DEFAULT_POSE;
    pose[0] = [10.0, 0.0, 0.0];
    let stand = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:armor_stand"), Saved, Stand { flags: SMALL | SHOW_ARMS | MARKER }, Pose(Box::new(pose)), DisabledSlots(5))).id();
    assert_eq!(nbt::get(&h.world, stand, "Small"), Some(Tag::Byte(1)));
    assert_eq!(nbt::get(&h.world, stand, "ShowArms"), Some(Tag::Byte(1)));
    assert_eq!(nbt::get(&h.world, stand, "NoBasePlate"), Some(Tag::Byte(0)));
    assert_eq!(nbt::get(&h.world, stand, "Marker"), Some(Tag::Byte(1)));
    assert_eq!(nbt::get(&h.world, stand, "DisabledSlots"), Some(Tag::Int(5)));
    let Some(Tag::Compound(parts)) = nbt::get(&h.world, stand, "Pose") else { panic!("a Pose compound") };
    assert_eq!(parts.len(), 1, "only the part that differs from the default");
    assert_eq!(parts.get(&key("Head")), Some(&Tag::List(vec![float(10.0), float(0.0), float(0.0)])));

    let c = nbt::save(&h.world, stand).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), TypeName("minecraft:armor_stand"), Saved, Stand::default(), Pose(Box::new(DEFAULT_POSE)), DisabledSlots(0))).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(nbt::save(&h.world, twin), Some(c.clone()));
    assert_eq!(h.world.get::<Stand>(twin).copied(), Some(Stand { flags: SMALL | SHOW_ARMS | MARKER }));

    let mut bad = c.clone();
    bad.insert(key("Small"), Tag::Int(1));
    bad.insert(key("NoBasePlate"), Tag::Byte(1));
    assert_eq!(nbt::load(&mut h.world, twin, &bad), Some(1), "an Int for a flag is refused");
    assert_eq!(h.world.get::<Stand>(twin).map(|s| s.flags & NO_BASE_PLATE), Some(NO_BASE_PLATE), "the other keys still apply");
}

#[test]
fn a_frame_saves_facing_block_and_rotation_and_refuses_a_bad_facing() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let frame = h.world.spawn((Pos(Vec3::new(1.5, 64.0, -1.5)), TypeName("minecraft:item_frame"), Saved, Frame { rotation: 3, item: None }, Attached { pos: [1, 64, -2], face: 2, kind: 0, born: 0 })).id();
    assert_eq!(nbt::get(&h.world, frame, "Facing"), Some(Tag::Byte(2)));
    assert_eq!(nbt::get(&h.world, frame, "block_pos"), Some(Tag::IntArray(vec![1, 64, -2])));
    assert_eq!(nbt::get(&h.world, frame, "ItemRotation"), Some(Tag::Byte(3)));

    let c = nbt::save(&h.world, frame).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(1.5, 64.0, -1.5)), TypeName("minecraft:item_frame"), Saved, Frame { rotation: 0, item: None }, Attached { pos: [0, 0, 0], face: 0, kind: 0, born: 0 })).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(nbt::save(&h.world, twin), Some(c.clone()));

    let mut bad = c.clone();
    bad.insert(key("Facing"), Tag::Byte(9));
    assert_eq!(nbt::load(&mut h.world, twin, &bad), Some(1), "a facing outside the six is refused");
    assert_eq!(h.world.get::<Attached>(twin).map(|a| a.face), Some(2));
}

#[test]
fn a_furnace_and_a_tnt_cart_save_their_keys_and_refuse_a_wrong_type() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let furnace = h.world.spawn((Pos(Vec3::new(0.5, 1.0, 0.5)), TypeName("minecraft:furnace_minecart"), Saved, Furnace { fuel: 4, push: Vec3::new(0.25, 0.0, -0.5) }, Fuse(40))).id();
    assert_eq!(nbt::get(&h.world, furnace, "PushX"), Some(double(0.25)));
    assert_eq!(nbt::get(&h.world, furnace, "PushZ"), Some(double(-0.5)));
    assert_eq!(nbt::get(&h.world, furnace, "Fuel"), Some(Tag::Short(4)));
    assert_eq!(nbt::get(&h.world, furnace, "fuse"), Some(Tag::Int(40)));

    let c = nbt::save(&h.world, furnace).expect("saved");
    let twin = h.world.spawn((Pos(Vec3::new(0.5, 1.0, 0.5)), TypeName("minecraft:furnace_minecart"), Saved, Furnace { fuel: 0, push: Vec3::new(0.0, 0.0, 0.0) }, Fuse(0))).id();
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(nbt::save(&h.world, twin), Some(c.clone()));

    let mut bad = c.clone();
    bad.insert(key("Fuel"), Tag::Int(4));
    assert_eq!(nbt::load(&mut h.world, twin, &bad), Some(1), "an Int for the fuel is refused");
}

#[test]
fn a_mount_saves_its_riders_under_one_root_and_restores_them_seated() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let species = species_table().0;
    let find = |name: &str| species.iter().find(|s| s.name == name).expect("the species").clone();
    let camel = find("minecraft:camel");
    let cow = find("minecraft:cow");
    let root = spawn_species(&mut a.world, &camel, 0.5, 0.5, 1);
    let first = spawn_species(&mut a.world, &cow, 0.5, 0.5, 2);
    let second = spawn_species(&mut a.world, &camel, 0.5, 0.5, 3);
    let third = spawn_species(&mut a.world, &cow, 0.5, 0.5, 4);
    assert!(ride(&mut a.world, root, Who::Entity(first)));
    assert!(ride(&mut a.world, root, Who::Entity(second)));
    assert!(ride(&mut a.world, second, Who::Entity(third)));

    let mut saved = Vec::new();
    nbt::save_all(&mut a.world, |e, c| saved.push((e, c)));
    assert_eq!(saved.len(), 1, "the riders are in their mount's NBT, not saved a second time");
    let (saved_root, c) = saved.remove(0);
    assert_eq!(saved_root, root);
    let Some(Tag::List(riders)) = c.get(&key("Passengers")).cloned() else { panic!("the Passengers list") };
    assert_eq!(riders.len(), 2);
    let Tag::Compound(inner) = &riders[1] else { panic!("the second rider is a compound") };
    assert!(matches!(inner.get(&key("Passengers")), Some(Tag::List(l)) if l.len() == 1), "the rider mounted in turn keeps its own rider");

    let mut b = Herd::new(0, 1.0, true, false, 1);
    let back = nbt::restore(&mut b.world, &c).expect("the camel back");
    assert_eq!(nbt::save(&b.world, back), Some(c.clone()), "the same NBT once restored");
    let mount = b.world.get::<Mount>(back).copied().expect("a mount");
    assert!(mount.rider.is_some() && mount.passenger.is_some(), "both seats taken");
    assert!(b.world.get::<Riding>(back).is_none(), "the root rides nothing");
}

/// A key no field declares until a test registers it: an Int read into a `Fuse`, so the creator's effect is visible.
fn foo() -> Field {
    fn write(_: &World, _: Entity, _: &mut Compound) {}
    fn read(w: &mut World, e: Entity, t: &Tag) -> bool {
        let Tag::Int(v) = t else { return false };
        w.entity_mut(e).insert(Fuse(*v));
        true
    }
    Field { key: Cow::Borrowed("Foo"), owner: None, accepts: accept::int, write, read }
}

#[test]
fn a_component_added_in_play_is_saved_and_a_creator_gives_it_back() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    h.world.entity_mut(e).insert(Fuse(40));
    let c = nbt::save(&h.world, e).expect("saved");
    assert_eq!(c.get(&key("fuse")), Some(&Tag::Int(40)));

    let twin = spawn_species(&mut h.world, &cow, 0.5, 0.5, 2);
    assert!(h.world.get::<Fuse>(twin).is_none());
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(h.world.get::<Fuse>(twin).map(|f| f.0), Some(40), "the creator of the key rebuilds the component");
}

#[test]
fn a_free_key_is_kept_until_a_creator_claims_it() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    let mut c = nbt::save(&h.world, e).expect("saved");
    c.insert(key("Foo"), Tag::Int(7));
    let twin = spawn_species(&mut h.world, &cow, 0.5, 0.5, 2);
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(nbt::get(&h.world, twin, "Foo"), Some(Tag::Int(7)), "no field declares it: kept free");

    nbt::register_creators(&mut h.world, &[foo()]);
    assert_eq!(nbt::get(&h.world, twin, "Foo"), None, "claimed by the late creator");
    assert_eq!(h.world.get::<Fuse>(twin).map(|f| f.0), Some(7));
}

#[test]
fn two_creators_of_one_key_leave_it_free() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    let mut c = nbt::save(&h.world, e).expect("saved");
    c.insert(key("Foo"), Tag::Int(7));
    let twin = spawn_species(&mut h.world, &cow, 0.5, 0.5, 2);
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));

    nbt::register_creators(&mut h.world, &[foo(), foo()]);
    assert_eq!(nbt::get(&h.world, twin, "Foo"), Some(Tag::Int(7)), "two creators: the key stays free");
    assert!(h.world.get::<Fuse>(twin).is_none());
}

#[test]
fn a_common_key_at_its_default_is_not_kept_free() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    let mut c = nbt::save(&h.world, e).expect("saved");
    c.insert(key("OnGround"), Tag::Byte(0));
    let twin = spawn_species(&mut h.world, &cow, 0.5, 0.5, 2);
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(nbt::get(&h.world, twin, "OnGround"), None, "the default is dropped");
    assert!(h.world.get::<NbtData>(twin).is_none(), "no NBT left on the entity");

    c.insert(key("OnGround"), Tag::Byte(1));
    assert_eq!(nbt::load(&mut h.world, twin, &c), Some(0));
    assert_eq!(nbt::get(&h.world, twin, "OnGround"), Some(Tag::Byte(1)), "a value off the default is kept");
}

#[test]
fn an_id_no_species_declares_is_flagged_outside_the_window_and_not_kept() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    let mut c = nbt::save(&h.world, e).expect("saved");
    c.insert(key("id"), Tag::String(JavaString::from("minecraft:not_a_species")));

    assert!(nbt::restore(&mut h.world, &c).is_none());
    assert_eq!(h.world.query_filtered::<Entity, With<Saved>>().iter(&h.world).count(), 1, "only the cow exists");
    assert!(nbt::has_unknown(&h.world, 0, 0), "the chunk is flagged");
    assert_eq!(nbt::keep_unknown(&mut h.world, 0, 0, std::iter::once(c.clone())), vec![c.clone()], "the NBT read back, whole");

    let marked = nbt::declare(&mut h.world, "minecraft:not_a_species", nbt::Recipe { saved: true, build: nbt::Build::With(refused) });
    assert_eq!(marked, vec![(0, 0)], "the declaration outside the window names the flagged chunk to reread");
    assert!(nbt::keep_unknown(&mut h.world, 0, 0, std::iter::once(c.clone())).is_empty(), "declared now, not kept");
    nbt::forget_chunk(&mut h.world, 0, 0);
    assert!(!nbt::has_unknown(&h.world, 0, 0), "the flag goes with the chunk");
}

#[test]
fn a_saved_entity_with_no_species_is_not_saved() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    h.world.spawn((Pos(Vec3::new(0.5, 1.0, 0.5)), Saved));
    let mut saved = 0;
    nbt::save_all(&mut h.world, |_, _| saved += 1);
    assert_eq!(saved, 0);
}

fn built(w: &mut World, at: Vec3, _: &Compound) -> Option<Entity> {
    Some(w.spawn((Pos(at), Saved)).id())
}

fn refused(_: &mut World, _: Vec3, _: &Compound) -> Option<Entity> {
    None
}

#[test]
fn a_declared_builder_makes_the_entity_and_the_keys_load_on_it() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    let mut c = nbt::save(&h.world, e).expect("saved");
    c.insert(key("OnGround"), Tag::Byte(1));
    nbt::declare(&mut h.world, "minecraft:cow", nbt::Recipe { saved: true, build: nbt::Build::With(built) });

    let back = nbt::restore(&mut h.world, &c).expect("the builder's entity");
    assert!(h.world.get::<Saved>(back).is_some());
    assert_eq!(nbt::get(&h.world, back, "OnGround"), Some(Tag::Byte(1)), "the keys are loaded on the built entity");
    assert_eq!(h.world.query_filtered::<Entity, With<Saved>>().iter(&h.world).count(), 2);
}

#[test]
fn a_builder_that_makes_nothing_in_the_window_keeps_the_compound_until_the_window_closes() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    let c = nbt::save(&h.world, e).expect("saved");
    nbt::begin_declarations(&mut h.world);
    nbt::declare(&mut h.world, "minecraft:cow", nbt::Recipe { saved: true, build: nbt::Build::With(refused) });
    assert!(nbt::restore(&mut h.world, &c).is_none());
    assert_eq!(h.world.query_filtered::<Entity, With<Saved>>().iter(&h.world).count(), 1, "only the cow exists");
    assert!(!nbt::has_unknown(&h.world, 0, 0), "in the window nothing is flagged yet");

    nbt::end_declarations(&mut h.world);
    assert!(nbt::has_unknown(&h.world, 0, 0), "the window closed with it pending: its chunk is flagged");
}

#[test]
fn a_pending_compound_is_built_when_its_id_is_declared_in_the_window() {
    let mut h = Herd::new(0, 1.0, true, false, 1);
    let cow = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let e = spawn_species(&mut h.world, &cow, 0.5, 0.5, 1);
    let mut c = nbt::save(&h.world, e).expect("saved");
    c.insert(key("id"), Tag::String(JavaString::from("minecraft:not_a_species")));
    nbt::begin_declarations(&mut h.world);
    assert!(nbt::restore(&mut h.world, &c).is_none());

    let marked = nbt::declare(&mut h.world, "minecraft:not_a_species", nbt::Recipe { saved: true, build: nbt::Build::With(built) });
    assert!(marked.is_empty(), "in the window nothing is flagged");
    assert_eq!(h.world.query_filtered::<Entity, With<Saved>>().iter(&h.world).count(), 2, "the declaration built it");
    nbt::end_declarations(&mut h.world);
    assert!(!nbt::has_unknown(&h.world, 0, 0), "built, so nothing flagged");
}
