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
    let mut h = Herd::with_slab_awake(16, true, false);
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
    let mut h = Herd::with_slab_awake(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    let sand = stone(&h);
    fall(&mut h.world, [0, top + 3, 0], sand, None);
    for _ in 0..60 {
        h.tick();
    }
    assert_eq!(h.world.query::<&Moving>().iter(&h.world).count(), 0, "nothing moves once the block landed");
}

/// A block that stays in the air (no gravity here) is removed on its 601st tick (`age > 600`) and drops its item; one that landed is not touched.
#[test]
fn a_block_still_in_the_air_after_600_ticks_is_removed_and_drops_its_item() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    let sand = stone(&h);
    let item = horizon_client_lib::items::ItemStack::new("minecraft:sand", 1);
    let e = fall(&mut h.world, [0, top + 10, 0], sand, Some(item));
    h.world.get_mut::<Body>(e).unwrap().gravity = 0.0;
    let mut gone = None;
    for t in 1..=700u64 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            gone = Some(t);
            break;
        }
    }
    assert_eq!(gone, Some(601), "removed on its 601st tick");
    h.tick();
    assert_eq!(h.world.query::<&horizon_ecs_proto::item::Item>().iter(&h.world).count(), 1, "its item is dropped");
}

/// Concrete powder that lands in water sets as concrete, not as powder.
#[test]
fn concrete_powder_landing_in_water_sets_as_concrete() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    h.set_block(0, top, 0, "minecraft:water");
    h.tick();
    let mw = h.world.resource::<Level>().mw.0;
    let powder = horizon_ecs_proto::blocks::resolve(mw, "minecraft:white_concrete_powder", &[]).unwrap();
    fall(&mut h.world, [0, top + 4, 0], powder, None);
    for _ in 0..80 {
        h.tick();
    }
    let concrete = horizon_ecs_proto::blocks::resolve(mw, "minecraft:white_concrete", &[]).unwrap();
    let placed = (top - 1..=top + 4).map(|y| mw.world.block(0, y, 0)).collect::<Vec<_>>();
    assert!(placed.contains(&concrete), "concrete where the powder landed: {placed:?}");
    assert!(!placed.contains(&powder), "no powder left");
}

/// The golden of Horizon's own falling block (`falling_golden_tests.rs` in `horizon-client`, 9 blocks onto a floor): the ECS block is at the
/// same height after every tick, and lands on the same tick.
#[test]
fn a_falling_block_follows_horizons_own_golden_tick_by_tick() {
    if !have_data() {
        return;
    }
    let golden = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/falling_free_fall.txt")).expect("the golden of Horizon's falling block");
    let mut h = Herd::with_slab_awake(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    let sand = stone(&h);
    let e = fall(&mut h.world, [0, top + 9, 0], sand, None);
    let y0 = f64::from(top) + 9.0;
    h.world.get_mut::<Pos>(e).unwrap().0.y = y0;
    h.world.get_mut::<Bounds>(e).unwrap().0 = box_at(horizon_core::geometry::Vec3::new(0.5, y0, 0.5), 0.98, 0.98);
    for line in golden.lines() {
        h.tick();
        let mut it = line.split(' ');
        let (t, y) = (it.next().unwrap(), it.next().unwrap());
        if y == "landed" {
            assert!(h.world.get_entity(e).is_err(), "it lands on tick {t}");
        } else {
            let at = h.world.get::<Pos>(e).unwrap_or_else(|| panic!("landed before tick {t}")).0.y;
            let want = y.parse::<f64>().unwrap() - 61.0 + f64::from(top);
            assert!((at - want).abs() < 1e-9, "tick {t}: ecs {at}, Horizon {want}");
        }
    }
}
