//! The falling block against the jar's `FallingBlockEntity.tick` (read with javap: `applyGravity`, `move`, the drag 0.98, the landing on the
//! tick the move ends on the ground, `canBeReplaced` and `FallingBlock.isFree` below). The reference is the naive recurrence of that tick, run
//! every tick on a free fall (no Horizon original of the entity is callable: it lives in `Game`). `HORIZON_TEST_RESOURCES` set.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::falling_block::{fall, FallingBlock};
use horizon_world::registry::BlockStateId;

/// The first solid cell under `(x, z)` from `from` down, as its top.
fn floor_under(h: &Herd, x: i32, z: i32, from: i32) -> i32 {
    let mw = h.world.resource::<Level>().mw.0;
    (-64..=from).rev().find(|&y| !mw.facts.is_air(mw.world.block(x, y, z))).map_or(0, |y| y + 1)
}

/// `FallingBlockEntity.tick`, the free fall: `applyGravity` (0.04), the move (stopped by the floor), then `getAirDrag` (a float 0.98). The tick at
/// which the move ends on the floor.
fn reference(start_y: f64, floor: f64) -> u64 {
    let (mut y, mut vy) = (start_y, 0.0_f64);
    for t in 1..=400 {
        vy -= 0.04;
        if y + vy <= floor {
            return t;
        }
        y += vy;
        vy *= f64::from(0.98_f32);
    }
    panic!("never lands");
}

fn stone(h: &Herd) -> BlockStateId {
    let mw = h.world.resource::<Level>().mw.0;
    horizon_ecs_proto::blocks::resolve(mw, "minecraft:sand", &[]).expect("sand")
}

/// A block dropped from above lands on the tick of the reference, and is a block again at the cell it stands in.
#[test]
fn a_falling_block_lands_on_the_reference_tick_and_is_placed() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    let cell = [0, top + 10, 0];
    let sand = stone(&h);
    let e = fall(&mut h.world, cell, sand, None);
    let start = h.world.get::<Pos>(e).unwrap().0.y;
    let want = reference(start, f64::from(top));
    let mut landed = None;
    for t in 1..=400u64 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            landed = Some(t);
            break;
        }
    }
    assert_eq!(landed, Some(want), "the landing tick");
    h.tick();
    let mw = h.world.resource::<Level>().mw.0;
    assert_eq!(mw.world.block(0, top, 0), stone(&h), "the block is placed on the floor");
    assert!(h.world.query::<&FallingBlock>().iter(&h.world).next().is_none());
}

/// Once landed nothing is left to visit: the entity is gone, and no block in the air stays awake.
#[test]
fn a_landed_block_leaves_nothing_awake() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    let sand = stone(&h);
    fall(&mut h.world, [0, top + 3, 0], sand, None);
    for _ in 0..60 {
        h.tick();
    }
    assert_eq!(h.world.query::<&Moving>().iter(&h.world).count(), 0, "nothing moves once the block landed");
}
