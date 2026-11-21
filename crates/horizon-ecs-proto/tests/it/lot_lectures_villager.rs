//! Lectures des capteurs du villageois contre le `Mob` (`HORIZON_TEST_RESOURCES` set) : bébés visibles, détecteur de golem, poste
//! secondaire du fermier, vie des voisins. Écrit sans compiler : à vérifier au premier passage. Chaque écart corrigé a son test : les
//! capteurs sont scannés à la suite, au même tic, sur la même cible.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::items::ItemStack;
use horizon_ecs_proto::item::drop_item;
use horizon_client_lib::mobs::villager;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{bed::NEAREST_BED, golem_recent::GOLEM_RECENT, nearby_babies::NEARBY_BABIES, nearest_bed::{self, BedCache}, summon_defender::{self, LastSlept}, trade_offers::Merchant, Cap},
    brain::{
        sensors::{living, nearest_wanted_item::{self, NEAREST_WANTED_ITEM}, recent_sighting, secondary_job_site::{self, SECONDARY_JOB_SITE}, villager_babies},
        *,
    },
    cow::*,
    effects::Invisible,
    targeting::Who,
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

/// A brain holding only `sensors`, started on `e` (the sensors are scanned by the test itself).
fn brain_with(h: &mut Herd, e: Entity, sensors: &'static [SensorEntry]) {
    let def = BrainDef { sensors, activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }], pick: &[], initial: &[], behaviours: &[] };
    let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
    h.world.entity_mut(e).insert(bundle(plan));
    start(&mut h.world, e);
}

fn scan(h: &mut Herd, e: Entity, s: &'static Sensor) {
    (s.scan)(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
}

/// `Sensor.isEntityTargetable` cuts the babies out of sight: the Mob's `visible_villager_babies` lists the open baby only.
#[test]
fn a_baby_behind_a_wall_is_not_a_visible_baby() {
    if skip() {
        return;
    }
    static SENSORS: [SensorEntry; 2] = [SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&villager_babies::SENSOR, 20)];
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 11);
    let open = spawn_named(&mut h, "minecraft:villager", 4.5, 0.5, 12);
    let walled = spawn_named(&mut h, "minecraft:villager", -4.5, 0.5, 13);
    for b in [open, walled] {
        let now = now_of(&h.world, b);
        h.world.entity_mut(b).insert(GrowAt(now + 24000));
    }
    for z in -1..=1 {
        for y in 1..=3 {
            h.set_block(-2, y, z, "minecraft:stone");
        }
    }
    h.tick();
    brain_with(&mut h, e, &SENSORS);
    scan(&mut h, e, &living::SENSOR);
    scan(&mut h, e, &villager_babies::SENSOR);
    let babies = match get(&h.world, e, &NEARBY_BABIES) {
        Some(MemValue::Whos(l)) => l.clone(),
        _ => Vec::new(),
    };
    assert_eq!(babies, vec![Who::Entity(open)], "one visible baby: the walled one is out of sight");
}

static SEEN: recent_sighting::Params = recent_sighting::Params { seen: iron_golem_seen };

fn iron_golem_seen(w: &bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> bool {
    w.get::<TypeName>(e).is_some_and(|t| t.0 == "minecraft:iron_golem")
}

/// `GolemSensor`: an iron golem among the living sets `golem_detected_recently` (599 ticks); with none near, nothing is set.
#[test]
fn an_iron_golem_nearby_sets_golem_detected_recently_and_none_does_not() {
    if skip() {
        return;
    }
    static SENSORS: [SensorEntry; 2] = [SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&recent_sighting::SENSOR, 200)];
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 21);
    let lonely = spawn_named(&mut h, "minecraft:villager", 20.5, 0.5, 22);
    spawn_named(&mut h, "minecraft:iron_golem", 3.5, 0.5, 23);
    h.tick();
    brain_with(&mut h, e, &SENSORS);
    brain_with(&mut h, lonely, &SENSORS);
    scan(&mut h, e, &living::SENSOR);
    scan_with(&mut h, e, &recent_sighting::SENSOR, &SEEN);
    scan(&mut h, lonely, &living::SENSOR);
    scan_with(&mut h, lonely, &recent_sighting::SENSOR, &SEEN);
    assert!(get(&h.world, e, &GOLEM_RECENT).is_some(), "the golem at 3 blocks is in MOBS: the memory is set");
    assert!(get(&h.world, lonely, &GOLEM_RECENT).is_none(), "no golem within the follow range: the memory is not set");
}

/// `NearestLivingEntitySensor`: a dead neighbour (health 0) is not in `mobs`; the living one is, as the Mob's `alive` test.
#[test]
fn a_dead_neighbour_is_not_a_living_entity() {
    if skip() {
        return;
    }
    static SENSORS: [SensorEntry; 1] = [SensorEntry::new(&living::SENSOR, 20)];
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 31);
    let other = spawn_named(&mut h, "minecraft:villager", 3.5, 0.5, 32);
    h.tick();
    brain_with(&mut h, e, &SENSORS);
    scan(&mut h, e, &living::SENSOR);
    assert_eq!(format!("{:?}", get(&h.world, e, &MOBS)), format!("{:?}", Some(&MemValue::Whos(vec![Who::Entity(other)]))), "the living neighbour is listed");
    h.world.get_mut::<Health>(other).unwrap().0 = 0.0;
    scan(&mut h, e, &living::SENSOR);
    assert!(get(&h.world, e, &MOBS).is_none(), "the dead neighbour leaves `mobs`: the list is empty and erased");
}

/// `SecondarySite` (`sense_secondary`): farmland is a secondary site of a farmer (2 cells here), never of another profession.
#[test]
fn farmland_is_a_secondary_site_of_the_farmer_only() {
    if skip() {
        return;
    }
    static SENSORS: [SensorEntry; 1] = [SensorEntry::new(&secondary_job_site::SENSOR, 40)];
    let farmer = villager::profession_index("farmer").unwrap();
    let mut h = Herd::with_slab_awake(40, true, false);
    let none = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 41);
    let farm = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 42);
    h.world.entity_mut(none).insert(Merchant::new(false, 0, 0));
    h.world.entity_mut(farm).insert(Merchant::new(false, farmer, 0));
    h.set_block(1, 0, 0, "minecraft:farmland");
    h.set_block(-1, 0, 0, "minecraft:farmland");
    h.tick();
    brain_with(&mut h, none, &SENSORS);
    brain_with(&mut h, farm, &SENSORS);
    scan(&mut h, none, &secondary_job_site::SENSOR);
    scan(&mut h, farm, &secondary_job_site::SENSOR);
    assert!(get(&h.world, none, &SECONDARY_JOB_SITE).is_none(), "a non-farmer has no secondary site");
    let cells = match get(&h.world, farm, &SECONDARY_JOB_SITE) {
        Some(MemValue::Blocks(v)) => v.len(),
        _ => 0,
    };
    assert_eq!(cells, 2, "the farmer remembers the 2 farmland cells within reach");
}

/// `sense_bed` (`cache.retain` when the path does not reach a bed and fewer than 5 are tried): an unreachable bed tried by a first scan
/// leaves the cache at a later scan that tries another bed; the Mob drops it, the ECS kept it while there were targets.
#[test]
fn an_unreachable_bed_leaves_the_cache_when_another_bed_is_tried() {
    if skip() {
        return;
    }
    static PARAMS: nearest_bed::Params = nearest_bed::Params { nearest: &NEAREST_BED, candidate: &nearest_bed::BED_CANDIDATE, follow_range: 48.0 };
    let (a, b) = ([6, 1, 0], [-6, 1, 0]);
    let mut h = Herd::with_slab_awake(40, true, false);
    let baby = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 51);
    let now = now_of(&h.world, baby);
    h.world.entity_mut(baby).insert(GrowAt(now + 24000));
    enclose_bed(&mut h, a[0]);
    h.tick();
    scan_with(&mut h, baby, &nearest_bed::SENSOR, &PARAMS);
    enclose_bed(&mut h, b[0]);
    for _ in 0..100 {
        h.tick();
    }
    scan_with(&mut h, baby, &nearest_bed::SENSOR, &PARAMS);
    let cache = h.world.get::<BedCache>(baby).unwrap().0.clone();
    assert!(cache.iter().any(|(p, _)| *p == b), "the new bed is tried and cached");
    assert!(cache.iter().all(|(p, _)| *p != a), "the unreachable bed tried 100 ticks ago is dropped: the Mob's retain keeps only fresh entries");
}

/// A bed walled in on every side, so no path reaches it.
fn enclose_bed(h: &mut Herd, x: i32) {
    for dy in -1..=1 {
        for dz in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy, dz) != (0, 0, 0) {
                    h.set_block(x + dx, 1 + dy, dz, "minecraft:stone");
                }
            }
        }
    }
    h.set_block_state(x, 1, 0, "minecraft:red_bed", &[("part", "head")]);
}

fn scan_with(h: &mut Herd, e: Entity, s: &'static Sensor, params: &'static (dyn std::any::Any + Sync)) {
    (s.scan)(&mut h.world, e, &Ctx { id: 0, nth: 0, params: params as &'static (dyn std::any::Any + Sync) });
}

/// `Villager.wantsToSpawnGolem`: a villager that slept wants a golem until one is detected recently (599 ticks), as the Mob's `wants_golem`.
#[test]
fn a_villager_who_slept_wants_a_golem_until_one_is_detected() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 61);
    let now = now_of(&h.world, e);
    h.world.entity_mut(e).insert(LastSlept(now));
    assert!(summon_defender::wants(&h.world, e, now), "slept, no golem detected: the villager wants one");
    set(&mut h.world, e, &GOLEM_RECENT, MemValue::Unit, Some(599));
    assert!(!summon_defender::wants(&h.world, e, now), "a golem detected recently: the villager wants none");
}

/// `Sensor.isEntityTargetable`: an invisible creature (`getVisibilityPercent`: 0.7 at least 0.1 of its cover, here none) is seen within
/// 2 blocks only, so the invisible baby 4 blocks away is out of the babies, the visible one 3 blocks away is in.
#[test]
fn an_invisible_baby_is_seen_only_within_two_blocks() {
    if skip() {
        return;
    }
    static SENSORS: [SensorEntry; 1] = [SensorEntry::new(&villager_babies::SENSOR, 20)];
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 71);
    let hidden = spawn_named(&mut h, "minecraft:villager", 4.5, 0.5, 72);
    let seen = spawn_named(&mut h, "minecraft:villager", -3.5, 0.5, 73);
    for b in [hidden, seen] {
        let now = now_of(&h.world, b);
        h.world.entity_mut(b).insert(GrowAt(now + 24000));
    }
    h.world.entity_mut(hidden).insert(Invisible);
    h.tick();
    brain_with(&mut h, e, &SENSORS);
    scan(&mut h, e, &villager_babies::SENSOR);
    assert_eq!(format!("{:?}", get(&h.world, e, &NEARBY_BABIES)), format!("{:?}", Some(&MemValue::Whos(vec![Who::Entity(seen)]))), "only the visible baby is seen");
}

/// `NearestItemSensor`: the nearest pickable item of the tag within 4 blocks is remembered; a nearer item outside the tag is not.
#[test]
fn the_nearest_wanted_item_of_the_tag_is_remembered() {
    if skip() {
        return;
    }
    static SENSORS: [SensorEntry; 1] = [SensorEntry::new(&nearest_wanted_item::SENSOR, 20)];
    static PARAMS: nearest_wanted_item::Params = nearest_wanted_item::Params { tag: "minecraft:villager_picks_up", range: 4.0 };
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn_named(&mut h, "minecraft:villager", 0.5, 0.5, 81);
    drop_item(&mut h.world, ItemStack::new("minecraft:stone", 1), Vec3::new(1.5, 1.0, 0.5), Vec3::ZERO, 0);
    let wanted = drop_item(&mut h.world, ItemStack::new("minecraft:bread", 1), Vec3::new(3.5, 1.0, 0.5), Vec3::ZERO, 0);
    h.tick();
    brain_with(&mut h, e, &SENSORS);
    scan_with(&mut h, e, &nearest_wanted_item::SENSOR, &PARAMS);
    assert_eq!(format!("{:?}", get(&h.world, e, &NEAREST_WANTED_ITEM)), format!("{:?}", Some(&MemValue::Who(Who::Entity(wanted)))), "the bread is the wanted item: the stone is not in the tag");
}
