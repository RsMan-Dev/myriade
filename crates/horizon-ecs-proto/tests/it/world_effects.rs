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

/// A cow in the spatial index from its creation, as a spawned one is (the pushes look it up there).
fn indexed_cow(h: &mut Herd, x: f64, y: f64, z: f64, seed: u64) -> Entity {
    let e = ecs_cow(h, x, y, z, seed);
    horizon_ecs_proto::cow::enter_index(&mut h.world, e);
    e
}

/// A retracting piston that is not the source: the one that drags nothing out of its own base.
fn retracting() -> MovingPistonState {
    MovingPistonState { source: false, ..piston(Direction::East, false) }
}

fn cube() -> [Aabb; 1] {
    [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))]
}

/// One push of `piston` along `movement`, the moved block a cube (a slime one when `slime`).
fn push(h: &mut Herd, piston: MovingPistonState, movement: Direction, slime: bool) {
    let boxes = cube();
    let block = PushBlock { boxes: &boxes, slime, honey: false, sticky_top: 0.0 };
    piston_push(&mut h.world, HERE, piston, 0.5, movement, &block, None);
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
    let e = indexed_cow(&mut h, 0.1, 1.0, 0.5, 1);
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
    let e = indexed_cow(&mut h, 0.1, 1.0, 0.5, 2);
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
    // As high as the tallest species (the giant is 12 blocks) and as wide as the widest (the ghast is 4).
    // The cow's back edge sits in the slab the piston sweeps (x 0.0 to 0.5), its front edge is shorter of the wall (x = 1) than the push.
    for y in 1..=14 {
        for z in -3..=3 {
            h.set_block(2, y, z, "minecraft:stone");
        }
    }
    h.tick();
    let e = indexed_cow(&mut h, 1.55 - f64::from(horizon_ecs_proto::cow::species().width) / 2.0, 1.0, 0.5, 4);
    for _ in 0..4 {
        push(&mut h, piston(Direction::East, true), Direction::East, false);
    }
    let face = h.world.get::<Bounds>(e).unwrap().0.max_position().x;
    assert!(face <= 2.0 + 1e-6, "the cow went through the wall: its east face is at {face}");
}

#[test]
fn a_slime_block_bounces_a_cow_it_pushes_along_the_move() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = indexed_cow(&mut h, 0.1, 1.0, 0.5, 5);
    let before = h.world.get::<Vel>(e).unwrap().0;
    push(&mut h, piston(Direction::East, true), Direction::East, true);
    let after = h.world.get::<Vel>(e).unwrap().0;
    let want = bounce_velocity(Direction::East, before);
    assert_eq!((after.x, after.y, after.z), (want.x, want.y, want.z), "the slime did not bounce the cow as the kernel gives");
}

#[test]
fn a_retracting_piston_pulls_a_cow_by_the_kernel_amount() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    // The retracting block sits at 0..1 and moves west: a cow over its west half is pulled along.
    let e = indexed_cow(&mut h, 1.0, 1.0, 0.5, 6);
    let want = kernel_move(&h, e, retracting(), Direction::West);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    push(&mut h, retracting(), Direction::West, false);
    let x1 = h.world.get::<Pos>(e).unwrap().0.x;
    assert!(want.x < 0.0, "the kernel does not retract the cow: {}", want.x);
    assert!((x1 - x0 - want.x).abs() < 1e-9, "the cow moved {} where the kernel gives {}", x1 - x0, want.x);
}

#[test]
fn a_cactus_hurts_a_cow_standing_in_it_each_tick() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    h.set_block(0, 1, 0, "minecraft:cactus");
    let e = indexed_cow(&mut h, 0.5, 1.0, 0.5, 3);
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
    if !ground_walker() {
        return;
    }
    // Water under a bubble column: an upward column (`drag=false`, the bubbles rise from a water source).
    h.set_block(0, 1, 0, "minecraft:water");
    h.set_block_state(0, 2, 0, "minecraft:bubble_column", &[("drag", "false")]);
    let e = indexed_cow(&mut h, 0.5, 2.0, 0.5, 6);
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
    if !ground_walker() {
        return;
    }
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

/// The honey block at the piston's place (the moving block at progress 0 is one cell west of it: the cow stands on the part over HERE), its moved box the honey shape: the top at 0.9375, the sticky top the same.
fn honey_push(h: &mut Herd, movement: Direction) {
    let boxes = [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.9375, 1.0))];
    let block = PushBlock { boxes: &boxes, slime: false, honey: true, sticky_top: 0.9375 };
    piston_push(&mut h.world, HERE, piston(Direction::East, true), 0.5, movement, &block, None);
}

#[test]
fn honey_pulls_a_cow_standing_on_it_by_the_kernel_amount() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    if !ground_walker() {
        return;
    }
    h.set_block(0, 1, 0, "minecraft:honey_block");
    h.tick();
    // On the honey's top (1 + 0.9375), over the block: supported by the piston's place.
    let e = indexed_cow(&mut h, -0.1, 1.9375, 0.5, 7);
    h.world.get_mut::<Vel>(e).unwrap().0.y = -0.08 * f64::from(0.98_f32);
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
    let e = indexed_cow(&mut h, 3.0, 1.0, 0.5, 8);
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

#[test]
fn a_cow_loses_its_velocity_against_the_wall_a_piston_pushes_it_into() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    for y in 1..=14 {
        for z in -3..=3 {
            h.set_block(1, y, z, "minecraft:stone");
        }
    }
    h.tick();
    let e = indexed_cow(&mut h, 0.05 + f64::from(horizon_ecs_proto::cow::species().width) / 2.0, 1.0, 0.5, 20);
    h.world.get_mut::<Vel>(e).unwrap().0 = Vec3::new(0.3, 0.0, 0.2);
    for _ in 0..4 {
        push(&mut h, piston(Direction::East, true), Direction::East, false);
    }
    let v = h.world.get::<Vel>(e).unwrap().0;
    assert_eq!(v.x, 0.0, "the wall left the cow its east velocity");
    assert_eq!(v.z, 0.2, "an axis the push did not cut lost its velocity");
}

#[test]
fn a_cow_is_pushed_at_most_half_a_block_a_tick_per_axis() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = indexed_cow(&mut h, 0.1, 1.0, 0.5, 21);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    for _ in 0..3 {
        push(&mut h, piston(Direction::East, true), Direction::East, false);
    }
    let moved = h.world.get::<Pos>(e).unwrap().0.x - x0;
    assert!(moved > 0.0 && moved <= 0.51 + 1e-9, "three pushes in one tick moved the cow {moved}, the limit is 0.51");
}

#[test]
fn a_sideways_push_clears_the_cows_ground_flag() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = indexed_cow(&mut h, 0.1, 1.0, 0.5, 22);
    h.world.get_mut::<Contact>(e).unwrap().on_ground = true;
    push(&mut h, piston(Direction::East, true), Direction::East, false);
    assert!(!h.world.get::<Contact>(e).unwrap().on_ground, "Entity.move by a sideways push sets the flag from the downward part only");
}

#[test]
fn a_piston_leaves_an_ignore_entity_creature_where_it_is() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = indexed_cow(&mut h, 0.1, 1.0, 0.5, 23);
    h.world.entity_mut(e).insert(horizon_ecs_proto::world_push::PistonIgnored);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    push(&mut h, piston(Direction::East, true), Direction::East, false);
    assert_eq!(h.world.get::<Pos>(e).unwrap().0.x, x0, "an IGNORE_ENTITY creature was moved by the piston");
}

/// One push with a mod's gate: it answers `answer` to the asked displacement; returns how many times it was asked.
fn gated_push(h: &mut Herd, answer: impl Fn(Vec3) -> Option<Vec3>) -> u32 {
    let boxes = cube();
    let block = PushBlock { boxes: &boxes, slime: false, honey: false, sticky_top: 0.0 };
    let (mut calls, mut next) = (0_u32, 100_u32);
    let mut alloc = || {
        next += 1;
        next
    };
    let mut arm = |_id: u32, _kind: &str, want: Vec3| {
        calls += 1;
        answer(want)
    };
    let gate = horizon_ecs_proto::world_push::PushGate { alloc: &mut alloc, arm: &mut arm };
    piston_push(&mut h.world, HERE, piston(Direction::East, true), 0.5, Direction::East, &block, Some(gate));
    calls
}

#[test]
fn a_mod_cancelling_the_move_keeps_the_cow_still() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = indexed_cow(&mut h, 0.1, 1.0, 0.5, 24);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    let calls = gated_push(&mut h, |_| None);
    assert!(calls >= 1, "the gate was not asked");
    assert_eq!(h.world.get::<Pos>(e).unwrap().0.x, x0, "a cancelled move still moved the cow");
}

#[test]
fn a_mod_replacing_the_move_changes_the_push() {
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let e = indexed_cow(&mut h, 0.1, 1.0, 0.5, 25);
    let x0 = h.world.get::<Pos>(e).unwrap().0.x;
    gated_push(&mut h, |want| Some(Vec3::new(want.x * 0.5, want.y, want.z)));
    let half = h.world.get::<Pos>(e).unwrap().0.x - x0;
    let mut free = Herd::new_awake(0, 6.0, true, false, 3);
    let f = indexed_cow(&mut free, 0.1, 1.0, 0.5, 25);
    push(&mut free, piston(Direction::East, true), Direction::East, false);
    let whole = free.world.get::<Pos>(f).unwrap().0.x - x0;
    assert!(half > 0.0 && (half - whole * 0.5).abs() < 1e-6, "the replaced move gave {half}, half of the plain push {whole}");
}
