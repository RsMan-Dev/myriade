//! `horizon-ecs-proto MODE N TICKS [THREADS]` (MODE: awake | sleep | par): runs the herd, prints the wall time
//! per tick of the ticks after the first 100 (the warm-up), and how many cows sleep at the end.
use horizon_ecs_proto::flat::Herd;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).map_or("sleep", String::as_str);
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let ticks: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    let threads: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);
    let parallel = mode == "par";
    if parallel {
        bevy_tasks::ComputeTaskPool::get_or_init(|| {
            bevy_tasks::TaskPoolBuilder::new().num_threads(threads).build()
        });
    }
    let mut herd = Herd::new(n, mode != "awake", parallel, 42);
    let warm = 100.min(ticks);
    for _ in 0..warm {
        herd.tick();
    }
    let t0 = Instant::now();
    for _ in warm..ticks {
        herd.tick();
    }
    let dt = t0.elapsed();
    let (asleep, moving, total) = herd.counts();
    let timed = (ticks - warm).max(1);
    println!(
        "{mode} n={n} ticks={ticks} wall/tick={:.1}us asleep={asleep}/{total} moving={moving}",
        dt.as_secs_f64() * 1e6 / timed as f64
    );
}
