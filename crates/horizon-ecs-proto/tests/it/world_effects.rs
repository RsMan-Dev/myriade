//! The world's effects on the creatures the ECS does not yet have as a player sees them: a piston pushes a cow out of its box
//! (`world_push::piston_push`), a sleeping cow it reaches wakes and moves, a wall stops it, a slime block bounces it, a retracting
//! piston pulls it, a cactus hurts a cow standing in it each tick, and a column of bubbles lifts it. The expected displacements are
//! the `horizon_world` kernels called with the same inputs (`collided_amount`, the 0.51 limit of `PistonMovement`, `bounce_velocity`).
use crate::common::*;
use bevy_ecs::prelude::Entity;
use horizon_core::{
    geometry::{Aabb, Direction, Vec3},
    position::BlockPos,
};
use horizon_ecs_proto::{
    cow::{Bounds, Clock, Contact, Herd, Pos, Vel},
    world_push::{piston_push, PushBlock},
    zone,
};
use horizon_world::{
    level::data::PushReaction,
    piston_movement::PistonMovement,
    piston_push::{bounce_velocity, collided_amount, sticky_area, sticky_entity_selected, step_vector},
    piston_shape::MovingPistonState,
    registry::BlockStateId,
};

/// The piston's block: the push is at this position.
const HERE: BlockPos = BlockPos { x: 0, y: 1, z: 0 };

/// A piston of one block, from progress 0 to 0.5 in one tick, its moved block one full cube.
fn piston(direction: Direction, extending: bool) -> MovingPistonState {
    MovingPistonState { moved_state: BlockStateId(0), direction, extending, source: true, progress: 0.0, previous_progress: 0.0 }
}

fn cube() -> [Aabb; 1] {
    [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))]
}

/// One push of `piston` along `movement`, the moved block a cube (a slime one when `slime`).
fn push(h: &mut Herd, piston: MovingPistonState, movement: Direction, slime: bool) {
    let boxes = cube();
    let block = PushBlock { boxes: &boxes, slime, honey: false, sticky_top: 0.0 };
    piston_push(&mut h.world, HERE, piston, 0.5, movement, &block);
}

/// The displacement the kernels give the cow's box for that push, alone in its tick: the collided amount, then the 0.51 limit.
fn kernel_move(h: &Herd, e: Entity, piston: MovingPistonState, movement: Direction) -> Vec3 {
    let bounds = h.world.get::<Bounds>(e).unwrap().0;
    let amount = collided_amount(&cube(), HERE, piston, 0.5, movement, bounds);
    PistonMovement::default().limit(step_vector(movement).scale(amount), || Ok::<i64, ()>(0)).unwrap()
}

#[test]
fn a_piston_pushes_a_cow_by_the_kernel_amount() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 1);
    let want = kernel_move(&h, e, piston(Direction::East, true), Direction::East);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    push(&mut h, piston(Direction::East, true), Direction::East, false);
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    assert!(want.x > 0.0, "the kernel does not push the cow: {}", want.x);
    assert!((x1 - x0 - want.x).abs() < 1e-9, "the cow moved {} where the kernel gives {}", x1 - x0, want.x);
}

#[test]
fn a_piston_wakes_a_sleeping_cow_it_reaches_and_moves_it() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 2);
    let want = kernel_move(&h, e, piston(Direction::East, true), Direction::East);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    let now = h.world.resource::<Clock>().0;
    zone::sleep(&mut h.world, e, now);
    assert!(h.world.get::<zone::Dormant>(e).is_some(), "the cow did not sleep");
    push(&mut h, piston(Direction::East, true), Direction::East, false);
    assert!(h.world.get::<zone::Dormant>(e).is_none(), "the piston did not wake the sleeping cow");
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    assert!((x1 - x0 - want.x).abs() < 1e-9, "the woken cow moved {} where the kernel gives {}", x1 - x0, want.x);
}

#[test]
fn a_wall_stops_a_cow_the_piston_pushes() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    // A stone wall at x = 2 (its face at 2.0). The blocks are placed at the next step.
    h.set_block(2, 1, 0, "minecraft:stone");
    h.set_block(2, 2, 0, "minecraft:stone");
    h.tick();
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 4);
    for _ in 0..4 {
        push(&mut h, piston(Direction::East, true), Direction::East, false);
    }
    let face = h.world.get::<Bounds>(e).unwrap().0.max_position().x;
    assert!(face <= 2.0 + 1e-6, "the cow went through the wall: its east face is at {face}");
}

#[test]
fn a_slime_block_bounces_a_cow_it_pushes_along_the_move() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = ecs_cow(&mut h, 1.1, 1.0, 0.5, 5);
    let before = h.world.get::<Vel>(e).unwrap().0;
    push(&mut h, piston(Direction::East, true), Direction::East, true);
    let after = h.world.get::<Vel>(e).unwrap().0;
    let want = bounce_velocity(Direction::East, before);
    assert_eq!((after.x, after.y, after.z), (want.x, want.y, want.z), "the slime did not bounce the cow as the kernel gives");
}

#[test]
fn a_retracting_piston_pulls_a_cow_by_the_kernel_amount() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    // The cow is west of the block: a retraction moves the block west, into it.
    let e = ecs_cow(&mut h, -0.3, 1.0, 0.5, 6);
    let want = kernel_move(&h, e, piston(Direction::East, false), Direction::West);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    push(&mut h, piston(Direction::East, false), Direction::West, false);
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    assert!(want.x < 0.0, "the kernel does not retract the cow: {}", want.x);
    assert!((x1 - x0 - want.x).abs() < 1e-9, "the cow moved {} where the kernel gives {}", x1 - x0, want.x);
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
    // Water under a bubble column: an upward column (`drag=false`, the bubbles rise from a water source).
    h.set_block(0, 1, 0, "minecraft:water");
    h.set_block_state(0, 2, 0, "minecraft:bubble_column", &[("drag", "false")]);
    let e = ecs_cow(&mut h, 0.5, 2.0, 0.5, 6);
    for _ in 0..10 {
        h.tick();
    }
    let y = h.world.get::<Pos>(e).unwrap().0.y;
    assert!(y > 2.0, "the bubbles did not lift the cow: y = {y}");
}

#[test]
fn a_downward_column_of_bubbles_sinks_a_cow_faster_than_no_column() {
    // Two identical herds, a cow falling from the same height; one has a downward column (`drag=true`) in its path.
    let mut column = Herd::new_awake(0, 6.0, true, false, 3);
    column.set_block_state(0, 2, 0, "minecraft:bubble_column", &[("drag", "true")]);
    column.tick();
    let mut free = Herd::new_awake(0, 6.0, true, false, 3);
    let (c, f) = (ecs_cow(&mut column, 0.5, 3.0, 0.5, 6), ecs_cow(&mut free, 0.5, 3.0, 0.5, 6));
    for _ in 0..3 {
        column.tick();
        free.tick();
    }
    let (yc, yf) = (column.world.get::<Pos>(c).unwrap().0.y, free.world.get::<Pos>(f).unwrap().0.y);
    assert!(yc < yf - 1e-6, "the downward column did not sink the cow: {yc} against {yf} without it");
}

/// The honey block at the piston's place, its moved box the honey shape: the top at 0.9375, the sticky top the same.
fn honey_push(h: &mut Herd, movement: Direction) {
    let boxes = [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.9375, 1.0))];
    let block = PushBlock { boxes: &boxes, slime: false, honey: true, sticky_top: 0.9375 };
    piston_push(&mut h.world, HERE, piston(Direction::East, true), 0.5, movement, &block);
}

#[test]
fn honey_pulls_a_cow_standing_on_it_by_the_kernel_amount() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    h.set_block(0, 1, 0, "minecraft:honey_block");
    h.tick();
    // On the honey's top (1 + 0.9375), over the block: supported by the piston's place.
    let e = ecs_cow(&mut h, 0.5, 1.9375, 0.5, 7);
    let at = pin(&h, e);
    tick_pinned(&mut h, e, &at);
    let (on_ground, bounds, pos) = (h.world.get::<Contact>(e).unwrap().on_ground, h.world.get::<Bounds>(e).unwrap().0, h.world.get::<Pos>(e).unwrap().0);
    assert!(on_ground, "the cow did not come to rest on the honey");
    let sticky = sticky_area(HERE, piston(Direction::East, true), 0.9375);
    assert!(sticky_entity_selected(sticky, bounds, PushReaction::PushPull, on_ground, true, pos), "the kernel does not select the cow on honey");
    let want = PistonMovement::default().limit(step_vector(Direction::East).scale(0.5), || Ok::<i64, ()>(0)).unwrap();
    honey_push(&mut h, Direction::East);
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    assert!((x1 - pos.x - want.x).abs() < 1e-9, "the honey moved the cow {} where the kernel gives {}", x1 - pos.x, want.x);
}

#[test]
fn honey_leaves_a_cow_standing_beside_it() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    h.set_block(0, 1, 0, "minecraft:honey_block");
    h.tick();
    // On the ground, three blocks away: not supported by the piston, and outside the sticky area.
    let e = ecs_cow(&mut h, 3.0, 1.0, 0.5, 8);
    let at = pin(&h, e);
    tick_pinned(&mut h, e, &at);
    let (on_ground, bounds, pos) = (h.world.get::<Contact>(e).unwrap().on_ground, h.world.get::<Bounds>(e).unwrap().0, h.world.get::<Pos>(e).unwrap().0);
    let sticky = sticky_area(HERE, piston(Direction::East, true), 0.9375);
    assert!(!sticky_entity_selected(sticky, bounds, PushReaction::PushPull, on_ground, false, pos), "the kernel selects a cow beside honey");
    let x0 = pos.x;
    honey_push(&mut h, Direction::East);
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    assert!((x1 - x0).abs() < 1e-9, "the honey moved a cow it does not stick: {} -> {x1}", x0);
}
