use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use myriade_engine_lib::mobs::villager;
use horizon_ecs_proto::{
    behaviours::trade_offers::Merchant,
    brain::{sensors::secondary_job_site::{self, SECONDARY_JOB_SITE}, *},
    cow::*,
};

const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}

fn spawn_named(h: &mut Herd, name: &str, x: f64, z: f64, seed: u64) -> Entity {
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    e
}

fn brain_with(h: &mut Herd, e: Entity, sensors: &'static [SensorEntry]) {
    let def = BrainDef { sensors, activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }], pick: &[], initial: &[], behaviours: &[] };
    let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
    h.world.entity_mut(e).insert(bundle(plan));
    start(&mut h.world, e);
}

fn scan(h: &mut Herd, e: Entity, s: &'static Sensor) {
    (s.scan)(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
}

/// `scan_bounds_are_inclusive`: the farmland at x 5, at y -3 and at the corner (-4, -2, 4) of the villager's block; the scan keeps
/// the corner only. Coordinates are those of the source test (64 at the villager's feet) shifted by its block height.
#[test]
fn scan_bounds_are_inclusive() {
    if skip() {
        return;
    }
    static SENSORS: [SensorEntry; 1] = [SensorEntry::new(&secondary_job_site::SENSOR, 40)];
    let farmer = villager::profession_index("farmer").unwrap();
    let mut h = Herd::with_slab_awake(40, true, false);
    let farm = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 43);
    let (items, enchantments, trades) = crate::common::registries();
    h.world.insert_resource(horizon_ecs_proto::behaviours::trade_offers::TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    h.set_block(5, 0, 0, "minecraft:farmland");
    h.set_block(0, -3, 0, "minecraft:farmland");
    h.set_block(-4, -2, 4, "minecraft:farmland");
    h.tick();
    h.world.entity_mut(farm).insert(Merchant::new(false, farmer, 0));
    brain_with(&mut h, farm, &SENSORS);
    scan(&mut h, farm, &secondary_job_site::SENSOR);
    let sites = match get(&h.world, farm, &SECONDARY_JOB_SITE) {
        Some(MemValue::Blocks(v)) => v,
        _ => Vec::new(),
    };
    assert_eq!(sites, vec![[-4, -2, 4]]);
}

/// `profession_without_entries_skips_the_scan`: a fisherman has no secondary block in its entry, so the farmland around it is no site
/// and the sensor leaves the memory empty (the scan is not run at all: `secondary_job_site.rs`).
#[test]
fn a_profession_without_secondary_blocks_has_no_site() {
    if skip() {
        return;
    }
    static SENSORS: [SensorEntry; 1] = [SensorEntry::new(&secondary_job_site::SENSOR, 40)];
    let fisherman = villager::profession_index("fisherman").unwrap();
    let mut h = Herd::with_slab_awake(40, true, false);
    let v = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 43);
    let (items, enchantments, trades) = crate::common::registries();
    h.world.insert_resource(horizon_ecs_proto::behaviours::trade_offers::TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    h.set_block(1, 0, 0, "minecraft:farmland");
    h.tick();
    h.world.entity_mut(v).insert(Merchant::new(false, fisherman, 0));
    brain_with(&mut h, v, &SENSORS);
    scan(&mut h, v, &secondary_job_site::SENSOR);
    assert!(get(&h.world, v, &SECONDARY_JOB_SITE).is_none(), "a fisherman sees no secondary site");
}
