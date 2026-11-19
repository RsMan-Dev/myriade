//! The optimised block-collision kernel against the reference one, bit for bit, over random calls in a varied
//! random world (suivi/ecs-noyau-collisions.md). `HORIZON_TEST_RESOURCES` must be set.
use horizon_ecs_proto::kernel_lab::{change_block, cube_world, occupancy_of, random_calls, run, run_kernel, run_reference, varied_world, Rng};

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
    for seed in 1..=6u64 {
        let world = varied_world(seed);
        for c in random_calls(seed, 12_000) {
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

/// The flag of the unit-cube states, state by state: set exactly for a state whose cached shape is the unit cube and
/// that has no contextual rule and no entity-kind variants; and the states that are the unit cube but not plain (the
/// ones the flag must keep out) are listed, so that the equality test's worlds can hold them.
#[test]
fn the_plain_cube_flag_is_right_for_every_state() {
    use horizon_world::registry::BlockStateId;
    let mw = varied_world(1);
    let binding = mw.collision;
    let (mut plain, mut cube_but_contextual) = (0, Vec::new());
    for i in 0..mw.registry.len() {
        let id = BlockStateId(i as u32);
        let cube = binding.cached_shapes().cached_shape(id).unwrap().is_some_and(horizon_core::geometry::VoxelShape::is_block);
        let ctx = binding.has_context_rule(id);
        assert_eq!(binding.is_plain_cube(id), cube && !ctx, "state {i}");
        plain += usize::from(binding.is_plain_cube(id));
        if cube && ctx {
            cube_but_contextual.push(mw.registry.state(id).unwrap().name.clone());
        }
    }
    eprintln!("{plain} plain cube states; unit cubes with a contextual rule: {cube_but_contextual:?}");
    assert!(plain > 100);
}

/// The masks answer for plain-cube worlds (and decline, the kernel running unchanged, where a block that is not a plain cube is
/// near): equal to the reference bit for bit, the fast path taken for most calls of the pure-cube worlds, and still equal after
/// blocks change (the masks of their sections rebuilt).
#[test]
fn the_occupancy_masks_equal_the_reference_in_cube_worlds_and_after_changes() {
    use std::sync::atomic::Ordering::Relaxed;
    let (mut calls, mut fast_pure) = (0u64, 0u64);
    for (seed, tricky) in [(2u64, false), (4, false), (6, true), (8, true), (10, false)] {
        let (world, ptr) = cube_world(seed, tricky);
        let occ = occupancy_of(world);
        let states: Vec<_> = ["stone", "air", "oak_fence", "glass", "oak_slab", "dirt"]
            .iter()
            .map(|n| world.registry.default_state(&format!("minecraft:{n}")).unwrap())
            .collect();
        let mut r = Rng(seed * 77 + 5);
        for round in 0..12 {
            let before = occ.fast.load(Relaxed);
            let list = random_calls(seed, 2_000);
            for c in &list {
                let (new, old, kernel) = (run(world, c), run_reference(world, c), run_kernel(world, c));
                assert!(same(&new, &old), "seed {seed} round {round}: {:?} {:?} step {} ground {}: masks {new:?} reference {old:?}", c.bounds, c.movement, c.step, c.on_ground);
                assert!(same(&kernel, &old));
                let scene = horizon_client_render::collisions::SceneCollisions { world: world.world, binding: world.collision, subject: None };
                let mut scene2 = horizon_client_render::collisions::SceneCollisions { world: world.world, binding: world.collision, subject: None };
                assert_eq!(occ.no_collision(scene, c.bounds), scene2.unobstructed(c.bounds).unwrap_or(false), "no_collision {:?}", c.bounds);
                calls += 1;
            }
            if !tricky {
                fast_pure += occ.fast.load(Relaxed) - before;
            }
            // Blocks change between rounds: a mask kept past a change would answer for the old world.
            for _ in 0..60 {
                let (x, y, z) = (r.below(28) as i32 - 14, r.below(7) as i32, r.below(28) as i32 - 14);
                let s = states[r.below(states.len())];
                unsafe { change_block(world, ptr, x, y, z, s) };
            }
        }
    }
    assert!(fast_pure * 2 > calls / 2, "the fast path answered only {fast_pure} of the pure-cube calls");
}
