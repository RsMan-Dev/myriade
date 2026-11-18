//! `cow-bench MODE N TICKS [FILS [ESPACEMENT]]` (MODE: awake | sleep | par), `HORIZON_TEST_RESOURCES` set: the
//! wall time per tick of the ticks after the first 100, and how many cows sleep and walk at the end.
use horizon_ecs_proto::cow::Herd;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).map_or("sleep", String::as_str);
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let ticks: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    let threads: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);
    let spacing: f64 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let players: usize = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);
        let parallel = mode == "par" || mode == "timed_par";
    let timed = mode.starts_with("timed");
    if parallel {
        bevy_tasks::ComputeTaskPool::get_or_init(|| bevy_tasks::TaskPoolBuilder::new().num_threads(threads).build());
    }
    let mut herd = Herd::with_hazards(n, spacing, mode != "awake", parallel, 42, std::env::var("HAZARDS").is_ok());
    let mut acc = [0.0_f64; 12];
    // The players stand on a grid over the slab.
    let side = (n as f64).sqrt().ceil() * spacing;
    let k = (players as f64).sqrt().ceil() as usize;
    let list: Vec<(f64, f64)> = (0..players)
        .map(|i| {
            let (cx, cz) = ((i % k.max(1)) as f64 + 0.5, (i / k.max(1)) as f64 + 0.5);
            (-side / 2.0 + side * cx / k.max(1) as f64, -side / 2.0 + side * cz / k.max(1) as f64)
        })
        .collect();
    herd.set_players(list);
    if mode == "trace" {
        // The active modules over the ticks (a packed herd: spacing below the width of a cow overlaps).
        for t in 0..ticks {
            herd.tick();
            if t % 10 == 0 || t < 10 {
                let (idle, following, moving, total) = herd.counts();
                println!("tick {t:4}: idle {idle:6}/{total}  physics active {moving:6}  following {following:6}");
            }
        }
        return;
    }
    let warm: usize = std::env::var("WARM").ok().and_then(|v| v.parse().ok()).unwrap_or(100).min(ticks);
    for _ in 0..warm {
        herd.tick();
    }
    let t0 = Instant::now();
    for _ in warm..ticks {
        if timed {
            herd.tick_timed(&mut acc);
        } else {
            herd.tick();
        }
    }
    let dt = t0.elapsed();
    let (idle, following, moving, total) = herd.counts();
    let window = (ticks - warm).max(1);
    if timed {
        let total: f64 = acc.iter().sum();
        for (name, t) in Herd::SYSTEMS.iter().zip(acc) {
            println!("  {name:13} {:8.1} us/tick {:5.1} %", t * 1e6 / (ticks - warm).max(1) as f64, 100.0 * t / total);
        }
    }
    println!(
        "{mode} n={n} ticks={ticks} spacing={spacing} players={players} wall/tick={:.1}us idle={idle}/{total} following={following} moving={moving}",
        dt.as_secs_f64() * 1e6 / window as f64
    );
}
