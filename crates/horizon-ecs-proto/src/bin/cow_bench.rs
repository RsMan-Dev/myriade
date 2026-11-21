//! `cow-bench MODE N TICKS [FILS [ESPACEMENT]]` (MODE: awake | sleep | par), `HORIZON_TEST_RESOURCES` set: the
//! wall time per tick of the ticks after the first 100, and how many cows sleep and walk at the end.
use horizon_ecs_proto::cow::Herd;
use std::time::Instant;


/// `PATHLOG=1`: the paths made, by what the target is (the block of the search, against the animal and the nearest player).
#[derive(Default)]
struct PathLog {
    last: std::collections::HashMap<usize, ([i32; 3], usize)>,
    made: u64,
    nodes: u64,
    dy: [u64; 5],
    dh: f64,
    on_player: u64,
}
impl PathLog {
    fn see(&mut self, key: usize, at: (f64, f64, f64), path: Option<(&[i32; 3], usize, usize)>, players: &[(f64, f64)]) {
        let Some((target, len, next)) = path else { return };
        if next != 0 || self.last.get(&key).is_some_and(|l| *l == (*target, len)) {
            return;
        }
        self.last.insert(key, (*target, len));
        self.made += 1;
        self.nodes += len as u64;
        self.dy[((target[1] - at.1.floor() as i32) + 2).clamp(0, 4) as usize] += 1;
        self.dh += ((f64::from(target[0]) + 0.5 - at.0).powi(2) + (f64::from(target[2]) + 0.5 - at.2).powi(2)).sqrt();
        let near = players.iter().min_by(|a, b| ((a.0 - at.0).powi(2) + (a.1 - at.2).powi(2)).total_cmp(&((b.0 - at.0).powi(2) + (b.1 - at.2).powi(2))));
        if near.is_some_and(|p| p.0.floor() as i32 == target[0] && p.1.floor() as i32 == target[2]) {
            self.on_player += 1;
        }
    }
    fn print(&self, what: &str) {
        let m = self.made.max(1) as f64;
        println!("{what}: paths {} nodes/path {:.2} dy(-2..+2) {:?} horizontal distance {:.2} target block = nearest player's block {:.1} %", self.made, self.nodes as f64 / m, self.dy, self.dh / m, 100.0 * self.on_player as f64 / m);
    }
}

fn main() {
    horizon_ecs_proto::bench_support::require_optimized();
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
    let mut herd = Herd::with_mix_awake(
        n,
        spacing,
        mode != "awake",
        parallel,
        42,
        std::env::var("HAZARDS").is_ok(),
        std::env::var("MIX").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
    );
    if std::env::var_os("NO_XP").is_some() {
        herd.world.insert_resource(horizon_ecs_proto::orb::NoXp);
    }
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
    let plist = herd.world.resource::<horizon_ecs_proto::cow::Players>().list.clone();
    let mut plog = PathLog::default();
    for _ in warm..ticks {
        if std::env::var_os("PATHLOG").is_some() {
            let mut q = herd.world.query::<(bevy_ecs::entity::Entity, &horizon_ecs_proto::cow::Nav, &horizon_ecs_proto::cow::Pos)>();
            let seen: Vec<_> = q.iter(&herd.world).filter_map(|(e, n, p)| n.path.as_ref().map(|pa| (e.index_u32() as usize, p.0, pa.target, pa.nodes.len(), pa.next))).collect();
            for (k, p, t, len, next) in seen {
                plog.see(k, (p.x, p.y, p.z), Some((&t, len, next)), &plist);
            }
        }
        if timed {
            herd.tick_timed(&mut acc);
        } else {
            herd.tick();
        }
    }
    let dt = t0.elapsed();
    let instr = counter.as_ref().map_or(0, |c| c.read() - i0) / ((ticks - warm).max(1) * n) as u64;
    println!("instr_per_entity_tick={instr}");
    if std::env::var_os("PATHLOG").is_some() {
        plog.print("ecs");
    }
    if std::env::var_os("HUNT").is_some() {
        let c = horizon_ecs_proto::goals::counts(&mut herd.world);
        let hits = herd.world.resource::<horizon_ecs_proto::hostile::PlayerState>().hits.len();
        {
            let a = |i: usize| horizon_ecs_proto::goals::ASKS[i].load(std::sync::atomic::Ordering::Relaxed);
            eprintln!("path searches over the run ({} ticks): asked by stroll {} panic {} melee {}; searches run {}", 300, a(0), a(1), a(5), horizon_ecs_proto::goals::FINDS.load(std::sync::atomic::Ordering::Relaxed));
            let (n, p) = (horizon_ecs_proto::goals::PATH_NODES.load(std::sync::atomic::Ordering::Relaxed), horizon_ecs_proto::goals::PATHS_FOUND.load(std::sync::atomic::Ordering::Relaxed));
            eprintln!("paths found {p}, mean length {:.2} nodes", n as f64 / p.max(1) as f64);
        }
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
            eprintln!("ordered pass: {} animals in the last tick, {:.0} on average; islands per tick {:.1} (sizes 1 / 2-3 / 4-10 / 11-100 / >100, per tick: {:.1} {:.1} {:.1} {:.1} {:.1}), largest island {}", s.last, s.total as f64 / s.ticks as f64, s.islands as f64 / s.ticks as f64, s.sizes[0] as f64 / s.ticks as f64, s.sizes[1] as f64 / s.ticks as f64, s.sizes[2] as f64 / s.ticks as f64, s.sizes[3] as f64 / s.ticks as f64, s.sizes[4] as f64 / s.ticks as f64, s.largest);
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
