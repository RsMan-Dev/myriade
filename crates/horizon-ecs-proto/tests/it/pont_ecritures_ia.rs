//! The AI's writes over the bridge (lot 44): the path and its stop, the wish, the restriction, the pause, the aggression, the sleep, the
//! size of a baby and the no-action count. The readers are those of lots 40 and 41, named as their cahiers name them.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::despawn::Despawn;
use horizon_ecs_proto::targeting::Who;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// Runs `f` on a host with one cow at the centre of a block, and no player.
fn cow_world(f: impl FnOnce(&mut EcsWorld, Entity)) {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    f(&mut h, e);
}

#[test]
fn nav_move_to_then_stop_navigation() {
    have_data();
    let (mw, mut h) = awake_host_on_stone(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    // Three ticks settle it on the floor: no path is made in the air.
    for _ in 0..3 {
        h.step(mw);
    }
    assert!(h.nav_move_to(e, [4.5, 1.0, 0.5], 1.0));
    h.step(mw);
    assert!(!h.nav_is_done(e), "a path is being walked");
    h.stop_navigation(e);
    assert!(h.nav_is_done(e), "the stop ends the path");
}

#[test]
fn set_wanted_is_read_back() {
    cow_world(|h, e| {
        h.set_wanted(e, [3.0, 1.0, 4.0], 1.0);
        assert_eq!(h.wanted(e), Some([3.0, 1.0, 4.0]));
    });
}

#[test]
fn restriction_holds_inside_and_is_cleared() {
    cow_world(|h, e| {
        h.restrict_to(e, [0, 1, 0], 5);
        assert!(h.has_restriction(e));
        assert!(h.is_within_restriction(e, [1.0, 1.0, 1.0]));
        assert!(!h.is_within_restriction(e, [50.0, 1.0, 50.0]));
        h.clear_restriction(e);
        assert!(!h.has_restriction(e));
    });
}

#[test]
fn ai_pause_is_read_back() {
    cow_world(|h, e| {
        h.set_ai_paused(e, true);
        assert!(h.ai_paused(e));
        h.set_ai_paused(e, false);
        assert!(!h.ai_paused(e));
    });
}

#[test]
fn aggression_is_read_back() {
    cow_world(|h, e| {
        h.set_aggressive(e, true);
        assert!(h.aggressive(e));
        h.set_aggressive(e, false);
        assert!(!h.aggressive(e));
    });
}

#[test]
fn start_sleeping_is_read_back() {
    cow_world(|h, e| {
        h.start_sleeping(e, [2, 64, 3], Vec3::new(2.5, 64.5, 3.5));
        assert_eq!(h.sleeping_pos(e), Some([2, 64, 3]));
        h.stop_sleeping(e);
        assert_eq!(h.sleeping_pos(e), None);
    });
}

#[test]
fn set_target_then_ai_target() {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.set_target(mw, e, Some(Who::Player(1)));
    assert!(matches!(h.ai_target(e), Some(Who::Player(1))));
    h.set_target(mw, e, None);
    assert!(h.ai_target(e).is_none());
}

#[test]
fn baby_is_smaller_and_grows_back() {
    cow_world(|h, e| {
        let adult = h.height(e);
        h.set_baby(e, true);
        assert!(h.height(e) < adult, "a baby is the baby box");
        h.set_baby(e, false);
        assert_eq!(h.height(e), adult, "an adult takes its own box back");
    });
}

#[test]
fn no_action_time_zero_restarts_the_despawn_count() {
    cow_world(|h, e| {
        h.world_mut().get_mut::<Despawn>(e).expect("a creature that may go away").deadline = 0;
        h.world_mut().get_mut::<horizon_ecs_proto::cow::ActiveUntil>(e).unwrap().0 = 1;
        h.set_no_action_time(e, 0);
        assert!(h.world().get::<Despawn>(e).unwrap().deadline > 0, "the deadline is pushed back");
    });
}
