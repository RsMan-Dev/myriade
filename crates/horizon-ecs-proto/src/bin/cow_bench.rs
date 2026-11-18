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
    let mut herd = Herd::with_mix(
        n,
        spacing,
        mode != "awake",
        parallel,
        42,
        std::env::var("HAZARDS").is_ok(),
        std::env::var("MIX").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
    );
    if std::env::var_os("ORDERED_PUSH").is_some() {
        herd.set_ordered_push(true);
    }
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
    if std::env::var_os("HUNT").is_some() {
        herd.set_online(&list);
    }
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
    *herd.world.resource_mut::<horizon_ecs_proto::cow::OrderedStats>() = Default::default();
    let counter = horizon_ecs_proto::instr::Counter::new();
    let i0 = counter.as_ref().map_or(0, |c| c.read());
    let t0 = Instant::now();
    for _ in warm..ticks {
        if timed {
            herd.tick_timed(&mut acc);
        } else {
            herd.tick();
        }
    }
    let dt = t0.elapsed();
    let instr = counter.as_ref().map_or(0, |c| c.read() - i0) / ((ticks - warm).max(1) * n) as u64;
    println!("instr_per_entity_tick={instr}");
    if std::env::var_os("HUNT").is_some() {
        let c = horizon_ecs_proto::goals::counts(&mut herd.world);
        let hits = herd.world.resource::<horizon_ecs_proto::hostile::PlayerState>().hits.len();
        eprintln!("hunting {:?} meleeing {:?} player hits so far {hits}", c.iter().find(|x| x.0 == "hunting"), c.iter().find(|x| x.0 == "meleeing"));
    }
    if std::env::var_os("COUNT_OVERLAPS").is_some() {
        let boxes: Vec<horizon_core::geometry::Aabb> = herd.world.query::<&horizon_ecs_proto::cow::Bounds>().iter(&herd.world).map(|b| b.0).collect();
        let n_over = (0..boxes.len()).filter(|&i| (0..boxes.len()).any(|j| j != i && boxes[i].intersects(boxes[j]))).count();
        eprintln!("animals overlapping another at the end: {n_over} of {}", boxes.len());
    }
    {
        let s = herd.world.resource::<horizon_ecs_proto::cow::OrderedStats>();
        if s.ticks > 0 {
            eprintln!("ordered pass: {} animals in the last tick, {:.0} on average", s.last, s.total as f64 / s.ticks as f64);
        }
    }
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
