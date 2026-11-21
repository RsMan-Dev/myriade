//! The NBT keys of the objects and projectiles (`nbt_fields/things.rs`): the type each key takes in the origin, the save and restore
//! round trip, the refusal of a wrong type, and a sort the origin does not save (the bobber) having no NBT. The entities are made by
//! their creation functions: `nbt::Saved` comes from the species declaration, never from the test.
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, JavaString, Tag},
};
use horizon_ecs_proto::{
    cow::{now_of, species_table, spawn_species, Herd},
    end_crystal,
    item::{drop_item, Item},
    nbt::{self, Saved},
    orb::{spawn_orb, Orb},
    projectile::Projectile,
    tnt::spawn_primed,
    trident::{self, Pickup},
};

fn at(c: &Compound, key: &str) -> Option<Tag> {
    c.get(&JavaString::from(key)).cloned()
}

fn saved(w: &World, e: Entity) -> Compound {
    nbt::save(w, e).expect("a saved entity has an NBT")
}

#[test]
fn a_sort_the_origin_does_not_save_has_no_nbt() {
    let mut h = Herd::with_slab(2, false, false);
    let sp = species_table().0.into_iter().find(|sp| sp.name == "minecraft:fishing_bobber").expect("the bobber species");
    assert!(!sp.saved);
    let e = spawn_species(&mut h.world, &sp, 0.0, 0.0, 0);
    assert!(h.world.get::<Saved>(e).is_none());
    assert!(nbt::save(&h.world, e).is_none());
}

#[test]
fn a_dropped_item_saves_its_age_pickup_delay_and_owner_and_restores_them() {
    let mut h = Herd::with_slab(2, false, false);
    for _ in 0..60 {
        h.tick();
    }
    let w = &mut h.world;
    let at_pos = Vec3::new(0.5, 1.0, 0.5);
    let e = drop_item(w, ItemStack::new("minecraft:stone", 3), at_pos, Vec3::ZERO, 10);
    let now = now_of(w, e);
    {
        let mut it = w.get_mut::<Item>(e).unwrap();
        it.age_base = now - 40;
        it.target = Some([1, 2, 3, 4]);
    }
    let c = saved(w, e);
    assert!(matches!(at(&c, "Item"), Some(Tag::Compound(_))));
    assert_eq!(at(&c, "Age"), Some(Tag::Short(40)));
    assert_eq!(at(&c, "PickupDelay"), Some(Tag::Short(10)));
    assert_eq!(at(&c, "Owner"), Some(Tag::IntArray(vec![1, 2, 3, 4])));

    let f = drop_item(w, ItemStack::new("minecraft:dirt", 1), at_pos, Vec3::ZERO, 0);
    assert_eq!(nbt::load(w, f, &c), Some(0));
    assert_eq!(nbt::save(w, f), Some(c.clone()));

    let before = nbt::save(w, f);
    let mut bad = c.clone();
    bad.insert("Age".into(), Tag::Int(40));
    assert_eq!(nbt::load(w, f, &bad), Some(1), "an int Age is refused");
    assert_eq!(nbt::save(w, f), before);
}

#[test]
fn an_experience_orb_saves_value_count_and_age_and_restores_them() {
    let mut h = Herd::with_slab(2, false, false);
    for _ in 0..60 {
        h.tick();
    }
    let w = &mut h.world;
    let e = spawn_orb(w, 7, 3, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO);
    let now = now_of(w, e);
    w.get_mut::<Orb>(e).unwrap().age_base = now - 25;
    let c = saved(w, e);
    assert_eq!(at(&c, "Value"), Some(Tag::Short(7)));
    assert_eq!(at(&c, "Count"), Some(Tag::Int(3)));
    assert_eq!(at(&c, "Age"), Some(Tag::Short(25)));

    let f = spawn_orb(w, 0, 1, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO);
    assert_eq!(nbt::load(w, f, &c), Some(0));
    assert_eq!(nbt::save(w, f), Some(c.clone()));

    let mut bad = c.clone();
    bad.insert("Value".into(), Tag::Int(7));
    assert_eq!(nbt::load(w, f, &bad), Some(1), "an int Value is refused");
}

#[test]
fn a_primed_tnt_saves_its_fuse_left_and_power_and_restores_them() {
    let mut h = Herd::with_slab(2, false, false);
    let w = &mut h.world;
    let e = spawn_primed(w, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 60, 4.0);
    let c = saved(w, e);
    assert_eq!(at(&c, "fuse"), Some(Tag::Short(60)));
    assert_eq!(at(&c, "explosion_power"), Some(Tag::Float(4.0_f32.to_bits())));

    let f = spawn_primed(w, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0, 0.0);
    assert_eq!(nbt::load(w, f, &c), Some(0));
    assert_eq!(nbt::save(w, f), Some(c.clone()));

    let mut bad = c.clone();
    bad.insert("fuse".into(), Tag::Int(60));
    assert_eq!(nbt::load(w, f, &bad), Some(1), "an int fuse is refused");
}

#[test]
fn an_end_crystal_saves_show_bottom_as_a_byte_and_restores_it() {
    let mut h = Herd::with_slab(2, false, false);
    let w = &mut h.world;
    let e = end_crystal::spawn(w, Vec3::new(0.5, 1.0, 0.5), true);
    let c = saved(w, e);
    assert_eq!(at(&c, "ShowBottom"), Some(Tag::Byte(1)));

    let f = end_crystal::spawn(w, Vec3::new(0.5, 1.0, 0.5), false);
    assert_eq!(nbt::load(w, f, &c), Some(0));
    assert_eq!(nbt::save(w, f), Some(c.clone()));

    let mut bad = c.clone();
    bad.insert("ShowBottom".into(), Tag::Int(1));
    assert_eq!(nbt::load(w, f, &bad), Some(1), "an int ShowBottom is refused");
}

#[test]
fn a_trident_saves_its_dealt_damage_pickup_and_stack_and_restores_them() {
    let mut h = Herd::with_slab(2, false, false);
    let w = &mut h.world;
    let e = trident::throw(w, ItemStack::new("minecraft:trident", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, None, 0, Pickup::CreativeOnly);
    w.get_mut::<trident::Trident>(e).unwrap().dealt = true;
    let c = saved(w, e);
    assert_eq!(at(&c, "DealtDamage"), Some(Tag::Byte(1)));
    assert_eq!(at(&c, "pickup"), Some(Tag::Byte(2)));
    assert!(matches!(at(&c, "item"), Some(Tag::Compound(_))));

    let f = trident::throw(w, ItemStack::new("minecraft:stick", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, None, 0, Pickup::Disallowed);
    assert_eq!(nbt::load(w, f, &c), Some(0));
    assert_eq!(nbt::save(w, f), Some(c.clone()));

    let mut bad = c.clone();
    bad.insert("pickup".into(), Tag::Byte(3));
    assert_eq!(nbt::load(w, f, &bad), Some(1), "a pickup outside 0..=2 is refused");
}

#[test]
fn a_projectile_saves_left_owner_only_when_set_and_restores_it() {
    let mut h = Herd::with_slab(2, false, false);
    let w = &mut h.world;
    let e = trident::throw(w, ItemStack::new("minecraft:trident", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, None, 0, Pickup::Allowed);
    w.get_mut::<Projectile>(e).unwrap().left_owner = true;
    let c = saved(w, e);
    assert_eq!(at(&c, "LeftOwner"), Some(Tag::Byte(1)));

    let f = trident::throw(w, ItemStack::new("minecraft:trident", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, None, 0, Pickup::Allowed);
    assert_eq!(nbt::load(w, f, &c), Some(0));
    assert_eq!(nbt::save(w, f), Some(c.clone()));

    let g = trident::throw(w, ItemStack::new("minecraft:trident", 1), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, None, 0, Pickup::Allowed);
    assert!(at(&saved(w, g), "LeftOwner").is_none(), "an unset LeftOwner is not written");
}

#[test]
fn a_large_fireball_keeps_its_explosion_power_as_a_byte_through_a_save_and_a_restore() {
    use horizon_ecs_proto::projectile::{launch, Flight, OnHit};
    let mut h = Herd::with_slab(2, false, false);
    let e = launch(&mut h.world, Flight::large_fireball(4), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, None);
    let c = saved(&h.world, e);
    assert_eq!(at(&c, "ExplosionPower"), Some(Tag::Byte(4)), "`LargeFireball.addAdditionalSaveData` puts a byte");

    let mut b = Herd::with_slab(2, false, false);
    let r = nbt::restore(&mut b.world, &c).expect("a declared, saved type");
    let flight = *b.world.get::<Flight>(r).expect("a flight");
    assert!(matches!(flight.hit, OnHit::Explode { power: 4, fire: true }), "the power comes back, not 1: {:?}", flight.hit);
    assert_eq!(nbt::save(&b.world, r).as_ref(), Some(&c));

    let mut absent = c.clone();
    absent.remove(&JavaString::from("ExplosionPower"));
    let d = nbt::restore(&mut b.world, &absent).expect("restored");
    assert!(matches!(b.world.get::<Flight>(d).unwrap().hit, OnHit::Explode { power: 1, .. }), "`getByteOr(1)` when absent");
    let wither = launch(&mut h.world, Flight::wither_skull(false), Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, None);
    assert!(at(&saved(&h.world, wither), "ExplosionPower").is_none(), "only the large fireball writes it");
}

#[test]
fn a_wither_skull_keeps_dangerous_and_a_hurting_projectile_its_acceleration() {
    use horizon_ecs_proto::projectile::{launch, Flight};
    let mut h = Herd::with_slab(2, false, false);
    let at_pos = Vec3::new(0.5, 1.0, 0.5);
    let blue = launch(&mut h.world, Flight::wither_skull(true), at_pos, Vec3::ZERO, None);
    let c = saved(&h.world, blue);
    assert_eq!(at(&c, "dangerous"), Some(Tag::Byte(1)));
    assert_eq!(at(&c, "acceleration_power"), Some(Tag::Double(0.1f64.to_bits())));
    let mut b = Herd::with_slab(2, false, false);
    let r = nbt::restore(&mut b.world, &c).expect("restored");
    let f = *b.world.get::<Flight>(r).unwrap();
    assert!(f.blue, "a blue skull stays blue");
    assert_eq!(f.inertia, Flight::wither_skull(true).inertia, "and keeps its 0.73");
    assert_eq!(nbt::save(&b.world, r).as_ref(), Some(&c));

    let arrow = launch(&mut h.world, Flight::ARROW, at_pos, Vec3::ZERO, None);
    let c = saved(&h.world, arrow);
    assert!(at(&c, "dangerous").is_none() && at(&c, "acceleration_power").is_none(), "an arrow has neither");
}
