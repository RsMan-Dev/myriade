//! The cushion and the leash knot against vanilla's survival timing: `BlockAttachedEntity.tick` checks `survives` on the 101st tick after
//! the birth, and an entity that stops surviving goes there (a cushion drops, a knot goes silently). The expected ticks come from the
//! jar's bytecode, not from Horizon. `HORIZON_TEST_RESOURCES` set.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::block_events::BlockEvents;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::species::cushion::{self, CushionHost, Hazard};
use horizon_ecs_proto::species::leash_knot::{self, Fences};

fn no_hazard(_: &World, _: Vec3) -> Hazard {
    Hazard::None
}
fn can_place(_: &World, _: Vec3) -> bool {
    true
}
fn held(_: &World, _: Vec3) -> bool {
    true
}
fn gone(_: &World, _: Vec3) -> bool {
    false
}
fn fence_held(_: &World, _: [i32; 3]) -> bool {
    true
}
fn fence_gone(_: &World, _: [i32; 3]) -> bool {
    false
}

/// A slab with the support held: a cushion sits on the block at (0, 60, 0); a knot is tied to the fence in that cell.
fn scene() -> Herd {
    let mut h = Herd::with_slab(16, true, false);
    cushion::insert_resources(&mut h.world);
    leash_knot::insert_resources(&mut h.world);
    h.world.insert_resource(CushionHost { can_place, survives: held, hazard: no_hazard });
    h.world.insert_resource(Fences(fence_held));
    h
}
fn clock(h: &Herd) -> u64 {
    h.world.resource::<Clock>().0
}
/// The support of the cushion and of the knot is removed at the change: the next tick carries it to the entities.
fn remove_support(h: &mut Herd, cell: [i32; 3]) {
    h.world.insert_resource(CushionHost { can_place, survives: gone, hazard: no_hazard });
    h.world.insert_resource(Fences(fence_gone));
    h.world.resource_mut::<BlockEvents>().changed.push(cell);
    h.tick();
    h.world.resource_mut::<BlockEvents>().changed.clear();
}

/// A cushion whose support is removed breaks on the 101st tick after its birth, not at the block change.
#[test]
fn a_cushion_breaks_on_the_101st_tick_after_its_birth() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let born = clock(&h);
    let e = cushion::place(&mut h.world, [0, 60, 0], Vec3::new(0.5, 61.0, 0.5), 14, 0.0, None).expect("placed");
    for _ in 0..4 {
        h.tick();
    }
    remove_support(&mut h, [0, 60, 0]);
    let mut broke_at = None;
    while broke_at.is_none() && clock(&h) < born + 300 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            broke_at = Some(clock(&h));
        }
    }
    assert_eq!(broke_at.map(|t| t - born), Some(101), "the cushion breaks on the 101st tick after its birth");
}

/// An intact cushion, nothing changes under it: it is still there after 1 000 ticks (no check of its own is armed).
#[test]
fn an_intact_cushion_is_still_there_after_a_thousand_ticks() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = cushion::place(&mut h.world, [0, 60, 0], Vec3::new(0.5, 61.0, 0.5), 14, 0.0, None).expect("placed");
    for _ in 0..1000 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_ok());
}

/// A knot whose fence is removed is untied on the 101st tick after its birth, without a drop.
#[test]
fn a_knot_is_untied_on_the_101st_tick_after_its_birth() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let born = clock(&h);
    let e = leash_knot::tie(&mut h.world, [0, 60, 0]);
    for _ in 0..4 {
        h.tick();
    }
    remove_support(&mut h, [0, 60, 0]);
    let mut untied_at = None;
    while untied_at.is_none() && clock(&h) < born + 300 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            untied_at = Some(clock(&h));
        }
    }
    assert_eq!(untied_at.map(|t| t - born), Some(101), "the knot is untied on the 101st tick after its birth");
}

/// A knot that no leash holds any more is discarded at once, with no check to wait for.
#[test]
fn a_released_knot_without_leashes_is_discarded_at_once() {
    if !have_data() {
        return;
    }
    let mut h = scene();
    let e = leash_knot::tie(&mut h.world, [0, 60, 0]);
    leash_knot::released(&mut h.world, [0, 60, 0]);
    assert!(h.world.get_entity(e).is_err());
}
