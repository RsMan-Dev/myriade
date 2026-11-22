//! The potion cloud against its own rules: it shrinks by `radius_per_tick` and is removed under half a block, and its save keeps the jar's
//! keys and loads back. Written, not run.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::cloud_tick::{self, Live};
use horizon_ecs_proto::cow::{skip_unless_default_species, DamageQueue, Health, Herd};
use horizon_ecs_proto::effect_cloud::CloudSpec;
use horizon_ecs_proto::nbt::{load, save};

fn at() -> Vec3 {
    Vec3::new(0.5, 64.0, 0.5)
}

/// A live cloud shrinks by its change a tick, then is removed on the tick it gets under half a block.
#[test]
fn a_cloud_shrinks_then_ends_under_half_a_block() {
    let mut herd = Herd::new(0, 4.0, false, false, 7);
    let spec = CloudSpec { radius_per_tick: -0.5, ..CloudSpec::new(1.2, "minecraft:poison", None) };
    let e = cloud_tick::spawn(&mut herd.world, at(), spec);
    herd.world.entity_mut(e).insert(Live);
    cloud_tick::tick(&mut herd.world);
    assert!((herd.world.get::<CloudSpec>(e).expect("alive").radius - 0.7).abs() < 1e-6);
    cloud_tick::tick(&mut herd.world);
    assert!(herd.world.get_entity(e).is_err());
}

/// The save writes the jar's keys with the jar's values, and a loaded cloud gets the same settings back.
#[test]
fn a_cloud_saves_to_the_jar_keys_and_loads_back() {
    let mut herd = Herd::new(0, 4.0, false, false, 7);
    let spec = CloudSpec { radius_on_use: -0.5, duration_on_use: 4, ..CloudSpec::new(3.0, "minecraft:slowness", None) };
    let e = cloud_tick::spawn(&mut herd.world, at(), spec);
    let saved = save(&herd.world, e).expect("a cloud is saved");
    assert!(matches!(saved.get(&JavaString::from("WaitTime")), Some(Tag::Int(20))));
    assert!(matches!(saved.get(&JavaString::from("Duration")), Some(Tag::Int(-1))));
    assert!(matches!(saved.get(&JavaString::from("DurationOnUse")), Some(Tag::Int(4))));

    let mut other = Herd::new(0, 4.0, false, false, 7);
    let back = cloud_tick::spawn(&mut other.world, at(), CloudSpec::new(0.0, "minecraft:harming", None));
    assert!(load(&mut other.world, back, &saved).is_some());
    let got = other.world.get::<CloudSpec>(back).expect("loaded");
    assert_eq!(got.potion, "minecraft:slowness");
    assert!((got.radius - 3.0).abs() < 1e-6);
    assert!((got.radius_on_use + 0.5).abs() < 1e-6);
    assert_eq!(got.duration_on_use, 4);
}

/// The hits a creature took since the last call, as `DamageQueue` holds them (then cleared).
fn hurt(h: &mut Herd, e: Entity) -> usize {
    std::mem::take(&mut h.world.resource_mut::<DamageQueue>().0).iter().filter(|x| x.target == e).count()
}

/// A harming cloud doses a creature in its circle, then again one reapplication delay (20 ticks) after each dose.
#[test]
fn a_creature_is_dosed_again_one_reapplication_delay_later() {
    if skip_unless_default_species("a_creature_is_dosed_again_one_reapplication_delay_later") {
        return;
    }
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let hit = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    h.tick();
    let spec = CloudSpec { wait: 0, duration: -1, ..CloudSpec::new(2.0, "minecraft:harming", None) };
    cloud_tick::spawn(&mut h.world, Vec3::new(0.5, 1.0, 0.5), spec);
    let mut doses = Vec::new();
    for tick in 1..=30 {
        h.tick();
        if hurt(&mut h, hit) > 0 {
            doses.push(tick);
        }
    }
    assert_eq!(doses.len(), 2, "two doses in 30 ticks, got {doses:?}");
    assert_eq!(doses[1] - doses[0], 20, "the reapplication delay between two doses");
}

/// A custom `Heal(0)` effect on a creature in the cloud heals it 4 whole at full strength and 2 at a cloud's half strength.
#[test]
fn a_custom_healing_effect_heals_a_creature_at_half_strength() {
    if skip_unless_default_species("a_custom_healing_effect_heals_a_creature_at_half_strength") {
        return;
    }
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let hit = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    h.tick();
    h.world.get_mut::<Health>(hit).expect("a creature has health").0 = 1.0;
    let spec = CloudSpec { wait: 0, duration: -1, custom: vec![horizon_ecs_proto::effects::Brew::Heal(0)], ..CloudSpec::new(2.0, "minecraft:water", None) };
    cloud_tick::spawn(&mut h.world, Vec3::new(0.5, 1.0, 0.5), spec);
    for _ in 0..6 {
        h.tick();
    }
    assert_eq!(h.world.get::<Health>(hit).map(|x| x.0), Some(3.0), "1 healed by 4 << 0 at half strength (2)");
}
