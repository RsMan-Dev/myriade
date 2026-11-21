//! The keys declared for the save conformity (`nbt_fields::creatures`): a hen's `EggLayTime` is saved as the ticks left and restored, an
//! idle timer keeps its ticks left, and a refused type changes nothing.
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::{
    behaviours::{bee::Bee, follow_caravan::{Caravan, Params}, forget_target::LlamaStrength, fox_state::FoxFlags, swallow_hold::SearchAfter, swallow_item::Swallowable},
    cow::Clock,
    farm::{Egg, EggDef, Wool},
    nbt::{self, Saved},
    nbt_fields::creatures,
};
use std::collections::HashMap;

fn world() -> World {
    let mut w = World::new();
    w.insert_resource(Clock::default());
    nbt::register(&mut w, creatures::FIELDS);
    w
}

fn bee(no_nectar_since: u64) -> Bee {
    Bee {
        nectar: true, stung: false, hive: None, flower: None, stay_out_until: 0, hive_cd_until: 0, flower_cd_until: 0, no_nectar_since,
        crops: 0, blacklist: Vec::new(), unreachable: HashMap::new(), pollinating: false,
    }
}

fn egg(left: u64) -> Egg {
    Egg { def: EggDef { base: 6000, range: 6000 }, left, at: u64::MAX }
}

#[test]
fn a_hen_egg_time_round_trips_as_the_ticks_left() {
    let mut w = world();
    let hen = w.spawn((Saved, egg(4321))).id();
    let c = nbt::save(&w, hen).unwrap();
    assert_eq!(c.get(&JavaString::from("EggLayTime")), Some(&Tag::Int(4321)));
    let mut w2 = world();
    let new = w2.spawn((Saved, egg(1))).id();
    assert_eq!(nbt::load(&mut w2, new, &c), Some(0));
    assert_eq!(w2.get::<Egg>(new).map(|g| g.left), Some(4321));
}

#[test]
fn a_refused_egg_time_changes_nothing() {
    let mut w = world();
    let hen = w.spawn((Saved, egg(77))).id();
    let mut bad = Compound::new();
    bad.insert("EggLayTime".into(), Tag::Long(5));
    nbt::load(&mut w, hen, &bad);
    assert_eq!(w.get::<Egg>(hen).map(|g| g.left), Some(77));
}

#[test]
fn a_sheep_colour_and_a_fox_sitting_round_trip() {
    let mut w = world();
    let sheep = w.spawn((Saved, Wool { sheared: false, color: 14 })).id();
    let fox = w.spawn((Saved, FoxFlags(1))).id();
    let sc = nbt::save(&w, sheep).unwrap();
    let fc = nbt::save(&w, fox).unwrap();
    assert_eq!(sc.get(&JavaString::from("Color")), Some(&Tag::Byte(14)));
    assert_eq!(fc.get(&JavaString::from("Sitting")), Some(&Tag::Byte(1)));
    let mut w2 = world();
    let sheep2 = w2.spawn((Saved, Wool::default())).id();
    let fox2 = w2.spawn((Saved, FoxFlags(0))).id();
    assert_eq!(nbt::load(&mut w2, sheep2, &sc), Some(0));
    assert_eq!(nbt::load(&mut w2, fox2, &fc), Some(0));
    assert_eq!(w2.get::<Wool>(sheep2).map(|x| x.color), Some(14));
    assert_eq!(w2.get::<FoxFlags>(fox2).map(|x| x.0 & 1), Some(1));
}

#[test]
fn a_llama_strength_round_trips_as_the_stored_value() {
    let caravan = || Caravan { p: &Params::DEFAULT, slot: 0, pick: None };
    let mut w = world();
    let llama = w.spawn((Saved, caravan(), LlamaStrength(4))).id();
    let c = nbt::save(&w, llama).unwrap();
    assert_eq!(c.get(&JavaString::from("Strength")), Some(&Tag::Int(4)));
    let mut w2 = world();
    let new = w2.spawn((Saved, caravan(), LlamaStrength(1))).id();
    assert_eq!(nbt::load(&mut w2, new, &c), Some(0));
    assert_eq!(w2.get::<LlamaStrength>(new).map(|s| s.0), Some(4));
}

#[test]
fn a_cube_without_timer_saves_a_zero_pickup_timer() {
    let mut w = world();
    let cube = w.spawn((Saved, Swallowable(&[]))).id();
    let c = nbt::save(&w, cube).unwrap();
    assert_eq!(c.get(&JavaString::from("pickup_timer")), Some(&Tag::Int(0)));
}

#[test]
fn a_bee_pollination_and_a_cube_pickup_timer_round_trip_as_ticks() {
    let mut w = world();
    w.resource_mut::<Clock>().0 = 1000;
    let b = w.spawn((Saved, bee(900))).id();
    let s = w.spawn((Saved, SearchAfter(1500), Swallowable(&[]))).id();
    let bc = nbt::save(&w, b).unwrap();
    let sc = nbt::save(&w, s).unwrap();
    assert_eq!(bc.get(&JavaString::from("TicksSincePollination")), Some(&Tag::Int(100)));
    assert_eq!(sc.get(&JavaString::from("pickup_timer")), Some(&Tag::Int(500)));
    let mut w2 = world();
    w2.resource_mut::<Clock>().0 = 2000;
    let b2 = w2.spawn((Saved, bee(0))).id();
    let s2 = w2.spawn((Saved, SearchAfter(0), Swallowable(&[]))).id();
    assert_eq!(nbt::load(&mut w2, b2, &bc), Some(0));
    assert_eq!(nbt::load(&mut w2, s2, &sc), Some(0));
    assert_eq!(w2.get::<Bee>(b2).map(|x| x.no_nectar_since), Some(1900));
    assert_eq!(w2.get::<SearchAfter>(s2).map(|x| x.0), Some(2500));
}
