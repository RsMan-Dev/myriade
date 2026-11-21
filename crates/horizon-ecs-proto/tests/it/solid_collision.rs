//! `world_push::solid_boxes`: which entities stop a mover the level does not hold (a shulker, a happy ghast, a boat).
use crate::common::*;
use horizon_ecs_proto::cow::Herd;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::world_push::{solid_boxes, Collidable, CollidableWhenStill, Mover};

fn sweep_around(x: f64, y: f64, z: f64) -> Aabb {
    Aabb::new(Vec3::new(x - 3.0, y - 3.0, z - 3.0), Vec3::new(x + 3.0, y + 3.0, z + 3.0))
}

fn indexed(h: &mut Herd) -> bevy_ecs::prelude::Entity {
    let e = ecs_cow(h, 0.0, 64.0, 0.0, 1);
    horizon_ecs_proto::cow::enter_index(&mut h.world, e);
    e
}

#[test]
fn a_living_collidable_stops_any_mover() {
    let mut h = Herd::new_awake(0, 1.0, false, false, 1);
    let e = indexed(&mut h);
    h.world.entity_mut(e).insert(Collidable);
    assert_eq!(solid_boxes(&h.world, sweep_around(0.0, 64.0, 0.0), Mover::Other).len(), 1);
}

#[test]
fn an_unmarked_creature_stops_nobody() {
    let mut h = Herd::new_awake(0, 1.0, false, false, 1);
    indexed(&mut h);
    assert!(solid_boxes(&h.world, sweep_around(0.0, 64.0, 0.0), Mover::Other).is_empty());
}

#[test]
fn a_moving_ghast_is_solid_only_to_a_player_at_its_top() {
    let mut h = Herd::new_awake(0, 1.0, false, false, 1);
    let e = indexed(&mut h);
    h.world.entity_mut(e).insert(CollidableWhenStill);
    let sweep = sweep_around(0.0, 64.0, 0.0);
    assert!(solid_boxes(&h.world, sweep, Mover::Other).is_empty());
    assert_eq!(solid_boxes(&h.world, sweep, Mover::Player { y: 1000.0, vehicle: None }).len(), 1);
    assert!(solid_boxes(&h.world, sweep, Mover::Player { y: -1000.0, vehicle: None }).is_empty());
}

#[test]
fn a_player_riding_the_ghast_is_not_stopped_by_it() {
    let mut h = Herd::new_awake(0, 1.0, false, false, 1);
    let e = indexed(&mut h);
    h.world.entity_mut(e).insert(CollidableWhenStill);
    let sweep = sweep_around(0.0, 64.0, 0.0);
    assert!(solid_boxes(&h.world, sweep, Mover::Player { y: 1000.0, vehicle: Some(e) }).is_empty());
}
