//! The pilot's input through the world: an unchanged input writes nothing, a boat steered by its pilot moves, a clear gives the keys back
//! to rest, a mount and a minecart read the same input. The world is a stone floor at y = 0, built like the boat's fixtures.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    boat_state,
    boat_tick::{self, BoatEvents, Steered},
    cart_move::Rolling,
    cow::{build_level_with, Level, Moving, Pos},
    minecart::Kind,
    mount::Mount,
    pilot::{self, PilotInput},
    targeting::Who,
};
use std::sync::Arc;

fn level() -> Level {
    build_level_with(0, |blocks, registry| {
        for cx in -2..2 {
            for cz in -2..2 {
                blocks.chunks.insert((cx, cz), Arc::new(horizon_client_render::chunk::Chunk::empty(cx, cz)));
            }
        }
        let stone = registry.default_state("minecraft:stone").unwrap();
        for x in -8..8 {
            for z in -8..8 {
                blocks.set(x, 0, z, stone).unwrap();
            }
        }
    })
    .0
}

/// A world with the level and an awake oak boat on the floor.
fn world_with_boat() -> (World, Entity) {
    let mut w = World::new();
    w.insert_resource(level());
    w.insert_resource(BoatEvents::default());
    let e = boat_state::spawn(&mut w, 0, false, [0.5, 1.0, 0.5], 0.0);
    (w, e)
}

/// The pilot `who`'s input: `forward` ahead, `strafe` to the right, the boat's `keys` (left, right, up, down).
fn input(who: Who, forward: f32, strafe: f32, keys: [bool; 4]) -> PilotInput {
    PilotInput { who, forward, strafe, yaw: 0.0, pitch: 0.0, jump: 0.0, sprint: false, sneak: false, steer: false, keys }
}

#[test]
fn the_same_input_twice_writes_once() {
    if !have_data() {
        return;
    }
    let (mut w, e) = world_with_boat();
    assert!(pilot::set(&mut w, e, input(Who::Player(7), 1.0, 0.0, [false, false, true, false])));
    assert!(!pilot::set(&mut w, e, input(Who::Player(7), 1.0, 0.0, [false, false, true, false])));
}

#[test]
fn a_boat_steered_up_moves() {
    if !have_data() {
        return;
    }
    let (mw, mut h) = awake_host_on_stone(&horizon_ecs_proto::bridge::host::HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 });
    let e = h.spawn_boat(mw, "oak", false, [0.5, 1.0, 0.5], 0.0).expect("a known wood");
    let before = h.world().get::<Pos>(e).expect("a position").0;
    pilot::set(h.world_mut(), e, input(Who::Player(7), 1.0, 0.0, [false, false, true, false]));
    for _ in 0..5 {
        h.step(mw);
    }
    let after = h.world().get::<Pos>(e).expect("a position").0;
    assert!((after.x, after.z) != (before.x, before.z), "the up key pushes the boat along the water");
}

#[test]
fn clear_gives_the_keys_back_to_rest() {
    if !have_data() {
        return;
    }
    let (mut w, e) = world_with_boat();
    pilot::set(&mut w, e, input(Who::Player(7), 1.0, 0.0, [false, false, true, false]));
    assert!(w.get::<Steered>(e).is_some());
    pilot::clear(&mut w, e);
    assert!(pilot::input_of(&w, e).is_none());
    assert!(w.get::<Steered>(e).is_none());
}

#[test]
fn the_pilot_stays_a_player() {
    if !have_data() {
        return;
    }
    let (mut w, e) = world_with_boat();
    pilot::set(&mut w, e, input(Who::Player(7), 0.0, 1.0, [false, true, false, false]));
    let p = pilot::input_of(&w, e).expect("a pilot");
    assert_eq!(p.who, Who::Player(7));
    assert_eq!(p.keys, [false, true, false, false]);
}

#[test]
fn a_mount_reads_the_rider_input_of_the_pilot() {
    if !have_data() {
        return;
    }
    let mut w = World::new();
    let e = w.spawn(Mount::default()).id();
    let mut p = input(Who::Player(7), 1.0, -0.5, [false; 4]);
    p.jump = 0.25;
    p.steer = true;
    assert!(pilot::set(&mut w, e, p));
    let r = pilot::as_rider_input(&pilot::input_of(&w, e).expect("a pilot"));
    assert_eq!((r.forward, r.strafe, r.jump, r.steer), (1.0, -0.5, 0.25, true));
    assert!(w.get::<Moving>(e).is_some(), "a mount with a pilot wakes");
}

#[test]
fn a_minecart_with_a_pilot_rolls_and_takes_its_intent() {
    if !have_data() {
        return;
    }
    let (mw, mut h) = awake_host(&horizon_ecs_proto::bridge::host::HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 });
    let e = h.spawn_cart(mw, "minecraft:minecart", [0.5, 1.0, 0.5], 0.0).expect("a known genre");
    let w = h.world_mut();
    assert!(pilot::set(w, e, input(Who::Player(7), 0.5, 0.25, [false; 4])));
    assert!(w.get::<Rolling>(e).is_some(), "a minecart with a pilot rolls");
    assert_eq!(pilot::cart_intent(&pilot::input_of(w, e).expect("a pilot")), [0.25, 0.5]);
}
