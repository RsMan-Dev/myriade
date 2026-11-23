use crate::common::*;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::behaviours::charged_shot::ExplosionPower;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::nbt;
use horizon_ecs_proto::projectile::{Flight, OnHit};
use horizon_ecs_proto::targeting::{Target, Who};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// `Ghast.explosionPower`: `ExplosionPower` is read with `getByteOr(1)` (any number, narrowed to a byte), saved as a byte, and held by the
/// ghast alone: a cow writes none.
#[test]
fn ghast_explosion_power_sizes_the_fireball_it_shoots() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let ghast = h.spawn(mw, "minecraft:ghast", [8.5, 100.0, 8.5], None).expect("a known species");
    assert_eq!(h.world().get::<ExplosionPower>(ghast).map(|p| p.0), Some(1), "a fresh ghast starts at 1");
    let key = JavaString::from("ExplosionPower");
    for (saved, expected) in [(Tag::Byte(6), 6), (Tag::Byte(0), 0), (Tag::Long(300), 44)] {
        let mut c = nbt::save(h.world(), ghast).expect("a saved creature");
        c.insert(key.clone(), saved.clone());
        let back = nbt::restore(h.world_mut(), &c).expect("a declared, saved type");
        assert_eq!(h.world().get::<ExplosionPower>(back).map(|p| p.0), Some(expected), "ExplosionPower={saved:?}");
        assert_eq!(nbt::save(h.world(), back).expect("saved").get(&key), Some(&Tag::Byte(expected as i8)));
    }
    let cow = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(nbt::save(h.world(), cow).expect("saved").get(&key).is_none(), "only the ghast writes it");
}

/// `Ghast.explosionPower` sizes the fireball in flight: a ghast that shoots a player launches a fireball whose explosion has its power.
#[test]
fn a_fireball_in_flight_carries_the_ghasts_power() {
    have_data();
    let tags = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"));
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags);
    let ghast = h.spawn(mw, "minecraft:ghast", [8.5, 100.0, 8.5], None).expect("a known species");
    let cow = h.spawn(mw, "minecraft:cow", [12.5, 100.0, 8.5], None).expect("a known species");
    h.world_mut().entity_mut(ghast).insert((ExplosionPower(6), Target::new(Who::Entity(cow), u16::MAX, u8::MAX)));
    let mut powers = Vec::new();
    for _ in 0..400 {
        h.step(mw);
        let w = h.world_mut();
        powers.extend(w.query::<&Flight>().iter(w).filter_map(|f| match f.hit {
            OnHit::Explode { power, .. } => Some(power),
            _ => None,
        }));
        if !powers.is_empty() {
            break;
        }
    }
    assert!(powers.contains(&6), "a fireball of power 6 is in flight: {powers:?}");
}
