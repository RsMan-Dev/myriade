//! The skeleton trap (`SkeletonTrapGoal`, `SkeletonHorse.isTrap`), written, not run: an armed horse waits, a player within 10 blocks springs it
//! (the bolt, the horse tamed and ridden, three more horses each ridden), and it goes after 18000 ticks of its own time.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::spring_trap::{self, Armed, Tamed};

fn count(h: &mut Herd, kind: &str) -> usize {
    let id = h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == kind).unwrap().id;
    h.world.query::<&SpeciesId>().iter(&h.world).filter(|s| s.0 == id).count()
}

fn horse(h: &mut Herd) -> Entity {
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:skeleton_horse").unwrap();
    spawn_armed(&mut h.world, &sp, 0.5, 0.5, 11)
}

/// No player within 10 blocks: the horse stays armed and nothing appears.
#[test]
fn an_armed_horse_waits_while_no_player_is_near() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_online(&[(30.5, 0.5)]);
    let e = horse(&mut h);
    assert!(spring_trap::arm(&mut h.world, e, 0));
    spring_trap::player_changed(&mut h.world);
    assert!(h.world.get::<Armed>(e).is_some() && h.world.get::<Tamed>(e).is_none());
    assert_eq!(count(&mut h, "minecraft:skeleton_horse"), 1);
}

/// A player within 10 blocks springs it: the horse is tamed, disarmed and ridden, three more horses come, each with a rider.
#[test]
fn a_player_in_range_springs_the_trap() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_online(&[(30.5, 0.5)]);
    let e = horse(&mut h);
    spring_trap::arm(&mut h.world, e, 0);
    h.set_online(&[(3.5, 0.5)]);
    spring_trap::player_changed(&mut h.world);
    assert!(h.world.get::<Armed>(e).is_none() && h.world.get::<Tamed>(e).is_some());
    assert_eq!(count(&mut h, "minecraft:skeleton_horse"), 4);
    assert_eq!(count(&mut h, "minecraft:skeleton"), 4);
}

/// A horse that is not armed does nothing, and `SkeletonTrapTime` counts the ticks waited.
#[test]
fn a_disarmed_horse_springs_nothing_and_the_wait_is_counted() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_online(&[(3.5, 0.5)]);
    let e = horse(&mut h);
    assert_eq!(spring_trap::elapsed(&h.world, e), None);
    spring_trap::player_changed(&mut h.world);
    assert_eq!(count(&mut h, "minecraft:skeleton"), 0);
    spring_trap::set_elapsed(&mut h.world, e, 500);
    assert_eq!(spring_trap::loaded_time(&h.world, e), 500);
}
