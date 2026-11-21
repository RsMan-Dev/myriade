//! The boats' tick in a world: a boat that lands on a floor sleeps, a fall past three blocks reports the fall, and a boat under the world
//! is discarded. The world is a stone floor at y = 0 (its top is y = 1), built like the other `Level` fixtures.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{
    boat_state,
    boat_tick::{self, take_boat_events, BoatEvents},
    cow::{build_level_with, Level, Moving, Pos},
    host_out_boats::BoatEvent,
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

/// A world with the level and an oak boat at `y`, awake.
fn world_with_boat(y: f64) -> (World, Entity) {
    let mut w = World::new();
    w.insert_resource(level());
    w.insert_resource(BoatEvents::default());
    let e = boat_state::spawn(&mut w, 0, false, [0.5, y, 0.5], 0.0);
    w.entity_mut(e).insert(Moving);
    (w, e)
}

/// A boat lying on the floor leaves the moving set within two ticks: it costs nothing from then on.
#[test]
fn a_boat_at_rest_on_the_floor_is_not_moving_after_two_ticks() {
    if !have_data() {
        return;
    }
    let (mut w, e) = world_with_boat(1.0);
    boat_tick::boat_step(&mut w);
    boat_tick::boat_step(&mut w);
    assert!(w.get::<Moving>(e).is_none());
}

/// A boat dropped from ten blocks up settles and makes no fall event (`AbstractBoat.checkFallDamage` hurts nothing).
#[test]
fn a_fall_past_three_blocks_makes_no_event() {
    if !have_data() {
        return;
    }
    let (mut w, e) = world_with_boat(10.0);
    for _ in 0..200 {
        boat_tick::boat_step(&mut w);
        if w.get::<Moving>(e).is_none() {
            break;
        }
    }
    let events = take_boat_events(&mut w);
    assert!(events.iter().all(|ev| matches!(ev, BoatEvent::Paddle { .. })), "{events:?}");
}

/// A boat that has fallen under the world's floor minus 64 blocks is discarded; one above it stays.
#[test]
fn a_boat_under_the_world_is_discarded() {
    if !have_data() {
        return;
    }
    let (mut w, e) = world_with_boat(1.0);
    let void_y = w.resource::<Level>().mw.0.world.bottom_y() - 64;
    w.get_mut::<Pos>(e).unwrap().0.y = f64::from(void_y - 1);
    boat_tick::void_check(&mut w, e);
    assert!(w.get::<Pos>(e).is_none());

    let (mut w, e) = world_with_boat(1.0);
    boat_tick::void_check(&mut w, e);
    assert!(w.get::<Pos>(e).is_some());
}
