//! The world's effects on the creatures the ECS does not yet have as a player sees them: a piston pushes a cow out of its box
//! (`world_push::piston_push`), a sleeping cow it reaches wakes and moves, a wall stops it, a slime block bounces it, a cactus
//! hurts a cow standing in it each tick, and a column of bubbles lifts it. Expected values are the vanilla rules written out
//! here (the 0.51 limit, the bounce's unit velocity), not read back from the code under test.
use crate::common::*;
use horizon_core::{
    geometry::{Aabb, Direction, Vec3},
    position::BlockPos,
};
use horizon_ecs_proto::{
    cow::{Clock, Herd, Pos, Vel},
    world_push::{piston_push, PushBlock},
    zone,
};
use horizon_world::{piston_shape::MovingPistonState, registry::BlockStateId};

/// A piston extending east, from progress 0 to 0.5 (half a block) in one tick, its moved block one full cube.
fn push_east(h: &mut Herd, pos: BlockPos, slime: bool) {
    let piston = MovingPistonState {
        moved_state: BlockStateId(0),
        direction: Direction::East,
        extending: true,
        source: true,
        progress: 0.0,
        previous_progress: 0.0,
    };
    let cube = [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))];
    let block = PushBlock { boxes: &cube, slime, honey: false, sticky_top: 0.0 };
    piston_push(&mut h.world, pos, piston, 0.5, Direction::East, &block);
}

#[test]
fn a_piston_pushes_a_cow_east_by_at_most_half_a_block() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 1);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    push_east(&mut h, BlockPos { x: 0, y: 1, z: 0 }, false);
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    // Pushed (the box is in the moved block), and no further than the 0.51 a tick the piston allows per axis.
    assert!(x1 > x0, "the piston did not push the cow: {x0} -> {x1}");
    assert!(x1 - x0 <= 0.51 + 1e-6, "the piston pushed the cow more than 0.51: {x0} -> {x1}");
}

#[test]
fn a_piston_wakes_a_sleeping_cow_it_reaches_and_moves_it() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 2);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    let now = h.world.resource::<Clock>().0;
    zone::sleep(&mut h.world, e, now);
    assert!(h.world.get::<zone::Dormant>(e).is_some(), "the cow did not sleep");
    push_east(&mut h, BlockPos { x: 0, y: 1, z: 0 }, false);
    assert!(h.world.get::<zone::Dormant>(e).is_none(), "the piston did not wake the sleeping cow");
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    assert!(x1 > x0 && x1 - x0 <= 0.51 + 1e-6, "the woken cow was not moved: {x0} -> {x1}");
}

#[test]
fn a_wall_stops_a_cow_the_piston_pushes() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    // A stone wall at x = 2 (its face at 2.0): the cow, half a block wide, must stay on the near side of it.
    h.set_block(2, 1, 0, "minecraft:stone");
    h.set_block(2, 2, 0, "minecraft:stone");
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 4);
    for _ in 0..4 {
        push_east(&mut h, BlockPos { x: 0, y: 1, z: 0 }, false);
    }
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    assert!(x1 < 2.0, "the cow went through the wall: x = {x1}");
}

#[test]
fn a_slime_block_bounces_a_cow_it_pushes_along_the_move() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 5);
    push_east(&mut h, BlockPos { x: 0, y: 1, z: 0 }, true);
    // `bounce_velocity`: the velocity along the move is the unit step of the move's direction.
    assert_eq!(h.world.get::<Vel>(e).unwrap().0.x, 1.0, "the slime did not bounce the cow");
}

#[test]
fn a_cactus_hurts_a_cow_standing_in_it_each_tick() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    h.set_block(0, 1, 0, "minecraft:cactus");
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 3);
    let at = pin(&h, e);
    let before = health(&h, e);
    for _ in 0..20 {
        tick_pinned(&mut h, e, &at);
    }
    assert!(health(&h, e) < before, "the cactus did not hurt the cow: {before} -> {}", health(&h, e));
}

#[test]
fn a_column_of_bubbles_lifts_a_cow_standing_in_it() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    // Water under a bubble column: an upward column (the bubbles rise from a water source).
    h.set_block(0, 1, 0, "minecraft:water");
    h.set_block(0, 2, 0, "minecraft:bubble_column");
    let e = ecs_cow(&mut h, 0.5, 2.0, 0.5, 6);
    for _ in 0..10 {
        h.tick();
    }
    let y = h.world.get::<Pos>(e).unwrap().0.y;
    assert!(y > 2.0, "the bubbles did not lift the cow: y = {y}");
}
