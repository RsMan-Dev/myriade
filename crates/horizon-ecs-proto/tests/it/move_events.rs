//! The movement game events: off unless the host asks, a walk steps with `moveDist`, a landing hits the ground, a type with no emission is silent.
use crate::common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::move_emission::{Cfg, Params};
use horizon_ecs_proto::bridge::host::HostSetup;
use horizon_ecs_proto::cow::Moving;
use horizon_ecs_proto::goals;
use horizon_ecs_proto::host_out::HostEvent;

macro_rules! world {
    ($h:ident, $mw:ident) => {
        have_data();
        let ($mw, mut $h) = awake_host_on_stone(&HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 });
    };
}

static NONE: Params = Params { events: false, ..Params::DEFAULT };

fn count(evs: &[HostEvent], event: &str) -> usize {
    evs.iter().filter(|x| matches!(x, HostEvent::GameEventBy { name, .. } if *name == event)).count()
}

// A downward pull each tick keeps the ground contact (`on_ground` comes from the push into the floor).
macro_rules! walk {
    ($h:ident, $mw:ident, $e:ident, $ticks:expr) => {{
        let mut out = Vec::new();
        for _ in 0..$ticks {
            goals::set_velocity($h.world_mut(), $e, Vec3::new(0.2, -0.08, 0.0));
            $h.world_mut().entity_mut($e).insert(Moving);
            out.extend($h.step($mw));
        }
        out
    }};
}

#[test]
fn a_walk_steps_each_time_move_dist_passes_next_step() {
    world!(h, mw);
    h.set_move_events(true);
    // The floor ends at x = 8: 60 ticks of 0.2 from -7.5 stay on it.
    let e = h.spawn(mw, "minecraft:cow", [-7.5, 1.0, 0.5], None).expect("a known species");
    let _ = walk!(h, mw, e, 40);
    // 0.2 a tick is 0.12 of moveDist: 20 ticks cross it twice (1.0 and 2.0).
    let evs = walk!(h, mw, e, 20);
    assert!((1..=3).contains(&count(&evs, "minecraft:step")), "{evs:?}");
}

#[test]
fn nothing_is_emitted_while_the_gate_is_off() {
    world!(h, mw);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let evs = walk!(h, mw, e, 60);
    assert_eq!(evs.iter().filter(|x| matches!(x, HostEvent::GameEventBy { .. })).count(), 0);
}

#[test]
fn a_type_with_no_emission_makes_no_step() {
    world!(h, mw);
    h.set_move_events(true);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().entity_mut(e).insert(Cfg(&NONE));
    let _ = walk!(h, mw, e, 40);
    assert_eq!(count(&walk!(h, mw, e, 20), "minecraft:step"), 0);
}

#[test]
fn a_fall_hits_the_ground() {
    world!(h, mw);
    h.set_move_events(true);
    let e = h.spawn(mw, "minecraft:cow", [0.5, 6.0, 0.5], None).expect("a known species");
    let mut evs = Vec::new();
    for _ in 0..60 {
        h.world_mut().entity_mut(e).insert(Moving);
        evs.extend(h.step(mw));
    }
    assert!(count(&evs, "minecraft:hit_ground") >= 1, "{evs:?}");
}
