//! The sensitive cells an entity touches (`inside.rs`): a creature entering a pressure plate's cell reports it once, one at rest reports
//! nothing, and the zone queries are exact at the edges of a box (a shared face is not a contact). Needs `insert_resources` and `register`
//! wired in the schedule, and the plate placed by `set_block`.
use crate::common;
use bevy_ecs::prelude::Entity;
use common::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::host_out_inside::InsideOut;
use horizon_ecs_proto::inside::{triggers_in, Sensitive};
use std::sync::Arc;

const PLATE: &str = "minecraft:stone_pressure_plate";

/// A world with a plate at (5, 1, 0), the table armed on its state, and a cow at (0.5, 1, 0.5) that has settled.
fn plate_and_cow() -> (Herd, Entity) {
    let (_, mut h) = flat_pair();
    h.set_block(5, 1, 0, PLATE);
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 4100);
    let state = h.world.resource::<Level>().mw.0.registry.default_state(PLATE).expect("the plate").0 as usize;
    let mut bits = vec![0u64; state / 64 + 1];
    bits[state / 64] |= 1 << (state % 64);
    h.world.resource_mut::<Sensitive>().0 = Some(Arc::from(bits));
    h.tick();
    h.tick();
    h.world.resource_mut::<InsideOut>().0.clear();
    (h, e)
}

fn move_cow(h: &mut Herd, e: Entity, x: f64) {
    h.world.get_mut::<Pos>(e).unwrap().0.x = x;
    h.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(x, 1.0, 0.5), 0.9, 1.4);
}

#[test]
fn a_creature_entering_a_plate_reports_it_once() {
    if !have_data() {
        return;
    }
    let (mut h, e) = plate_and_cow();
    move_cow(&mut h, e, 5.5);
    h.tick();
    let out = std::mem::take(&mut h.world.resource_mut::<InsideOut>().0);
    assert_eq!(out.len(), 1, "one event for the move: {out:?}");
    assert_eq!((out[0].entity, out[0].cells.as_slice()), (e, &[[5, 1, 0]][..]));
}

#[test]
fn a_creature_at_rest_reports_nothing() {
    if !have_data() {
        return;
    }
    let (mut h, e) = plate_and_cow();
    move_cow(&mut h, e, 5.5);
    h.tick();
    h.world.resource_mut::<InsideOut>().0.clear();
    for _ in 0..5 {
        h.tick();
    }
    assert!(h.world.resource::<InsideOut>().0.is_empty());
    // Away from the plate: a move reports nothing either (no sensitive cell under its box).
    move_cow(&mut h, e, 2.5);
    h.tick();
    assert!(h.world.resource::<InsideOut>().0.is_empty());
}

#[test]
fn a_zone_query_is_exact_at_the_edges() {
    if !have_data() {
        return;
    }
    let (mut h, e) = plate_and_cow();
    move_cow(&mut h, e, 5.5);
    h.tick();
    let (lo, hi) = {
        let b = h.world.get::<Bounds>(e).unwrap().0;
        (b.min_position(), b.max_position())
    };
    let zone = |x0: f64, x1: f64| Aabb::new(Vec3::new(x0, 0.0, 0.0), Vec3::new(x1, 3.0, 1.0));
    let n = |a: Aabb, mobs| triggers_in(&h.world, a, mobs).len();
    assert_eq!(n(zone(hi.x - 0.01, hi.x + 1.0), true), 1, "overlapping by 0.01");
    assert_eq!(n(zone(hi.x, hi.x + 1.0), true), 0, "touching the face only");
    assert_eq!(n(zone(lo.x - 1.0, lo.x), true), 0, "touching the other face only");
    assert_eq!(n(zone(lo.x - 1.0, lo.x + 0.01), false), 1, "everything, overlapping");
}
