//! `pool-bench [N [ROUNDS]]`, `HORIZON_TEST_RESOURCES` set: the cost of one collision call (`Occupancy::resolve`, what `do_move` calls)
//! for N walkers PACKED into one or a few sections (spacing 0.5 to 1 block) against N walkers SPREAD one per section (16.5 blocks), and
//! how many calls of one tick ask for the same neighbourhood of cells (the swept box and its ring, in whole cells).
use myriade_level::collisions::SceneCollisions;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::instr::Counter;
use std::collections::HashSet;

fn run(label: &str, n: usize, rounds: usize, spacing: f64, counter: &Counter) {
    let half = 270;
    let (level, _s, _k) = horizon_ecs_proto::cow::build_level(half, false);
    let mw = level.mw.0;
    let occ = level.occ;
    let cols = (n as f64).sqrt().ceil() as usize;
    let origin = if spacing > 10.0 { -f64::from(half) + 4.0 } else { -8.0 };
    let mut pos: Vec<Vec3> = (0..n).map(|i| Vec3::new(origin + (i % cols) as f64 * spacing + 0.3, 1.0, origin + (i / cols) as f64 * spacing + 0.3)).collect();
    let (w, h) = (0.9, 1.4);
    let mut total = 0u64;
    let mut calls = 0u64;
    let mut distinct_sum = 0usize;
    let mut sections = HashSet::new();
    // Warm-up round (the masks are built when a section is first read).
    for round in 0..=rounds {
        let mut keys: HashSet<[i32; 6]> = HashSet::new();
        for p in pos.iter_mut() {
            let want = Vec3::new(0.1, -0.0784, 0.0);
            let b = Aabb::new(Vec3::new(p.x - w / 2.0, p.y, p.z - w / 2.0), Vec3::new(p.x + w / 2.0, p.y + h, p.z + w / 2.0));
            let scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
            let c0 = counter.read();
            let moved = occ.resolve(scene, b, want, 0.6, true).unwrap_or(Vec3::ZERO);
            let d = counter.read() - c0;
            if round > 0 {
                total += d;
                calls += 1;
                keys.insert([
                    (b.min_position().x - 1.0).floor() as i32,
                    (b.min_position().y - 1.0).floor() as i32,
                    (b.min_position().z - 1.0).floor() as i32,
                    (b.max_position().x + 0.1 + 1.0).floor() as i32,
                    (b.max_position().y + 1.0).floor() as i32,
                    (b.max_position().z + 1.0).floor() as i32,
                ]);
                sections.insert(((p.x.floor() as i32) >> 4, (p.z.floor() as i32) >> 4));
            }
            *p = Vec3::new(p.x + moved.x, p.y, p.z + moved.z);
        }
        if round > 0 {
            distinct_sum += keys.len();
        }
    }
    println!(
        "{label}: {n} walkers, spacing {spacing}: {:.0} instr per collision call ({} calls); sections touched {}; distinct cell neighbourhoods per tick {:.0} of {n} ({:.1} calls per neighbourhood)",
        total as f64 / calls as f64,
        calls,
        sections.len(),
        distinct_sum as f64 / rounds as f64,
        n as f64 * rounds as f64 / distinct_sum as f64
    );
    println!("    served by the masks {} , returned to the kernel {}", occ.fast.load(std::sync::atomic::Ordering::Relaxed), occ.fallback.load(std::sync::atomic::Ordering::Relaxed));
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let rounds: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let counter = Counter::new().expect("perf counter");
    for (label, s) in [("dispersed", 16.5), ("packed 1.0", 1.0), ("packed 0.7", 0.7), ("packed 0.5", 0.5)] {
        run(label, n, rounds, s, &counter);
    }
}
