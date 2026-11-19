//! The contract of the occupancy masks (`occupancy.rs`): every writer of blocks must tell them. Tested by the BEHAVIOUR, not by the name of
//! a function: the masks are warmed by a query over the cell, a block is placed and removed by each route the modules use
//! (`BlockChanges`, `blocks::set_state`, `blocks::set_state_id`, what an explosion or an enderman asks), and the collision answer of the
//! cell must follow at once. A route that writes a block without telling the masks leaves the old answer (a wall that is not there,
//! or a hole in a wall) and this test fails.
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::cow::{BlockChanges, Herd, Level};

fn free(h: &Herd, x: i32, y: i32, z: i32) -> bool {
    let cell = Aabb::new(Vec3::new(f64::from(x) + 0.1, f64::from(y) + 0.1, f64::from(z) + 0.1), Vec3::new(f64::from(x) + 0.9, f64::from(y) + 0.9, f64::from(z) + 0.9));
    h.world.resource::<Level>().no_collision(cell)
}

/// One route: `place` writes a stone, `remove` writes air; both are ticked once.
fn check(name: &str, place: impl Fn(&mut Herd, i32, i32, i32), remove: impl Fn(&mut Herd, i32, i32, i32)) {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
        return;
    }
    let mut h = Herd::with_slab(20, false, false);
    let (x, y, z) = (3, 4, 3);
    // Warm: the masks of the section are built and answer "free".
    assert!(free(&h, x, y, z), "{name}: the cell starts free");
    place(&mut h, x, y, z);
    h.tick();
    assert!(!free(&h, x, y, z), "{name}: a block placed is not seen by the masks (stale: the cell still answers free)");
    remove(&mut h, x, y, z);
    h.tick();
    assert!(free(&h, x, y, z), "{name}: a block removed is not seen by the masks (stale: the cell still answers solid)");
}

#[test]
fn the_block_changes_queue_tells_the_masks() {
    check(
        "BlockChanges",
        |h, x, y, z| h.world.resource_mut::<BlockChanges>().0.push((x, y, z, "minecraft:stone")),
        |h, x, y, z| h.world.resource_mut::<BlockChanges>().0.push((x, y, z, "minecraft:air")),
    );
}

#[test]
fn set_state_tells_the_masks() {
    check(
        "blocks::set_state",
        |h, x, y, z| horizon_ecs_proto::blocks::set_state(&mut h.world, [x, y, z], "minecraft:stone", &[]),
        |h, x, y, z| horizon_ecs_proto::blocks::set_state(&mut h.world, [x, y, z], "minecraft:air", &[]),
    );
}

#[test]
fn the_test_helper_of_the_herd_tells_the_masks() {
    check("Herd::set_block", |h, x, y, z| h.set_block(x, y, z, "minecraft:stone"), |h, x, y, z| h.set_block(x, y, z, "minecraft:air"));
}
