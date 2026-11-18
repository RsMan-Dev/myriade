//! The optimised block-collision kernel against the reference one, bit for bit, over random calls in a varied
//! random world (suivi/ecs-noyau-collisions.md). `HORIZON_TEST_RESOURCES` must be set.
use horizon_ecs_proto::kernel_lab::{random_calls, run, run_reference, varied_world};

fn same(a: &Result<horizon_core::geometry::Vec3, String>, b: &Result<horizon_core::geometry::Vec3, String>) -> bool {
    match (a, b) {
        (Ok(a), Ok(b)) => [a.x, a.y, a.z].map(f64::to_bits) == [b.x, b.y, b.z].map(f64::to_bits),
        (Err(a), Err(b)) => a == b,
        _ => false,
    }
}

#[test]
fn the_fast_kernel_equals_the_reference_bit_for_bit() {
    let mut calls = 0;
    let mut moved = 0;
    for seed in 1..=4u64 {
        let world = varied_world(seed);
        for c in random_calls(seed, 15_000) {
            let (new, old) = (run(world, &c), run_reference(world, &c));
            assert!(
                same(&new, &old),
                "seed {seed}: {:?} {:?} step {} ground {}: new {new:?} old {old:?}",
                c.bounds, c.movement, c.step, c.on_ground
            );
            calls += 1;
            moved += usize::from(old.as_ref().is_ok_and(|v| v.x != c.movement.x || v.y != c.movement.y || v.z != c.movement.z));
        }
    }
    // The cases must actually collide, or the equality says nothing.
    assert!(moved * 5 > calls, "only {moved} of {calls} calls were stopped by a block");
}
