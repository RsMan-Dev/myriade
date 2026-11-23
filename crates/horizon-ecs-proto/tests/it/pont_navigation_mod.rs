//! The navigation a mod drives on a creature (greffe lot 81): a supplied path followed from the next step, the flags and the path
//! maluses, with no tweak stored until a mod writes one. The fixtures are those of `pont_lectures_ia`.
use crate::common::*;
use myriade_engine_lib::mobs::world::leaked_test_world_mut;
use horizon_ecs_proto::{
    behaviours::restrict_sun::AvoidSun,
    bridge::host::{EcsWorld, HostSetup},
    cow::{Nav, NavParams, Pos},
};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

#[test]
fn supplied_path_is_followed_from_the_next_step() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let nodes = [[1.5, 1.0, 0.5], [2.5, 1.0, 0.5], [3.5, 1.0, 0.5]];
    assert!(h.nav_move_to_supplied(mw, e, &nodes, 1.0));
    assert_eq!(h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).map(|p| p.nodes.len()), Some(3));
    let x0 = h.world().get::<Pos>(e).expect("a position").0.x;
    h.step(mw);
    assert!(h.world().get::<Pos>(e).expect("a position").0.x > x0);
}

#[test]
fn empty_path_and_creature_without_navigation_are_refused() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(!h.nav_move_to_supplied(mw, e, &[], 1.0));
    let bare = h.world_mut().spawn_empty().id();
    assert!(!h.nav_move_to_supplied(mw, bare, &[[1.5, 1.0, 0.5]], 1.0));
}

#[test]
fn flag_and_malus_read_back_what_was_written() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.set_nav_flag(e, "can_open_doors", true));
    assert_eq!(h.nav_flag(e, "can_open_doors"), Some(true));
    assert!(!h.set_nav_flag(e, "no_such_flag", true));
    assert!(h.set_nav_malus(e, "WATER", 2.5));
    assert_eq!(h.nav_malus(e, "WATER"), Some(2.5));
    assert!(!h.set_nav_malus(e, "NO_SUCH_TYPE", 1.0));
}

#[test]
fn avoid_sun_flag_is_the_marker() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.set_nav_flag(e, "avoid_sun", true));
    assert!(h.world().get::<AvoidSun>(e).is_some());
    assert_eq!(h.nav_flag(e, "avoid_sun"), Some(true));
}

#[test]
fn flag_write_lands_on_the_creature_navigation() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.set_nav_flag(e, "can_pass_doors", true));
    assert!(h.set_nav_malus(e, "FENCE", 3.0));
    let n = h.world().get::<NavParams>(e).expect("a navigation");
    assert!(n.can_pass_doors);
    assert_eq!(n.malus[myriade_engine_lib::mobs::path::PathType::Fence as usize], Some(3.0));
}
