//! The path finder with its fast paths against the original one, node for node (kind included), over random
//! searches in the varied worlds of `kernel_lab`. `HORIZON_TEST_RESOURCES` must be set.
use horizon_client_lib::mobs::path::{Evaluator, Finder, PathMob, with_reference_finder, PATH_TYPES};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::kernel_lab::{Rng, varied_world};

#[test]
fn the_finder_finds_the_same_paths() {
    let (mut searches, mut found) = (0, 0);
    for seed in 1..=4u64 {
        let mw = varied_world(seed);
        let mut r = Rng(seed * 77 + 5);
        let (lo, hi) = if seed % 2 == 0 { (-12.0, 12.0) } else { (-30.0, 30.0) };
        for _ in 0..300 {
            let p = Vec3::new(r.range(lo, hi), f64::from(r.below(6) as u32) + 1.0, r.range(lo, hi));
            let (w, h) = [(0.9, 1.4), (0.6, 1.95), (0.4, 0.7)][r.below(3)];
            let mut malus = [None; PATH_TYPES];
            if r.below(2) == 0 {
                malus = std::array::from_fn(|i| (i % 5 == 0).then_some(1.0 + i as f32));
            }
            let pm = PathMob {
                position: p,
                width: w,
                height: h,
                on_ground: true,
                in_water: false,
                max_up_step: [0.6, 1.0][r.below(2)],
                max_fall_distance: 3,
                malus,
                can_float: r.below(2) == 0,
                can_open_doors: r.below(2) == 0,
                can_pass_doors: r.below(2) == 0,
                can_walk_over_fences: r.below(4) == 0,
                stands_on_lava: false,
            };
            let spread = [4.0, 10.0, 25.0][r.below(3)];
            let target = [(p.x + r.range(-spread, spread)).floor() as i32, r.below(6) as i32 + 1, (p.z + r.range(-spread, spread)).floor() as i32];
            let range = [16.0, 35.0][r.below(2)];
            let run = || Finder::new(mw, &pm, Evaluator::Walk).find(&[target], range, 1, (range * 16.0) as i32);
            let new = run();
            let old = with_reference_finder(run);
            searches += 1;
            found += usize::from(old.is_some());
            let key = |p: &Option<horizon_client_lib::mobs::path::Path>| {
                p.as_ref().map(|p| (p.nodes.iter().map(|n| (n.x, n.y, n.z, n.kind as u8)).collect::<Vec<_>>(), p.next, p.target, p.dist_to_target.to_bits(), p.reached))
            };
            assert_eq!(key(&new), key(&old), "seed {seed}: from {p:?} to {target:?}");
        }
    }
    assert!(found * 3 > searches, "only {found} of {searches} searches found a path");
}
