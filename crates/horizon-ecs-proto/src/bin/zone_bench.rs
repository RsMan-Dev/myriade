//! `zone-bench rings N [R]`, `zone-bench burst SPACING [R]`, `HORIZON_TEST_RESOURCES` set: the cost of the simulated chunks (`zone.rs`).
//!
//! `rings`: N creatures in one ring of a player (near: within 30 blocks, mid: in the simulated zone and beyond 36 blocks,
//! far: outside it) and in the mixed realistic scene (N over a large surface, players walking): instructions per tick and
//! per creature (`instructions:u`, one thread, 100 warm-up ticks untimed). `burst`: a full chunk goes to sleep and wakes, in
//! form (a) (marker) and form (b) (out of the world), with the memory of each.
use horizon_ecs_proto::{
    cow::Herd,
    profile::Counter,
    zone,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        LIVE.fetch_add(n, Relaxed);
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;

fn half() -> i32 {
    std::env::var("HALF").ok().and_then(|v| v.parse().ok()).unwrap_or(200)
}

/// One counter for the process (each open one takes a slot of the processor's counters).
fn counter() -> &'static Counter {
    static C: std::sync::OnceLock<Counter> = std::sync::OnceLock::new();
    C.get_or_init(Counter::new)
}

/// The points of a grid of `spacing` blocks over the slab that `keep` accepts.
fn grid(spacing: f64, keep: impl Fn(f64, f64) -> bool) -> Vec<(f64, f64)> {
    let side = (f64::from(2 * half() - 4) / spacing) as i32;
    (0..side * side)
        .map(|i| (-f64::from(half()) + 2.0 + f64::from(i % side) * spacing + 0.5, -f64::from(half()) + 2.0 + f64::from(i / side) * spacing + 0.5))
        .filter(|p| keep(p.0, p.1))
        .collect()
}

fn run(label: &str, at: Vec<(f64, f64)>, players: Vec<(f64, f64)>, walk: bool, radius: Option<i32>, ticks: usize, base: f64) -> f64 {
    let mut herd = Herd::with_slab(half(), true, false);
    let n = at.len();
    let shared = std::env::var_os("PROF").map(|_| horizon_ecs_proto::profile::instrument(&mut herd));
    herd.populate(&at, 7);
    herd.set_players(players.clone());
    let counter = counter();
    let warm: usize = std::env::var("WARM").ok().and_then(|v| v.parse().ok()).unwrap_or(400);
    let mut total = 0u64;
    let mut seed = 0x9e37_79b9_u64;
    let mut cur = players;
    let dirs: Vec<(f64, f64)> = cur
        .iter()
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let a = (seed >> 11) as f64 / (1u64 << 53) as f64 * std::f64::consts::TAU;
            (a.cos() * 0.2, a.sin() * 0.2)
        })
        .collect();
    let mut woke_slept = (0, 0);
    for t in 0..(ticks + warm) {
        if walk {
            for (p, d) in cur.iter_mut().zip(&dirs) {
                p.0 = (p.0 + d.0).clamp(-f64::from(half()) + 3.0, f64::from(half()) - 3.0);
                p.1 = (p.1 + d.1).clamp(-f64::from(half()) + 3.0, f64::from(half()) - 3.0);
            }
            herd.set_players(cur.clone());
        }
        // The creatures settle first (they start in the air); the zone is turned on 20 ticks before the measure.
        if t + 20 == warm
            && let Some(r) = radius
        {
            herd.set_simulation(r);
        }
        if t == warm {
            if let Some(sh) = &shared {
                horizon_ecs_proto::profile::reset(sh);
            }
        }
        let c0 = counter.read();
        herd.tick();
        if t >= warm {
            total += counter.read() - c0;
        }
        if t == warm {
            let z = &herd.world.resource::<horizon_ecs_proto::cow::Index>().zone;
            woke_slept = (z.stats.woke, z.stats.slept);
        }
    }
    let asleep = herd.world.query_filtered::<(), bevy_ecs::prelude::With<bevy_ecs::entity_disabling::Disabled>>().iter(&herd.world).count();
    let z = &herd.world.resource::<horizon_ecs_proto::cow::Index>().zone;
    if std::env::var_os("ZDEBUG").is_some() {
        eprintln!("clock {} wheel {}", herd.world.resource::<horizon_ecs_proto::cow::Clock>().0, herd.world.resource::<horizon_ecs_proto::cow::Wheel>().len());
    }
    println!(
        "{label:<28} creatures={n:>6} asleep={asleep:>6} instr/tick={:>10.0} marginal/creature/tick={:>8.1} (slept {}, woke {} while measured)",
        total as f64 / ticks as f64,
        (total as f64 / ticks as f64 - base) / n.max(1) as f64,
        z.stats.slept - woke_slept.1,
        z.stats.woke - woke_slept.0
    );
    if let Some(sh) = &shared {
        let r = sh.lock().unwrap();
        let t = r.ticks.max(1) as f64;
        let mut rows: Vec<_> = r.rows.iter().map(|x| (x.name.rsplit("::").next().unwrap_or(&x.name).to_string(), (x.run + x.apply) as f64 / t, x.visited as f64 / t)).collect();
        rows.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (name, per, v) in rows.iter().take(9) {
            eprintln!("    {name:<22} {per:>10.0} instr/tick, visited {v:>8.1}/tick");
        }
    }
    total as f64 / ticks as f64
}

fn main() {
    horizon_ecs_proto::bench_support::require_optimized();
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).map_or("rings", String::as_str);
    let n: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let radius: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(6);
    match mode {
        "rings" => {
            let p = (0.0, 0.0);
            let d = |x: f64, z: f64| ((x - p.0).powi(2) + (z - p.1).powi(2)).sqrt();
            let zone_edge = f64::from(radius) * 16.0;
            let ticks: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(200);
            // The zone's reach in blocks from the player at the origin (chunk 0 .. 15): up to (radius + 1) * 16 on the east/north.
            for (name, rad) in [("off (everything ticks)", None), ("zone on", Some(radius))] {
                println!("--- {name}");
                let base = run("no creature", Vec::new(), vec![p], false, rad, ticks, 0.0);
                run("near (< 30 blocks)", grid(n, |x, z| d(x, z) < 30.0), vec![p], false, rad, ticks, base);
                run("mid (38 .. zone edge-20)", grid(n, |x, z| d(x, z) > 38.0 && x.abs().max(z.abs()) < zone_edge - 20.0), vec![p], false, rad, ticks, base);
                run("far (outside the zone)", grid(n, |x, z| x.abs().max(z.abs()) > zone_edge + 40.0), vec![p], false, rad, ticks, base);
            }
        }
        "mixed" => {
            let players: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(10);
            let ticks: usize = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(200);
            let at = grid(n, |_, _| true);
            let ps: Vec<(f64, f64)> = (0..players).map(|i| (-150.0 + 300.0 * ((i * 7 % 10) as f64 / 10.0), -150.0 + 300.0 * ((i * 3 % 10) as f64 / 10.0))).collect();
            let walk = std::env::var("WALK").map_or(true, |v| v != "0");
            let base = run("no creature", Vec::new(), ps.clone(), walk, Some(radius), ticks, 0.0);
            run("mixed, zone off", at.clone(), ps.clone(), walk, None, ticks, base);
            run("mixed, zone on", at, ps, walk, Some(radius), ticks, base);
        }
        "burst" => {
            burst(n, radius);
        }
        "border" => {
            // A player runs along a populated frontier, then jitters across a chunk boundary: cost per tick with the zone on
            // (a creature sleeps the tick its chunk stops ticking, no grace) and off.
            let ticks: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(400);
            let at = grid(n, |_, _| true);
            let run_path = |label: &str, path: &dyn Fn(usize) -> (f64, f64), rad: Option<i32>| {
                let mut herd = Herd::with_slab(half(), true, false);
                herd.populate(&at, 7);
                herd.set_players(vec![path(0)]);
                let c = counter();
                let warm = 300;
                for t in 0..warm {
                    if t + 20 == warm
                        && let Some(r) = rad
                    {
                        herd.set_simulation(r);
                    }
                    herd.tick();
                }
                let before = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
                let c0 = c.read();
                for t in 0..ticks {
                    herd.set_players(vec![path(t)]);
                    herd.tick();
                }
                let total = c.read() - c0;
                let st = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
                println!("{label:<34} zone {:<4} creatures {:>6}: {:>10.0} instr/tick (slept {}, woke {})", if rad.is_some() { "on" } else { "off" }, at.len(), total as f64 / ticks as f64, st.slept - before.slept, st.woke - before.woke);
            };
            let run = |t: usize| (-100.0 + 0.28 * t as f64, 8.0);
            let jitter = |t: usize| (if (t / 2) % 2 == 0 { 15.9 } else { 16.1 }, 8.0);
            let still = |_t: usize| (15.9, 8.0);
            for rad in [None, Some(radius)] {
                run_path("player running (0.28 block/tick)", &run, rad);
                run_path("player jittering across a chunk edge", &jitter, rad);
                run_path("player standing", &still, rad);
            }
        }
        "cycle" => {
            // Sleep and wake the creatures of one chunk again and again (for a profile).
            let at = grid(n, |x, z| (0.0..16.0).contains(&x) && (0.0..16.0).contains(&z));
            let mut herd = Herd::with_slab(half(), true, false);
            let ents = herd.populate(&at, 3);
            for _ in 0..200 {
                herd.tick();
            }
            herd.world.resource_mut::<horizon_ecs_proto::cow::Wheel>().enable_tracking_pub();
            let c = counter();
            let c0 = c.read();
            let cycles: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
            for k in 0..cycles {
                let now = 1000 + k as u64 * 10;
                for &e in &ents {
                    zone::sleep(&mut herd.world, e, now);
                }
                for &e in &ents {
                    zone::wake(&mut herd.world, e, now + 5);
                }
            }
            let tot = c.read() - c0;
            println!("cycle: {} creatures, {} sleep+wake pairs: {:.0} instr per creature per pair", ents.len(), cycles, tot as f64 / (cycles * ents.len()) as f64);
        }
        _ => eprintln!("rings | mixed | burst | cycle"),
    }
}

fn burst(spacing: f64, radius: i32) {
    let side = (f64::from(2 * half() - 4) / spacing) as i32;
    let at: Vec<(f64, f64)> = (0..side * side).map(|i| (-f64::from(half()) + 2.0 + f64::from(i % side) * spacing + 0.5, -f64::from(half()) + 2.0 + f64::from(i / side) * spacing + 0.5)).collect();
    let n = at.len();
    let m0 = LIVE.load(Relaxed);
    let mut herd = Herd::with_slab(half(), true, false);
    let base = LIVE.load(Relaxed);
    herd.populate(&at, 3);
    for _ in 0..150 {
        herd.tick();
    }
    let awake = LIVE.load(Relaxed) - base;
    println!("N={n} spacing={spacing} per chunk={:.0}; awake world: {:.0} bytes/creature ({} MB)", (16.0 / spacing).powi(2), awake as f64 / n as f64, awake >> 20);
    let counter = counter();
    let _ = m0;
    // The player stands at the origin chunk; the zone turns on: every chunk outside it goes to sleep (the first burst).
    herd.set_players(vec![(8.0, 8.0)]);
    let c0 = counter.read();
    herd.set_simulation(radius);
    let enable = counter.read() - c0;
    let z = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
    let after = LIVE.load(Relaxed) - base;
    println!("(a) marker: enable (all but {} chunks sleep): {} instr for {} creatures = {:.0}/creature; world now {:.0} bytes/creature", (2 * radius + 1) * (2 * radius + 1), enable, z.slept, enable as f64 / z.slept.max(1) as f64, after as f64 / n as f64);
    // Idle: a tick with all of them asleep.
    herd.tick();
    let c0 = counter.read();
    for _ in 0..50 {
        herd.tick();
    }
    let idle = (counter.read() - c0) / 50;
    println!("(a) idle tick: {idle} instr in all ({:.2} per sleeping creature, {} awake)", idle as f64 / z.slept.max(1) as f64, n as u64 - z.slept);
    // The player takes one chunk step east: a column of chunks wakes and one sleeps (a ring of the zone).
    let ticks_before = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
    herd.set_players(vec![(8.0 + 16.0, 8.0)]);
    let c0 = counter.read();
    herd.tick();
    let step = counter.read() - c0;
    let st = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
    let (slept, woke) = (st.slept - ticks_before.slept, st.woke - ticks_before.woke);
    // The same tick without the step, for the share that is not the burst.
    let c0 = counter.read();
    herd.tick();
    let calm = counter.read() - c0;
    println!("(a) one chunk step: tick {step} instr (calm tick {calm}), {slept} slept, {woke} woke: {:.0} instr per creature moved (burst above the calm tick)", (step.saturating_sub(calm)) as f64 / (slept + woke).max(1) as f64);
    // (a) for the same chunk, to compare like with like.
    let mut herd = Herd::with_slab(half(), true, false);
    let ents = herd.populate(&at, 3);
    for _ in 0..150 {
        herd.tick();
    }
    herd.world.resource_mut::<horizon_ecs_proto::cow::Wheel>().enable_tracking_pub();
    let chunk: Vec<_> = ents.into_iter().filter(|&e| {
        let p = herd.world.get::<horizon_ecs_proto::cow::Pos>(e).unwrap().0;
        (0.0..16.0).contains(&p.x) && (0.0..16.0).contains(&p.z)
    }).collect();
    let k = chunk.len();
    let now = herd.world.resource::<horizon_ecs_proto::cow::Clock>().0;
    let c0 = counter.read();
    for &e in &chunk {
        zone::sleep(&mut herd.world, e, now);
    }
    let out_a = counter.read() - c0;
    let c0 = counter.read();
    for &e in &chunk {
        zone::wake(&mut herd.world, e, now + 1000);
    }
    let back_a = counter.read() - c0;
    println!("(a) {k} creatures of a chunk: out {out_a} instr ({:.0}/creature), back {back_a} instr ({:.0}/creature)", out_a as f64 / k as f64, back_a as f64 / k as f64);
}
