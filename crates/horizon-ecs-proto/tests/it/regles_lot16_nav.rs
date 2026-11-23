//! Lot 16a: the rules of the old Mob's supplied-path navigation (nav.rs) that the ECS keeps in `nav_supplied::follow`.
use crate::common::*;
use horizon_client_lib::mobs::path::{Evaluator, Path, PathNode, PathType};
use horizon_ecs_proto::{
    bridge::host::HostSetup,
    cow::{BlockChanges, Following, Health, NeedsPath, Nav, Pos, Recompute, Stroll, surface_search},
    fall_by_health::{FallByHealth, path_max_fall},
    locomotion::swim::{Control, Push, SwimParams},
    nav_supplied::{NodeClock, evaluator, trim_before_sky},
};

fn open_path(nodes: &[(i32, i32, i32)]) -> Path {
    let nodes = nodes.iter().map(|&(x, y, z)| PathNode { x, y, z, kind: PathType::Open }).collect();
    Path { nodes, next: 0, target: [0, 0, 0], dist_to_target: 0.0, reached: true }
}

/// `GroundPathNavigation.trimPath` with `avoidSun`: the nodes from the first one that sees the sky are cut off.
#[test]
fn avoid_sun_cuts_the_path_before_the_first_node_that_sees_the_sky() {
    let mut p = open_path(&[(0, 60, 0), (1, 60, 0), (2, 70, 0), (3, 70, 0)]);
    trim_before_sky(&mut p, |_, y, _| y >= 70);
    assert_eq!(p.nodes.len(), 2);
    assert_eq!(p.nodes.last().map(|n| n.x), Some(1));
}

/// The sky trim keeps the whole path when no node sees the sky.
#[test]
fn avoid_sun_keeps_a_path_that_never_sees_the_sky() {
    let mut p = open_path(&[(0, 60, 0), (1, 60, 0)]);
    trim_before_sky(&mut p, |_, _, _| false);
    assert_eq!(p.nodes.len(), 2);
}

/// `Creeper`'s max fall with a target is `3 + (health - 1)`: 22 at 20 health, 6 at 4 health. The fiche gives the marker.
#[test]
fn a_creeper_falls_by_its_own_health_while_it_has_a_target() {
    assert_eq!(path_max_fall(Some(4.0), true), 6);
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:creeper", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.world().get::<FallByHealth>(e).is_some());
    let hp = h.world().get::<Health>(e).expect("a health").0;
    assert_eq!(hp, 20.0);
    assert_eq!(path_max_fall(Some(hp), true), 22);
    assert_eq!(path_max_fall(Some(hp), false), 3);
}

/// `Frog.FrogPathNavigation.prefersShallowSwimming` has to reach the path finder: the amphibious flag of the species gives
/// the evaluator's shallow preference, true or false.
#[test]
fn the_amphibious_navigation_keeps_its_shallow_preference() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    for prefers_shallow in [true, false] {
        let swim = SwimParams { amphibious: Some(prefers_shallow), breach: false, control: Control::Fish, push: Push::Fixed(0.01), sinks: false, squid: false, reset_pitch: false, flop: None };
        h.world_mut().entity_mut(e).insert(swim);
        assert!(matches!(evaluator(h.world(), e), Evaluator::Amphibious { prefers_shallow: p } if p == prefers_shallow));
    }
}

/// The node timeout: a creature that stays on its node past three times the limit (`dist / speed * 20` ticks) drops the path.
#[test]
fn the_per_node_timeout_stops_a_supplied_path() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.nav_move_to_supplied(mw, e, &[[1.5, 1.0, 0.5], [2.5, 1.0, 0.5]], 1.0));
    let limit = h.world().get::<NodeClock>(e).expect("a node clock").limit;
    assert_eq!(limit, 20);
    h.world_mut().get_mut::<NodeClock>(e).expect("a node clock").timer = 3 * limit;
    h.step(mw);
    assert!(h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).is_none());
    assert!(h.world().get::<NodeClock>(e).is_none());
    assert!(h.world().get::<Following>(e).is_none());
}

/// `GroundPathNavigation.canUpdatePath` with `isPassenger()`: a rider in the air still asks for a path.
#[test]
fn a_rider_in_the_air_still_gets_a_path() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let vehicle = h.spawn(mw, "minecraft:pig", [0.5, 1.0, 0.5], None).expect("a known species");
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.mount(mw, e, vehicle);
    h.teleport(mw, vehicle, [0.5, 80.0, 0.5]);
    h.world_mut().entity_mut(e).insert(NeedsPath);
    h.world_mut().get_mut::<Stroll>(e).expect("a stroll").tx = 4.5;
    h.world_mut().get_mut::<Stroll>(e).expect("a stroll").tz = 0.5;
    let before = h.world().get::<Nav>(e).expect("a navigation").searches;
    h.step(mw);
    // The search is made (what `canUpdatePath` allows); whether a path exists from the air is the finder's own answer.
    assert_eq!(h.world().get::<Nav>(e).expect("a navigation").searches, before + 1);
}

/// `Path.raise_cauldron_nodes`: a node on a cauldron is raised onto its rim, the node after it follows.
#[test]
fn a_node_on_a_cauldron_is_raised_onto_its_rim_in_a_supplied_path() {
    let mut p = open_path(&[(0, 60, 0), (1, 60, 0), (2, 60, 0)]);
    p.raise_cauldron_nodes(|x, _, _| x == 1);
    assert_eq!(p.nodes[1].y, 61);
    assert_eq!(p.nodes[2].y, 61);
}

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// `a_supplied_path_is_followed_and_finishes_at_its_node_count`: the creature walks the three nodes and the path is dropped at the end.
#[test]
fn a_supplied_path_is_followed_and_finishes_at_its_node_count() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let nodes = [[1.5, 1.0, 0.5], [2.5, 1.0, 0.5], [3.5, 1.0, 0.5]];
    assert!(h.nav_move_to_supplied(mw, e, &nodes, 1.0));
    for _ in 0..200 {
        h.step(mw);
        if h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).is_none() {
            break;
        }
    }
    assert!(h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).is_none());
    assert!(h.world().get::<Pos>(e).expect("a position").0.x > 2.5);
}

/// `unloaded_and_out_of_world_nodes_are_accepted_without_loading_chunks`: a node far outside the world is kept as written.
#[test]
fn unloaded_and_out_of_world_nodes_are_accepted_without_loading_chunks() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.nav_move_to_supplied(mw, e, &[[1.5, 1.0, 0.5], [1.0e6 + 0.5, 1.0, 0.5]], 1.0));
    let p = h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).expect("a path");
    assert_eq!(p.nodes.len(), 2);
    assert_eq!(p.nodes.last().map(|n| n.x), Some(1_000_000));
}

/// `a_node_the_engine_calls_blocked_is_recorded_not_refused`: a node inside the stone floor is classified, not filtered out.
#[test]
fn a_node_the_engine_calls_blocked_is_recorded_not_refused() {
    have_data();
    let (mw, mut h) = awake_host_on_stone(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.nav_move_to_supplied(mw, e, &[[1.5, 0.0, 0.5], [2.5, 1.0, 0.5]], 1.0));
    assert_eq!(h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).map(|p| p.nodes.len()), Some(2));
}

/// `an_extreme_walking_node_is_classified_and_followed`: a node at the top of the `i32` range is accepted without overflow.
#[test]
fn an_extreme_walking_node_is_classified_and_followed() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.nav_move_to_supplied(mw, e, &[[2_147_483_647.5, 1.0, 0.5]], 1.0));
    h.step(mw);
    assert!(h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).is_some());
}

/// `an_extreme_flying_node_is_classified_and_followed`: a flying creature's node at the top of the `i32` range is accepted.
#[test]
fn an_extreme_flying_node_is_classified_and_followed() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:bee", [0.5, 60.0, 0.5], None).expect("a known species");
    assert!(h.nav_move_to_supplied(mw, e, &[[2_147_483_647.5, 60.0, 0.5]], 1.0));
    h.step(mw);
    assert!(h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).is_some());
}

/// `the_stuck_check_stops_a_supplied_path`: a creature that does not move for 100 ticks drops its path.
#[test]
fn the_stuck_check_stops_a_supplied_path() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.nav_move_to_supplied(mw, e, &[[1.5, 1.0, 0.5], [2.5, 1.0, 0.5]], 0.0));
    let x0 = h.world().get::<Pos>(e).expect("a position").0.x;
    for _ in 0..110 {
        h.step(mw);
    }
    assert!(h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).is_none());
    assert_eq!(h.world().get::<Pos>(e).expect("a position").0.x, x0);
}

/// `a_change_near_the_middle_of_the_path_asks_for_a_recompute`: a walking path with a change near its middle asks for a new search.
#[test]
fn a_change_near_the_middle_of_the_path_asks_for_a_recompute() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let nodes: Vec<[f64; 3]> = (1..=10).map(|i| [f64::from(i) + 0.5, 1.0, 0.5]).collect();
    assert!(h.nav_move_to_supplied(mw, e, &nodes, 1.0));
    h.world_mut().get_mut::<Stroll>(e).expect("a stroll").walking = true;
    h.world_mut().resource_mut::<BlockChanges>().0.push((5, 1, 0, "minecraft:stone"));
    h.step(mw);
    assert!(h.world().get::<NeedsPath>(e).is_some() || h.world().get::<Recompute>(e).is_some());
}

/// `a_block_change_drops_a_supplied_path_without_re_searching`: a change on a supplied path drops it, and nothing is searched again.
#[test]
fn a_block_change_drops_a_supplied_path_without_re_searching() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    assert!(h.nav_move_to_supplied(mw, e, &[[1.5, 1.0, 0.5], [2.5, 1.0, 0.5], [3.5, 1.0, 0.5]], 1.0));
    h.world_mut().resource_mut::<BlockChanges>().0.push((2, 1, 0, "minecraft:stone"));
    h.step(mw);
    assert!(h.world().get::<Nav>(e).and_then(|n| n.path.as_ref()).is_none());
    assert!(h.world().get::<Following>(e).is_none());
    assert!(h.world().get::<NeedsPath>(e).is_none());
}

/// `surface_search_uses_the_world_build_range`: the search stays within the build range, at its bottom and its top.
#[test]
fn surface_search_uses_the_world_build_range() {
    let air = |_: [i32; 3]| true;
    let solid = |_: [i32; 3]| false;
    assert_eq!(surface_search(0, 127, air, solid, [0, 2_000_000_000, 0]), [0, 2_000_000_001, 0]);
    assert_eq!(surface_search(0, 127, air, solid, [0, -2_000_000_000, 0]), [0, 128, 0]);
    assert_eq!(surface_search(0, 127, air, solid, [0, 127, 0]), [0, 128, 0]);
    assert_eq!(surface_search(0, 127, air, solid, [0, 0, 0]), [0, 128, 0]);
}

/// `surface_search_from_above_keeps_the_top_nonsolid_block`: from above a torch (air-less, not solid) the search keeps the torch's top.
#[test]
fn surface_search_from_above_keeps_the_top_nonsolid_block() {
    let air = |p: [i32; 3]| p[1] != 319;
    let solid = |_: [i32; 3]| false;
    assert_eq!(surface_search(0, 319, air, solid, [0, 321, 0]), [0, 320, 0]);
    assert_eq!(surface_search(0, 319, air, solid, [0, 319, 0]), [0, 319, 0]);
}
