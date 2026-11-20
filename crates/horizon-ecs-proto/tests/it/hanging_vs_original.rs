//! The hanging entities (item frame, painting) against vanilla's survival timing: `BlockAttachedEntity.tick` checks `survives` on the
//! 101st tick after the birth (the jar: `ticksSinceLastCheck` is incremented, then compared with 100 before the test), and an entity
//! that stops surviving breaks there. The expected ticks come from the jar's bytecode, not from Horizon. `HORIZON_TEST_RESOURCES` set.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::block_events::BlockEvents;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::hanging::*;

fn wall_held(_: &World, _: [i32; 3], _: u8, _: (u8, u8)) -> bool {
    true
}
fn wall_gone(_: &World, _: [i32; 3], _: u8, _: (u8, u8)) -> bool {
    false
}

/// A slab with the walls held; the frame hangs on the south face of the block at (0, 60, 0), in front of it.
fn scene() -> Herd {
    let mut h = Herd::with_slab(16, true, false);
    insert_resources(&mut h.world);
    h.world.insert_resource(Walls(wall_held));
    h
}
fn clock(h: &Herd) -> u64 {
    h.world.resource::<Clock>().0
}

/// A frame whose wall is removed at the tick `cut` breaks on the 101st tick after its birth, not at the block change.
#[test]
fn a_frame_breaks_on_the_101st_tick_after_its_birth() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let born = clock(&h);
    let e = hang(&mut h.world, [0, 60, 0], 3, ITEM_FRAME, None, None);
    let support = [0, 60, -1];
    for _ in 0..4 {
        h.tick();
    }
    h.world.insert_resource(Walls(wall_gone));
    h.world.resource_mut::<BlockEvents>().changed.push(support);
    h.tick();
    h.world.resource_mut::<BlockEvents>().changed.clear();
    let mut broke_at = None;
    while broke_at.is_none() && clock(&h) < born + 300 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            broke_at = Some(clock(&h));
        }
    }
    assert_eq!(broke_at.map(|t| t - born), Some(101), "the frame breaks on the 101st tick after its birth");
}

/// An intact frame, nothing changes around it: it is alive after 1 000 ticks (no check of its own is armed).
#[test]
fn an_intact_frame_is_still_there_after_a_thousand_ticks() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = hang(&mut h.world, [0, 60, 0], 3, ITEM_FRAME, None, None);
    for _ in 0..1000 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_ok());
}

/// Two frames in one cell on one face: both are checked on the 101st tick after the birth. The first checked breaks (`discard` leaves the
/// entity lookup at once, jar `BlockAttachedEntity.tick`), so the second no longer overlaps anything and stays.
#[test]
fn two_frames_in_one_cell_the_first_breaks_on_the_101st_tick_and_the_second_stays() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let born = clock(&h);
    let a = hang(&mut h.world, [0, 60, 0], 3, ITEM_FRAME, None, None);
    let b = hang(&mut h.world, [0, 60, 0], 3, GLOW_ITEM_FRAME, None, None);
    let mut broke_at = None;
    while broke_at.is_none() && clock(&h) < born + 300 {
        h.tick();
        if h.world.get_entity(a).is_err() {
            broke_at = Some(clock(&h));
        }
    }
    assert_eq!(broke_at.map(|t| t - born), Some(101));
    assert!(h.world.get_entity(b).is_ok(), "the second frame stays once the first is gone");
}

#[test]
#[ignore = "écart connu : Horizon (frames.rs, check_interval >= 100) casse au 100e tick, le jar au 101e ; l'ECS suit le jar"]
fn a_frame_breaks_on_the_100th_tick_as_horizon_does() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let born = clock(&h);
    let a = hang(&mut h.world, [0, 60, 0], 3, ITEM_FRAME, None, None);
    for _ in 0..4 {
        h.tick();
    }
    h.world.insert_resource(Walls(wall_gone));
    h.world.resource_mut::<BlockEvents>().changed.push([0, 60, -1]);
    h.tick();
    h.world.resource_mut::<BlockEvents>().changed.clear();
    let mut broke_at = None;
    while broke_at.is_none() && clock(&h) < born + 300 {
        h.tick();
        if h.world.get_entity(a).is_err() {
            broke_at = Some(clock(&h));
        }
    }
    assert_eq!(broke_at.map(|t| t - born), Some(100));
}
