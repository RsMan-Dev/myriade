//! `kernel-bench CASE ITERS [ref]` (CASE: a-rest | b-walk | c-wall | d-fall | e-furniture), `HORIZON_TEST_RESOURCES`
//! set: the kernel alone, ITERS calls cycled over 512 prepared calls. Instructions per call: the difference of two
//! ITERS under `perf stat -e instructions:u` (`kbench.sh`).
use horizon_ecs_proto::kernel_lab::{bench_world, case_calls, occupancy_of, run_kernel, run_reference, run_with};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let case = a.get(1).map_or("a-rest", String::as_str);
    let iters: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let reference = a.get(3).is_some_and(|s| s == "ref");
    let kernel = a.get(3).is_some_and(|s| s == "kernel");
    let world = bench_world();
    let calls = case_calls(case, 512);
    let occ = occupancy_of(world);
    let mut sum = 0.0;
    let t = Instant::now();
    for i in 0..iters {
        let c = &calls[i % calls.len()];
        let r = if reference { run_reference(world, c) } else if kernel { run_kernel(world, c) } else { run_with(occ, world, c) };
        sum += r.map_or(0.0, |v| v.x + v.y + v.z);
    }
    let dt = t.elapsed();
    println!("{case} iters={iters} ns/call={:.0} checksum={sum:.6}", dt.as_nanos() as f64 / iters as f64);
}
