//! The spawn charges of the biomes: a spawn must keep the energy change of its place within its budget (`SpawnState.canSpawn`). The vanilla
//! values are `NetherBiomes.soulSandValley` (javap): skeleton, ghast, enderman and strider, each `addMobSpawnCost(type, 0.7, 0.15)`, that is
//! a charge of 0.7 and an energy budget of 0.15. The energy change is `PotentialCalculator.getPotentialEnergyChange`: each charge over its
//! distance to the spawn, summed, times the spawn's charge.
use bevy_ecs::prelude::*;
use horizon_client_lib::biome::SpawnCost;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::spawn_biome::biome_at;
use horizon_ecs_proto::spawn_cost::{refresh, ChargeBearer, Charges};
use horizon_ecs_proto::spawning::{Counted, Kind, Spawner};

const TICKS: usize = 800;

fn have() -> bool {
    horizon_ecs_proto::species::inscribe_every_module();
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}

/// A world whose bearers stand at `at`, each with the charge 0.7, read as a reading of the field.
fn charged(at: &[[f64; 3]]) -> World {
    let mut w = World::new();
    w.insert_resource(Charges::default());
    for p in at {
        w.spawn((ChargeBearer { charge: 0.7 }, Pos(Vec3::new(p[0], p[1], p[2]))));
    }
    refresh(&mut w);
    w
}

#[test]
fn the_budget_refuses_the_spawn_once_two_charges_stand_nearby() {
    // One charge 4 blocks away: 0.7 / 4 * 0.7 = 0.1225 <= 0.15, allowed. Two: 0.245 > 0.15, refused.
    let cost = SpawnCost { energy_budget: 0.15, charge: 0.7 };
    let one = charged(&[[4.0, 0.0, 0.0]]);
    assert!(one.resource::<Charges>().can_spawn([0, 0, 0], cost));
    let two = charged(&[[4.0, 0.0, 0.0], [0.0, 4.0, 0.0]]);
    assert!(!two.resource::<Charges>().can_spawn([0, 0, 0], cost));
}

#[test]
fn a_newborn_joins_the_field_without_a_new_reading() {
    let cost = SpawnCost { energy_budget: 0.15, charge: 0.7 };
    let mut w = charged(&[[4.0, 0.0, 0.0]]);
    let before = w.resource::<Charges>().version();
    w.spawn((ChargeBearer { charge: 0.7 }, Pos(Vec3::new(0.0, 4.0, 0.0))));
    w.resource_mut::<Charges>().after_birth([0, 4, 0], 0.7, before);
    assert!(!w.resource::<Charges>().can_spawn([0, 0, 0], cost));
}

#[test]
fn the_soul_sand_valley_charges_its_skeletons_and_the_plains_charge_nothing() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_kind_awake(48, true, Kind::TwoBiomes);
    h.enable_spawning("minecraft:plains", 7);
    let mw = h.world.resource::<Level>().mw.0;
    let spawner = h.world.resource::<Spawner>();
    let skeleton = species_table().0.iter().find(|s| s.name == "minecraft:skeleton").unwrap().id;
    let valley = &spawner.tables[mw.biomes.index["minecraft:soul_sand_valley"] as usize];
    assert_eq!(valley.costs.get(&skeleton), Some(&SpawnCost { energy_budget: 0.15, charge: 0.7 }));
    let plains = &spawner.tables[mw.biomes.index["minecraft:plains"] as usize];
    assert!(plains.costs.is_empty());
    for _ in 0..TICKS {
        h.tick();
    }
    assert!(h.world.resource::<Spawner>().stats.born > 0);
    assert_eq!(h.world.query::<&ChargeBearer>().iter(&h.world).count(), 0);
    assert_eq!(h.world.get_resource::<Charges>().map_or(0, |c| c.version()), 0);
}

#[test]
fn a_skeleton_reloaded_from_its_nbt_carries_its_charge_and_is_counted() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_kind_awake(48, true, Kind::TwoBiomes);
    h.enable_spawning("minecraft:plains", 7);
    let mw = h.world.resource::<Level>().mw.0;
    let zoom = h.world.resource::<Spawner>().zoom;
    let valley = mw.biomes.index["minecraft:soul_sand_valley"];
    let x = (0..512).step_by(4).find(|&x| biome_at(mw, zoom, x, 64, 0) == valley).expect("a soul sand valley column");
    let (tag, _) = horizon_core::nbt::parse_snbt(&format!("{{id:\"minecraft:skeleton\",Pos:[{x}.5d,64.0d,0.5d]}}")).unwrap();
    let horizon_core::nbt::Tag::Compound(c) = tag else { panic!() };
    let e = horizon_ecs_proto::nbt::restore(&mut h.world, &c).unwrap();
    assert_eq!(h.world.get::<ChargeBearer>(e).map(|b| b.charge), Some(0.7));
    assert!(h.world.get::<Counted>(e).is_some());
}
