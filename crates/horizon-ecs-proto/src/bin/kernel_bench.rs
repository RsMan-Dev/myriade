//! `kernel-bench CASE ITERS [ref]` (CASE: a-rest | b-walk | c-wall | d-fall | e-furniture), `HORIZON_TEST_RESOURCES`
//! set: the kernel alone, ITERS calls cycled over 512 prepared calls. Instructions per call: the difference of two
//! ITERS under `perf stat -e instructions:u` (`kbench.sh`).
use horizon_ecs_proto::kernel_lab::{bench_world_mut, case_calls, occupancy_of, run_kernel, run_reference, run_with};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let case = a.get(1).map_or("a-rest", String::as_str);
    let iters: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let reference = a.get(3).is_some_and(|s| s == "ref");
    let kernel = a.get(3).is_some_and(|s| s == "kernel");
    // `g-churn:N`: walkers in the section where N blocks change every 32 calls (an explosion, a piston farm).
    let (case, churn) = match case.split_once(':') {
        Some((c, n)) => (c, n.parse::<usize>().unwrap_or(0)),
        None => (case, 0),
    };
    let (world, ptr) = bench_world_mut();
    let stone = world.registry.default_state("minecraft:stone").unwrap();
    let air = world.registry.default_state("minecraft:air").unwrap();
    let mut lcg = 0x2545_F491_4F6C_DD1Du64;
    let mut flip = false;
    let calls = case_calls(case, 512);
    let occ = occupancy_of(world);
    let mut sum = 0.0;
    let t = Instant::now();
    for i in 0..iters {
        let c = &calls[i % calls.len()];
        if churn > 0 && i % 32 == 0 {
            flip = !flip;
            for _ in 0..churn {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let (x, y, z) = (-16 + (lcg >> 33) as i32 % 16, 1 + (lcg >> 20) as i32 % 3, (lcg >> 40) as i32 % 16);
                let s = if flip { stone } else { air };
                if kernel {
                    // SAFETY: nothing else runs.
                    unsafe { let _ = (*ptr).set(x, y, z, s); }
                } else {
                    // SAFETY: nothing else runs.
                    unsafe { horizon_ecs_proto::kernel_lab::change_block(world, ptr, x, y, z, s) };
                }
            }
        }
        let r = if reference { run_reference(world, c) } else if kernel { run_kernel(world, c) } else { run_with(occ, world, c) };
        sum += r.map_or(0.0, |v| v.x + v.y + v.z);
    }
    let dt = t.elapsed();
    let (fast, fall) = (occ.fast.load(std::sync::atomic::Ordering::Relaxed), occ.fallback.load(std::sync::atomic::Ordering::Relaxed));
    println!("{case} churn={churn} declined_by_lockfree={} masks={fast} kernel_fallback={fall} table={:?} iters={iters}", occ.declined.load(std::sync::atomic::Ordering::Relaxed), occ.table_size());
    // The first encounter of a state: a fresh table classifies every state of the registry once (shape cloned into the table).
    let fresh = horizon_ecs_proto::occupancy::Occupancy::default();
    let t1 = Instant::now();
    let n = world.registry.len();
    for i in 0..n {
        let _ = fresh.class_name(world.collision, horizon_world::registry::BlockStateId(i as u32));
    }
    let dt1 = t1.elapsed();
    println!("table: {n} states classified in {:?} ({:.0} ns per state, first encounter), {:?} (states, shapes); one section's masks {} bytes", dt1, dt1.as_nanos() as f64 / n as f64, fresh.table_size(), horizon_ecs_proto::occupancy::Occupancy::section_bytes());
    println!("{case} iters={iters} ns/call={:.0} checksum={sum:.6}", dt.as_nanos() as f64 / iters as f64);
}
