//! `DistanceMaps` (`behaviours/hostiles_near.rs`) installed from the pack's `neoforge:acceptable_villager_distances`: the same values the
//! Mob reads with `NeoForgeMobMaps::villager_distance`, and no entry for a species the pack does not name.
use bevy_ecs::prelude::*;
use horizon_client_lib::neoforge_mob_maps::NeoForgeMobMaps;
use horizon_ecs_proto::behaviours::hostiles_near::DistanceMaps;
use horizon_ecs_proto::goals::insert_mob_maps;
use std::collections::BTreeMap;
use std::sync::Arc;

const TABLE: &str = "neoforge:acceptable_villager_distances";

#[test]
fn the_distance_map_holds_the_pack_values_as_the_mob_reads_them() {
    let mut maps = NeoForgeMobMaps::default();
    maps.set_distances(BTreeMap::from([("minecraft:zombie".to_string(), 21.0_f32)]));
    let mut w = World::new();
    insert_mob_maps(&mut w, Arc::new(maps));
    let d = w.resource::<DistanceMaps>();
    assert_eq!(d.get(TABLE, "minecraft:zombie"), Some(21.0));
    assert_eq!(d.get(TABLE, "minecraft:creeper"), None);
    assert_eq!(d.max(TABLE), 21.0);
}
