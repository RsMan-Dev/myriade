//! `pathfind-bench N ROUNDS` (`HORIZON_SPECIES`, `HORIZON_TEST_RESOURCES`): the `pathfind` system alone, for N animals each given a
//! target 1.5 blocks away: instructions per search, by round (the first rounds are cold).
use horizon_ecs_proto::{cow::Herd, profile::Counter};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let rounds: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(6);
    let mut herd = Herd::with_mix_awake(n, 6.0, true, false, 42, false, 0);
    for _ in 0..50 {
        herd.tick();
    }
    let c = Counter::new();
    // The `Finder` alone on the same data, for what the system adds.
    let mut line = Vec::new();
    for _ in 0..rounds {
        herd.bench_pathfind_prepare();
        let c0 = c.read();
        let searches = herd.bench_finder_only();
        line.push((c.read() - c0) / searches.max(1) as u64);
        herd.bench_pathfind_clear(false);
    }
    println!("Finder alone, same data: instr per search by round {line:?}");
    for flush in [false, true] {
        let mut line = Vec::new();
        for _ in 0..rounds {
            herd.bench_pathfind_prepare();
            let c0 = c.read();
            let searches = herd.bench_pathfind_run();
            line.push((c.read() - c0) / searches.max(1) as u64);
            herd.bench_pathfind_clear(flush);
        }
        println!("flush {flush}: instr per search by round {line:?}");
    }
}
