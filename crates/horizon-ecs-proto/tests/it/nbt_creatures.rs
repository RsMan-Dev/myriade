//! The creatures' NBT fields (`nbt_fields::creatures`): a slime's `Size` is saved as vanilla writes it (size minus one) and restored, a refused
//! type changes nothing, and the key of one entity does not touch another.
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::collections::HashMap;
use horizon_ecs_proto::{
    behaviours::{attached_face::Attached, bee::Bee, do_nothing::Invulnerable, forget_target::LlamaStrength, trade_offers::Merchant},
    cow::{Clock, GrowAt},
    explosion::{Charged, Fuse},
    nbt::{self, Saved},
    nbt_fields::creatures,
    size::Size,
};

fn fuse(max: i32, radius: f32) -> Fuse {
    Fuse { value: 0, at: 0, dir: 1, max, radius, armed: 0, ignited: false }
}

#[test]
fn a_creeper_keeps_its_fuse_radius_and_charge_in_vanilla_types() {
    let mut w = world();
    let a = w.spawn((Saved, fuse(40, 5.0), Charged)).id();
    let c = nbt::save(&w, a).unwrap();
    assert_eq!(c.get(&JavaString::from("Fuse")), Some(&Tag::Short(40)));
    assert_eq!(c.get(&JavaString::from("ExplosionRadius")), Some(&Tag::Byte(5)));
    assert_eq!(c.get(&JavaString::from("powered")), Some(&Tag::Byte(1)));
    let mut w2 = world();
    let b = w2.spawn((Saved, fuse(30, 3.0), Charged)).id();
    assert_eq!(nbt::load(&mut w2, b, &c), Some(0));
    assert_eq!(w2.get::<Fuse>(b).map(|f| (f.max, f.radius)), Some((40, 5.0)));
    assert!(w2.get::<Charged>(b).is_some());
    let mut off = Compound::new();
    off.insert("powered".into(), Tag::Byte(0));
    assert_eq!(nbt::load(&mut w2, b, &off), Some(0));
    assert!(w2.get::<Charged>(b).is_none());
}

#[test]
fn a_wither_invul_and_a_llama_strength_round_trip() {
    let mut w = world();
    let wither = w.spawn((Saved, Invulnerable(120))).id();
    let llama = w.spawn((Saved, LlamaStrength(3))).id();
    let wc = nbt::save(&w, wither).unwrap();
    let lc = nbt::save(&w, llama).unwrap();
    assert_eq!(wc.get(&JavaString::from("Invul")), Some(&Tag::Int(120)));
    assert_eq!(lc.get(&JavaString::from("Strength")), Some(&Tag::Int(3)));
    let mut w2 = world();
    let w_new = w2.spawn((Saved, Invulnerable(0))).id();
    let l_new = w2.spawn((Saved, LlamaStrength(0))).id();
    assert_eq!(nbt::load(&mut w2, w_new, &wc), Some(0));
    assert_eq!(nbt::load(&mut w2, l_new, &lc), Some(0));
    assert_eq!(w2.get::<Invulnerable>(w_new).map(|i| i.0), Some(120));
    assert_eq!(w2.get::<LlamaStrength>(l_new).map(|s| s.0), Some(3));
    let mut bad = Compound::new();
    bad.insert("Strength".into(), Tag::Long(3));
    assert_eq!(nbt::load(&mut w2, l_new, &bad), Some(1));
    assert_eq!(w2.get::<LlamaStrength>(l_new).map(|s| s.0), Some(3));
}

fn world() -> World {
    let mut w = World::new();
    nbt::register(&mut w, creatures::FIELDS);
    w
}

#[test]
fn a_slime_size_is_saved_as_size_minus_one_and_restored() {
    let mut w = world();
    let a = w.spawn((Saved, Size(3))).id();
    let c = nbt::save(&w, a).unwrap();
    assert_eq!(c.get(&JavaString::from("Size")), Some(&Tag::Int(2)));
    let mut w2 = world();
    let b = w2.spawn((Saved, Size(1))).id();
    assert_eq!(nbt::load(&mut w2, b, &c), Some(0));
    assert_eq!(w2.get::<Size>(b).map(|s| s.0), Some(3));
}

#[test]
fn a_size_of_another_type_is_refused_and_changes_nothing() {
    let mut w = world();
    let e = w.spawn((Saved, Size(2))).id();
    let mut c = Compound::new();
    c.insert("Size".into(), Tag::Long(5));
    assert_eq!(nbt::load(&mut w, e, &c), Some(1));
    assert_eq!(w.get::<Size>(e).map(|s| s.0), Some(2));
}

#[test]
fn a_size_on_one_entity_does_not_touch_the_key_of_another() {
    let mut w = world();
    let slime = w.spawn((Saved, Size(4))).id();
    let other = w.spawn(Saved).id();
    assert!(nbt::save(&w, other).unwrap().get(&JavaString::from("Size")).is_none());
    let mut c = Compound::new();
    c.insert("Size".into(), Tag::Int(9));
    assert_eq!(nbt::load(&mut w, other, &c), Some(0));
    assert_eq!(nbt::get(&w, other, "Size"), Some(Tag::Int(9)));
    assert_eq!(w.get::<Size>(slime).map(|s| s.0), Some(4));
}

fn timed_world(now: u64) -> World {
    let mut w = world();
    w.insert_resource(Clock::default());
    w.resource_mut::<Clock>().0 = now;
    w
}

#[test]
fn a_baby_age_is_the_ticks_left_and_grows_at_the_absolute_tick() {
    let mut w = timed_world(1000);
    let baby = w.spawn((Saved, GrowAt(1100))).id();
    assert_eq!(nbt::save(&w, baby).unwrap().get(&JavaString::from("Age")), Some(&Tag::Int(-100)));
    let mut w2 = timed_world(2000);
    let b = w2.spawn((Saved, GrowAt(0))).id();
    assert_eq!(nbt::load(&mut w2, b, &nbt::save(&w, baby).unwrap()), Some(0));
    assert_eq!(w2.get::<GrowAt>(b).map(|g| g.0), Some(2100));
    let mut grown = Compound::new();
    grown.insert("Age".into(), Tag::Int(0));
    assert_eq!(nbt::load(&mut w2, b, &grown), Some(0));
    assert!(w2.get::<GrowAt>(b).is_none());
}

#[test]
fn a_shulker_lid_face_and_colour_are_bytes() {
    let mut w = world();
    let s = w.spawn((Saved, Attached { face: 2, color: 5, raw: 0, eased: 0.0 })).id();
    let c = nbt::save(&w, s).unwrap();
    assert_eq!(c.get(&JavaString::from("AttachFace")), Some(&Tag::Byte(2)));
    assert_eq!(c.get(&JavaString::from("Color")), Some(&Tag::Byte(5)));
    let mut w2 = world();
    let t = w2.spawn((Saved, Attached { face: 0, color: 0, raw: 0, eased: 0.0 })).id();
    assert_eq!(nbt::load(&mut w2, t, &c), Some(0));
    assert_eq!(w2.get::<Attached>(t).map(|a| (a.face, a.color)), Some((2, 5)));
    let mut bad = Compound::new();
    bad.insert("Color".into(), Tag::Int(5));
    assert_eq!(nbt::load(&mut w2, t, &bad), Some(1));
}

fn bee(stay_out_until: u64, hive: Option<[i32; 3]>) -> Bee {
    Bee {
        nectar: true, stung: false, hive, flower: None, stay_out_until, hive_cd_until: 0, flower_cd_until: 0, no_nectar_since: 0,
        crops: 0, blacklist: Vec::new(), unreachable: HashMap::new(), pollinating: false,
    }
}

#[test]
fn a_bee_keeps_its_flags_hive_and_stay_out_time() {
    let mut w = timed_world(1000);
    let a = w.spawn((Saved, bee(1300, Some([4, 64, -2])))).id();
    let c = nbt::save(&w, a).unwrap();
    assert_eq!(c.get(&JavaString::from("HasNectar")), Some(&Tag::Byte(1)));
    assert_eq!(c.get(&JavaString::from("HasStung")), Some(&Tag::Byte(0)));
    assert_eq!(c.get(&JavaString::from("CannotEnterHiveTicks")), Some(&Tag::Int(300)));
    assert_eq!(c.get(&JavaString::from("hive_pos")), Some(&Tag::IntArray(vec![4, 64, -2])));
    let mut w2 = timed_world(5000);
    let b = w2.spawn((Saved, bee(0, None))).id();
    assert_eq!(nbt::load(&mut w2, b, &c), Some(0));
    let got = w2.get::<Bee>(b).unwrap();
    assert_eq!((got.nectar, got.stung, got.stay_out_until, got.hive), (true, false, 5300, Some([4, 64, -2])));
    let mut bad = Compound::new();
    bad.insert("hive_pos".into(), Tag::IntArray(vec![1, 2]));
    assert_eq!(nbt::load(&mut w2, b, &bad), Some(1));
}

#[test]
fn a_baby_is_written_as_is_baby_and_a_villager_data_of_another_type_is_refused() {
    let mut w = timed_world(0);
    let baby = w.spawn((Saved, GrowAt(50))).id();
    assert_eq!(nbt::save(&w, baby).unwrap().get(&JavaString::from("IsBaby")), Some(&Tag::Byte(1)));
    let v = w.spawn((Saved, Merchant::new(false, 1, 2))).id();
    let mut c = Compound::new();
    c.insert("VillagerData".into(), Tag::Int(3));
    assert_eq!(nbt::load(&mut w, v, &c), Some(1));
}
