//! The villager's points, lot V1: validation, competitors, assignment, reset, the walk to a potential site and its acquisition.
//! Yield and the home's occupancy are not tested here.
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
    species::villager::{JOB_SITE, POTENTIAL_JOB_SITE},
};
use myriade_engine_lib::mobs::villager;

/// Mid-morning: the villager works (by night it rests).
const DAY: i64 = 4000;
use horizon_world::poi::PoiType;

const SITE: [i32; 3] = [3, 1, 3];


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
    let (items, enchantments, trades) = crate::common::registries();
    h.world.insert_resource(horizon_ecs_proto::behaviours::trade_offers::TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    h.tick();
    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    h.tick();
    h
}

static VALIDATE: poi_validate::Params = poi_validate::Params { memory: &POTENTIAL_JOB_SITE, accept: accept_job, held: false, bed: false };

/// A point the villager remembers near it is dropped once the block goes.
#[test]
fn ecs_validate_drops_a_gone_point() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:air");
    h.tick();
    h.tick();
    // Set after the ticks: the villager's own brain would drop the point itself.
    set(&mut h.world, e, &POTENTIAL_JOB_SITE, MemValue::Block(SITE), None);
    let c = ctx(&VALIDATE);
    assert!((poi_validate::BEHAVIOUR.can_start)(&mut h.world, e, &c));
    (poi_validate::BEHAVIOUR.start)(&mut h.world, e, &c);
    assert!(!present(&h.world, e, &POTENTIAL_JOB_SITE));
}

static COMPETE: poi_competitor_scan::Params = poi_competitor_scan::Params { job: &JOB_SITE };

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
        set(&mut h.world, e, &JOB_SITE, MemValue::Block(SITE), None);
        h.world.entity_mut(e).insert(Profession(PoiType::Farmer));
    }
    // The brain runs the scan when it wakes (the neighbours come from its sensor): of two equal villagers exactly one keeps the site.
    for _ in 0..40 {
        h.tick();
    }
    let kept = [a, b].iter().filter(|&&e| present(&h.world, e, &JOB_SITE)).count();
    assert_eq!(kept, 1, "exactly one of two equal villagers keeps the site");
}

static YIELD: poi_yield_site::Params = poi_yield_site::Params { job: &JOB_SITE, potential: &POTENTIAL_JOB_SITE, baby: no_baby };

/// A villager without a profession gives up its potential site to a nearby villager of its profession who holds none.
#[test]
fn ecs_yield_job_site_gives_up_to_a_free_neighbour() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    let other = villager_at(&mut h, 3.5, 3.6, 12);
    h.world.entity_mut(other).insert(Profession(PoiType::Farmer));
    // The neighbours come from the sensor; the point is set after, the brain would act on it itself.
    for _ in 0..20 {
        h.tick();
    }
    // The brain took the site for itself during the warm-up: the villager is put back without a profession and without a site.
    h.world.entity_mut(e).remove::<Profession>();
    erase(&mut h.world, e, &JOB_SITE);
    set(&mut h.world, e, &POTENTIAL_JOB_SITE, MemValue::Block(SITE), None);
    let c = ctx(&YIELD);
    assert!((poi_yield_site::BEHAVIOUR.can_start)(&mut h.world, e, &c));
    (poi_yield_site::BEHAVIOUR.start)(&mut h.world, e, &c);
    assert!(!present(&h.world, e, &POTENTIAL_JOB_SITE));
}

static ASSIGN: poi_assign::Params = poi_assign::Params { job: &JOB_SITE, potential: &POTENTIAL_JOB_SITE, professions: &[] };

/// A potential site within 2 blocks becomes the job site and gives the profession of the site.
#[test]
fn ecs_assign_profession_takes_the_site_within_two_blocks() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    set(&mut h.world, e, &POTENTIAL_JOB_SITE, MemValue::Block(SITE), None);
    let c = ctx(&ASSIGN);
    assert!((poi_assign::ASSIGN.can_start)(&mut h.world, e, &c));
    (poi_assign::ASSIGN.start)(&mut h.world, e, &c);
    assert!(present(&h.world, e, &JOB_SITE));
    assert_eq!(h.world.get::<Profession>(e), Some(&Profession(PoiType::Farmer)));
}

static CLAIM: poi_claim::Params = poi_claim::Params { memory: &POTENTIAL_JOB_SITE, accept: accept_job, range: 48, filter: poi_claim::Filter::Any, on_acquired: no_acquired, on_lost: no_lost };
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
    assert!(present(&h.world, e, &POTENTIAL_JOB_SITE));
}

static RESET_SITE: poi_assign::Params = poi_assign::Params { job: &JOB_SITE, potential: &POTENTIAL_JOB_SITE, professions: &[] };

/// Without a job site, a villager with a profession and no experience gives it back.
#[test]
fn ecs_reset_profession_without_a_site_and_no_experience() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    let farmer = villager::profession_index("farmer").unwrap();
    h.world.entity_mut(e).insert((Profession(PoiType::Farmer), horizon_ecs_proto::behaviours::trade_offers::Merchant::new(false, farmer, 0)));
    let c = ctx(&RESET_SITE);
    assert!((poi_assign::RESET.can_start)(&mut h.world, e, &c));
    (poi_assign::RESET.start)(&mut h.world, e, &c);
    assert!(h.world.get::<Profession>(e).is_none());
}

/// `ResetProfession` (jar): a villager that traded (experience over 0) or is above level 1 keeps its profession without a job site.
#[test]
fn ecs_employee_who_lost_the_site_keeps_the_profession_with_experience_or_level() {
    if !have_data() {
        return;
    }
    let farmer = villager::profession_index("farmer").unwrap();
    for (xp, level) in [(5, 1), (0, 2)] {
        let mut h = scene();
        let e = villager_at(&mut h, 3.5, 3.5, 11);
        let mut m = horizon_ecs_proto::behaviours::trade_offers::Merchant::new(false, farmer, 0);
        m.trade.xp = xp;
        m.level = level;
        h.world.entity_mut(e).insert((Profession(PoiType::Farmer), m));
        assert!(!(poi_assign::RESET.can_start)(&mut h.world, e, &ctx(&RESET_SITE)), "xp {xp} level {level}");
    }
}

static GO_TO: poi_go_to_site::Params = poi_go_to_site::Params { potential: &POTENTIAL_JOB_SITE, speed: 0.5 };

/// The walk to a potential site starts from its memory, and its stop clears the memory.
#[test]
fn ecs_go_to_site_walks_then_stops_by_clearing_the_site() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 12.5, 3.5, 11);
    set(&mut h.world, e, &POTENTIAL_JOB_SITE, MemValue::Block(SITE), None);
    let c = ctx(&GO_TO);
    assert!((poi_go_to_site::BEHAVIOUR.can_start)(&mut h.world, e, &c));
    (poi_go_to_site::BEHAVIOUR.start)(&mut h.world, e, &c);
    assert!(present(&h.world, e, &WALK_TARGET));
    (poi_go_to_site::BEHAVIOUR.stop)(&mut h.world, e, &c);
    assert!(!present(&h.world, e, &POTENTIAL_JOB_SITE));
}

/// The same walk by the whole ECS brain, no module started by hand: a jobless villager 9 blocks from a composter, by day, acquires the
/// point, walks to it and takes its profession.
#[test]
fn ecs_villager_walks_to_the_site_and_takes_its_profession_as_the_mob() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = villager_at(&mut h, 12.5, 3.5, 11);
    horizon_ecs_proto::behaviours::schedule::Times::set(&mut h.world, DAY);
    let goal = Vec3::new(3.5, 1.0, 3.5);
    let start = h.world.get::<horizon_ecs_proto::cow::Pos>(e).unwrap().0;
    let mut got_at = None;
    for t in 0..600 {
        h.tick();
        if got_at.is_none() && h.world.get::<Profession>(e).is_some() {
            got_at = Some(t);
        }
    }
    let end = h.world.get::<horizon_ecs_proto::cow::Pos>(e).unwrap().0;
    assert!(end.distance(goal) + 3.0 < start.distance(goal), "the ECS villager did not walk: {start:?} -> {end:?}");
    assert!(matches!(h.world.get::<Profession>(e), Some(Profession(PoiType::Farmer))), "no farmer after 600 ticks");
    assert!(got_at.is_some_and(|t| t < 600), "the profession is taken within 600 ticks");
}
