//! The villager's points, lot V1, ECS against `Mob` where the outcome is visible (profession, position): validation, competitors,
//! assignment, reset, the walk to a potential site and its acquisition. Yield and the home's occupancy are not visible from the
//! harness (no memory access on the `Mob`) and are not tested here.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{poi_claim, poi_claim::ClaimIndex, profession::Profession},
    brain::behaviours::{poi_acquire, poi_assign, poi_competitor_scan, poi_go_to_site, poi_validate, poi_yield_site},
    brain::*,
    cow::*,
    goals::*,
    poi::PoiRegistry,
};
use horizon_client_lib::mobs::villager;
use horizon_world::poi::PoiType;

const SITE: [i32; 3] = [3, 1, 3];

static SITE_MEM: MemKey = MemKey::new("test:v1_site", Shape::Block);
static JOB_MEM: MemKey = MemKey::new("test:v1_job", Shape::Block);

fn accept_job(k: PoiType) -> bool {
    k.is_job()
}

fn no_acquired(_: &mut World, _: Entity, _: [i32; 3]) {}
fn no_lost(_: &mut World, _: Entity) {}
fn no_baby(_: &World, _: Entity) -> bool {
    false
}

fn ctx<T: std::any::Any + Sync>(p: &'static T) -> Ctx {
    Ctx { id: 0, nth: 0, params: p }
}

fn villager_at(h: &mut Herd, x: f64, z: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// A composter at the site, read by the registry: the point the validation looks for.
fn scene() -> Herd {
    let mut h = Herd::with_slab_awake(40, true, false);
    h.world.insert_resource(ClaimIndex::default());
    h.tick();
    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    h.tick();
    h
}

static VALIDATE: poi_validate::Params = poi_validate::Params { memory: &SITE_MEM, accept: accept_job, held: false, bed: false };

/// A point the villager remembers near it is dropped once the block goes.
#[test]
fn ecs_validate_drops_a_gone_point() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    set(&mut h.world, e, &SITE_MEM, MemValue::Block(SITE), None);
    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:air");
    h.tick();
    h.tick();
    let c = ctx(&VALIDATE);
    assert!((poi_validate::BEHAVIOUR.can_start)(&mut h.world, e, &c));
    (poi_validate::BEHAVIOUR.start)(&mut h.world, e, &c);
    assert!(!present(&h.world, e, &SITE_MEM));
}

/// The `Mob`: the job site goes with its block, and the profession with it (validation, then reset) within 5 ticks.
#[test]
fn mob_validate_drops_the_job_site_with_its_block() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(40, false);
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let i = r.add_kind(Vec3::new(3.5, 1.0, 3.5), mob_kind("minecraft:villager"));
    for _ in 0..40 {
        r.step(None);
    }
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:air");
    for _ in 0..5 {
        r.step(None);
    }
    assert_eq!(r.mobs[i].extra.profession.index(), Some(villager::NONE));
}

static COMPETE: poi_competitor_scan::Params = poi_competitor_scan::Params { job: &JOB_MEM };

/// Of two villagers of one profession with one job site, the one with less experience loses it.
#[test]
fn ecs_competitor_scan_loses_to_a_more_experienced_villager() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let a = villager_at(&mut h, 3.5, 3.5, 11);
    let b = villager_at(&mut h, 3.5, 3.6, 12);
    for e in [a, b] {
        set(&mut h.world, e, &JOB_MEM, MemValue::Block(SITE), None);
        h.world.entity_mut(e).insert(Profession(PoiType::Farmer));
    }
    h.tick();
    let c = ctx(&COMPETE);
    let lose = |w: &mut World, e: Entity| (poi_competitor_scan::BEHAVIOUR.can_start)(w, e, &c);
    let (x, y) = (lose(&mut h.world, a), lose(&mut h.world, b));
    assert!(x != y, "exactly one of two equal villagers loses the site");
}

/// The `Mob`: two villagers of one profession for one job site, exactly one keeps it.
#[test]
fn mob_competitor_one_site_one_profession() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(40, false);
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let i = r.add_kind(Vec3::new(3.5, 1.0, 2.5), mob_kind("minecraft:villager"));
    let j = r.add_kind(Vec3::new(3.5, 1.0, 4.5), mob_kind("minecraft:villager"));
    for _ in 0..300 {
        r.step(None);
    }
    let farmer = villager::profession_index("farmer").map(Some);
    let held = [i, j].iter().filter(|&&k| Some(r.mobs[k].extra.profession.index()) == farmer).count();
    assert_eq!(held, 1);
}

static YIELD: poi_yield_site::Params = poi_yield_site::Params { job: &JOB_MEM, potential: &SITE_MEM, baby: no_baby };

/// A villager without a profession gives up its potential site to a nearby villager of its profession who holds none.
#[test]
fn ecs_yield_job_site_gives_up_to_a_free_neighbour() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    let other = villager_at(&mut h, 3.5, 3.6, 12);
    set(&mut h.world, e, &SITE_MEM, MemValue::Block(SITE), None);
    h.world.entity_mut(other).insert(Profession(PoiType::Farmer));
    h.tick();
    let c = ctx(&YIELD);
    assert!((poi_yield_site::BEHAVIOUR.can_start)(&mut h.world, e, &c));
    (poi_yield_site::BEHAVIOUR.start)(&mut h.world, e, &c);
    assert!(!present(&h.world, e, &SITE_MEM));
}

static ASSIGN: poi_assign::Params = poi_assign::Params { job: &JOB_MEM, potential: &SITE_MEM, professions: &[] };

/// A potential site within 2 blocks becomes the job site and gives the profession of the site.
#[test]
fn ecs_assign_profession_takes_the_site_within_two_blocks() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    set(&mut h.world, e, &SITE_MEM, MemValue::Block(SITE), None);
    let c = ctx(&ASSIGN);
    assert!((poi_assign::ASSIGN.can_start)(&mut h.world, e, &c));
    (poi_assign::ASSIGN.start)(&mut h.world, e, &c);
    assert!(present(&h.world, e, &JOB_MEM));
    assert_eq!(h.world.get::<Profession>(e), Some(&Profession(PoiType::Farmer)));
}

/// The `Mob`: the composter's point is taken and the villager becomes a farmer within 40 ticks.
#[test]
fn mob_assign_takes_the_profession_of_the_site() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(40, false);
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let i = r.add_kind(Vec3::new(3.5, 1.0, 3.5), mob_kind("minecraft:villager"));
    for _ in 0..40 {
        r.step(None);
    }
    assert_eq!(Some(r.mobs[i].extra.profession.index()), villager::profession_index("farmer").map(Some));
}

static CLAIM: poi_claim::Params = poi_claim::Params { memory: &SITE_MEM, accept: accept_job, range: 48, filter: poi_claim::Filter::Any, on_acquired: no_acquired, on_lost: no_lost };
static ACQUIRE: poi_acquire::Params = poi_acquire::Params { claim: &CLAIM, adult: true, event: false, baby: no_baby };

/// An acquisition takes the nearest job point of the range, the memory holds it.
#[test]
fn ecs_acquire_takes_a_job_point_in_range() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    let c = ctx(&ACQUIRE);
    assert!((poi_acquire::BEHAVIOUR.can_start)(&mut h.world, e, &c));
    (poi_acquire::BEHAVIOUR.start)(&mut h.world, e, &c);
    assert!(present(&h.world, e, &SITE_MEM));
}

static RESET_SITE: poi_assign::Params = poi_assign::Params { job: &JOB_MEM, potential: &SITE_MEM, professions: &[] };

/// Without a job site, a villager with a profession and no experience gives it back.
#[test]
fn ecs_reset_profession_without_a_site_and_no_experience() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    h.world.entity_mut(e).insert(Profession(PoiType::Farmer));
    let c = ctx(&RESET_SITE);
    assert!((poi_assign::RESET.can_start)(&mut h.world, e, &c));
    (poi_assign::RESET.start)(&mut h.world, e, &c);
    assert!(h.world.get::<Profession>(e).is_none());
}

static GO_TO: poi_go_to_site::Params = poi_go_to_site::Params { potential: &SITE_MEM, speed: 0.5 };

/// The walk to a potential site starts from its memory, and its stop clears the memory.
#[test]
fn ecs_go_to_site_walks_then_stops_by_clearing_the_site() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 12.5, 3.5, 11);
    set(&mut h.world, e, &SITE_MEM, MemValue::Block(SITE), None);
    let c = ctx(&GO_TO);
    assert!((poi_go_to_site::BEHAVIOUR.can_start)(&mut h.world, e, &c));
    (poi_go_to_site::BEHAVIOUR.start)(&mut h.world, e, &c);
    assert!(present(&h.world, e, &WALK_TARGET));
    (poi_go_to_site::BEHAVIOUR.stop)(&mut h.world, e, &c);
    assert!(!present(&h.world, e, &SITE_MEM));
}

/// The `Mob`: a villager that has the composter in range walks to it.
#[test]
fn mob_go_to_site_walks_to_the_potential_site() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(40, false);
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let i = r.add_kind(Vec3::new(12.5, 1.0, 3.5), mob_kind("minecraft:villager"));
    let start = r.mobs[i].position;
    for _ in 0..300 {
        r.step(None);
    }
    let goal = Vec3::new(3.5, 1.0, 3.5);
    assert!(r.mobs[i].position.distance_squared(goal) < start.distance_squared(goal));
}
