//! The variant of a creature (`RabbitType`, the fox's `Type`): saved with the keys of the jar, an unknown value read as 0, and the killer
//! bunny (variant 99) armed as `Rabbit.setVariant(EVIL)` does (armour 8, the species' attack plus 5, a default name), the arming
//! undone by another variant. Needs the vanilla data: the tests return early without it.
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::{
    behaviours::variant::{set_variant, Variant},
    cow::{AttackDamage, Herd, SpeciesId, SpeciesTable},
    custom_name,
    equipment::Armor,
    nbt,
    rules::variant::{RABBIT_EVIL, SNOW},
};

fn creature(kind: &str, extra: &[(&str, Tag)]) -> Compound {
    let pos = Tag::List(vec![Tag::Double(0.5f64.to_bits()), Tag::Double(1.0f64.to_bits()), Tag::Double(0.5f64.to_bits())]);
    let mut c = Compound::default();
    c.insert("id".into(), Tag::String(JavaString::from(kind)));
    c.insert("Pos".into(), pos);
    for (k, v) in extra {
        c.insert((*k).into(), v.clone());
    }
    c
}

fn herd() -> Herd {
    let mut h = Herd::with_slab(40, false, false);
    h.tick();
    h
}

fn base_attack(h: &Herd, e: bevy_ecs::entity::Entity) -> f32 {
    let id = usize::from(h.world.get::<SpeciesId>(e).unwrap().0);
    h.world.resource::<SpeciesTable>().0[id].attack_damage
}

#[test]
fn the_killer_bunny_is_armed_by_its_saved_variant_and_disarmed_by_another() {
    if !super::common::have_data() {
        return;
    }
    let mut h = herd();
    let e = nbt::restore(&mut h.world, &creature("minecraft:rabbit", &[("RabbitType", Tag::Int(i32::from(RABBIT_EVIL)))])).expect("a rabbit");
    assert_eq!(h.world.get::<Variant>(e), Some(&Variant(RABBIT_EVIL)));
    assert_eq!(h.world.get::<Armor>(e).map(|a| a.armor), Some(8.0));
    assert_eq!(h.world.get::<AttackDamage>(e).map(|a| a.0), Some(base_attack(&h, e) + 5.0));
    assert!(custom_name::get(&h.world, e).is_some_and(|n| n.contains("entity.minecraft.killer_bunny")));
    let saved = nbt::save(&h.world, e).expect("a saved rabbit");
    assert_eq!(saved.get(&JavaString::from("RabbitType")), Some(&Tag::Int(99)));
    assert!(saved.get(&JavaString::from("Type")).is_none(), "the fox's key is not the rabbit's");
    set_variant(&mut h.world, e, 3);
    assert_eq!(h.world.get::<AttackDamage>(e).map(|a| a.0), Some(base_attack(&h, e)), "the transient +5 goes with the variant");
}

#[test]
fn an_unknown_rabbit_type_reads_as_zero_and_a_named_one_keeps_its_name() {
    if !super::common::have_data() {
        return;
    }
    let mut h = herd();
    let e = nbt::restore(&mut h.world, &creature("minecraft:rabbit", &[("RabbitType", Tag::Int(7))])).expect("a rabbit");
    assert_eq!(h.world.get::<Variant>(e), Some(&Variant(0)));
    let named = nbt::restore(&mut h.world, &creature("minecraft:rabbit", &[("CustomName", Tag::String("\"Bun\"".into())), ("RabbitType", Tag::Int(99))])).expect("a rabbit");
    assert_eq!(custom_name::get(&h.world, named).as_deref(), Some("\"Bun\""));
}

#[test]
fn the_fox_type_is_the_string_of_the_jar() {
    if !super::common::have_data() {
        return;
    }
    let mut h = herd();
    let e = nbt::restore(&mut h.world, &creature("minecraft:fox", &[("Type", Tag::String("snow".into()))])).expect("a fox");
    assert_eq!(h.world.get::<Variant>(e), Some(&Variant(SNOW)));
    let saved = nbt::save(&h.world, e).expect("a saved fox");
    assert_eq!(saved.get(&JavaString::from("Type")), Some(&Tag::String("snow".into())));
    assert!(saved.get(&JavaString::from("RabbitType")).is_none());
}
