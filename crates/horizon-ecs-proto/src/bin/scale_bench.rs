//! `scale-bench SCENE N TICKS [WARM [THREADS]]`, `HORIZON_TEST_RESOURCES` set. Scale ladder of the ECS herd: wall time per tick
//! (median, 99th percentile, mean, max), instructions per tick (main thread), resident memory, deaths in the window.
//! SCENE: `rest` (animals at rest, no player), `mix` (rings: walking players, zone view = simulation = 10, 6-block grid, the
//! bilan's 2 players per 17 424 animals), `solicit` (goal_bench's: 50 players, 30 % holding, 20 hurt, 20 love per 50 ticks,
//! per 2 000 animals, spacing 2), `chase` (the process's species, `HORIZON_SPECIES=minecraft:zombie`, one immortal player
//! per zombie, spacing 6). THREADS: absent or 0 = sequential batch systems; k >= 1 = `Config.parallel` with a task pool of k threads.
//! `SPACING` overrides the spacing. Prints a checksum of the positions at the end (sequential against parallel).
use horizon_ecs_proto::{
    cow::{Cause, DamageQueue, Herd, Pos},
    goals::{self, Online, Player},
};
use std::time::Instant;

fn status(key: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok())))
        .unwrap_or(0)
}

/// Hash of the positions and velocities of every entity, by entity index.
fn state_hash(herd: &mut Herd) -> u64 {
    let mut ps: Vec<(u32, [u64; 6])> = herd
        .world
        .query::<(bevy_ecs::entity::Entity, &Pos, &horizon_ecs_proto::cow::Vel)>()
        .iter(&herd.world)
        .map(|(e, p, v)| (e.index_u32(), [p.0.x.to_bits(), p.0.y.to_bits(), p.0.z.to_bits(), v.0.x.to_bits(), v.0.y.to_bits(), v.0.z.to_bits()]))
        .collect();
    ps.sort_unstable();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in &ps {
        for v in std::iter::once(u64::from(p.0)).chain(p.1) {
            h = (h ^ v).wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn main() {
    if std::env::var_os("OWN").is_some() { horizon_ecs_proto::pathsearch::USE_OWN.store(true, std::sync::atomic::Ordering::Relaxed); }
    eprintln!("path search: {}", if horizon_ecs_proto::pathsearch::USE_OWN.load(std::sync::atomic::Ordering::Relaxed) { "OWN (pathsearch.rs)" } else { "Horizon Finder" });
    if std::env::var_os("NOLAYER").is_some() { horizon_ecs_proto::pathsearch::LAYER_ON.store(false, std::sync::atomic::Ordering::Relaxed); }
    let a: Vec<String> = std::env::args().collect();
    let scene = a.get(1).map_or("rest", String::as_str);
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let ticks: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let warm: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(400);
    let threads: usize = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
    let parallel = threads > 0;
    if parallel {
        bevy_tasks::ComputeTaskPool::get_or_init(|| bevy_tasks::TaskPoolBuilder::new().num_threads(threads).build());
    }
    let envf = |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    horizon_ecs_proto::bench_support::install_dummies(0);
    let rss0 = status("VmRSS:");
    let t_build = Instant::now();
    let mut seed = Rng(0x9e37_79b9);
    let mut walkers: Vec<(f64, f64, f64, f64)> = Vec::new(); // mix: x, z, dx, dz
    let mut herd;
    let mut hold_item = 0;
    let (mut npl, mut holders, mut hurt, mut love) = (0usize, 0usize, 0usize, 0usize);
    let mut spot: Box<dyn Fn(usize) -> (f64, f64)> = Box::new(|_| (0.0, 0.0));
    let spacing;
    match scene {
        "rest" | "crowd" | "walk" => {
            // `crowd` and `walk`: every animal stays awake for good (`ActiveUntil` max) so the stroll goal runs without a player.
            spacing = envf("SPACING", if scene == "crowd" { 0.8 } else { 6.0 });
            herd = Herd::new(n, spacing, true, parallel, 42);
            if scene != "rest" {
                let all: Vec<bevy_ecs::entity::Entity> = herd.world.query_filtered::<bevy_ecs::entity::Entity, bevy_ecs::prelude::With<horizon_ecs_proto::cow::Stroll>>().iter(&herd.world).collect();
                for e in all {
                    herd.world.get_mut::<horizon_ecs_proto::cow::ActiveUntil>(e).unwrap().0 = u64::MAX;
                    herd.world.resource_mut::<goals::Entered>().0.push(e);
                }
            }
        }
        "mix" => {
            spacing = envf("SPACING", 6.0);
            let side = (n as f64).sqrt().ceil() as i32;
            let half = ((f64::from(side) * spacing + 4.0) / 2.0) as i32 + 1;
            herd = Herd::with_slab(half, true, false);
            if parallel {
                herd.world.resource_mut::<horizon_ecs_proto::cow::Config>().parallel = true;
            }
            let at: Vec<(f64, f64)> = (0..side * side)
                .take(n)
                .map(|i| (-f64::from(half) + 2.0 + f64::from(i % side) * spacing + 0.5, -f64::from(half) + 2.0 + f64::from(i / side) * spacing + 0.5))
                .collect();
            herd.populate(&at, 7);
            npl = ((n as f64 * 2.0 / 17424.0).round() as usize).max(2);
            let k = (npl as f64).sqrt().ceil() as usize;
            for i in 0..npl {
                let (cx, cz) = ((i % k) as f64 + 0.5, (i / k) as f64 + 0.5);
                let x = -f64::from(half) + 2.0 * f64::from(half) * cx / k as f64 + (seed.unit() - 0.5) * 40.0;
                let z = -f64::from(half) + 2.0 * f64::from(half) * cz / k as f64 + (seed.unit() - 0.5) * 40.0;
                let ang = seed.unit() * std::f64::consts::TAU;
                walkers.push((x, z, ang.cos() * 0.2, ang.sin() * 0.2));
            }
            let h = f64::from(half);
            herd.set_players(walkers.iter().map(|w| (w.0.clamp(-h + 3.0, h - 3.0), w.1.clamp(-h + 3.0, h - 3.0))).collect());
        }
        "solicit" | "chase" => {
            spacing = envf("SPACING", if scene == "chase" { 6.0 } else { 2.0 });
            herd = Herd::new(n, spacing, true, parallel, 42);
            let side = (n as f64).sqrt().ceil() * spacing;
            if scene == "chase" {
                npl = envf("PLAYERS", n as f64) as usize;
            } else {
                npl = (n as f64 * 50.0 / 2000.0) as usize;
                holders = (npl as f64 * 0.3) as usize;
                hurt = n / 100;
                love = n / 100;
                hold_item = goals::item_id(&std::env::var("ITEM").unwrap_or_else(|_| "minecraft:wheat".into()));
            }
            let k = ((npl as f64).sqrt().ceil() as usize).max(1);
            spot = Box::new(move |i| {
                let (cx, cz) = ((i % k) as f64 + 0.5, (i / k) as f64 + 0.5);
                (-side / 2.0 + side * cx / k as f64, -side / 2.0 + side * cz / k as f64)
            });
            if scene == "chase" {
                let list: Vec<(f64, f64)> = (0..npl).map(&spot).collect();
                herd.set_online(&list);
                herd.set_players(list);
            } else {
                for i in 0..npl {
                    let (x, z) = spot(i);
                    herd.world.resource_mut::<Online>().set(Player { id: i as u32, x, y: 1.0, z, main: if i < holders { hold_item } else { 0 }, off: 0, ..Default::default() });
                }
            }
        }
        _ => panic!("rest | mix | solicit | chase"),
    }
    if std::env::var_os("NO_XP").is_some() {
        herd.world.insert_resource(horizon_ecs_proto::orb::NoXp);
    }
    if std::env::var_os("EAGER").is_some() {
        goals::set_lazy_look(&mut herd.world, false);
    }
    // Preload the loot tables (read at the first death otherwise: tens of ms).
    for kind in ["minecraft:cow", "minecraft:zombie", "minecraft:pig", "minecraft:sheep", "minecraft:chicken"] {
        let _ = horizon_ecs_proto::loot::roll(&mut herd.world, kind, false, false, 1);
    }
    let build_s = t_build.elapsed().as_secs_f64();
    let cows: Vec<_> = if scene == "solicit" { herd.world.query::<bevy_ecs::entity::Entity>().iter(&herd.world).collect() } else { Vec::new() };
    let walking = holders / 10;
    let counter = horizon_ecs_proto::instr::Counter::new();
    let before = herd.counts().3;
    let animals0 = herd.world.query_filtered::<(), bevy_ecs::prelude::With<horizon_ecs_proto::cow::Stroll>>().iter(&herd.world).count();
    let half_i = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
    let _ = half_i;
    let mut tickhash = std::env::var("TICKHASH").ok().map(|p| std::io::BufWriter::new(std::fs::File::create(p).unwrap()));
    // `TICKSTAT=file`: per tick "tick path_searches instructions wall_us".
    let mut tickstat = std::env::var("TICKSTAT").ok().map(|p| std::io::BufWriter::new(std::fs::File::create(p).unwrap()));
    let mut times: Vec<f64> = Vec::with_capacity(ticks);
    let prof = std::env::var_os("PROFILE").map(|_| horizon_ecs_proto::profile::instrument(&mut herd));
    let mut instr_total = 0u64;
    let mut instr_ticks: Vec<u64> = Vec::new();
    let zone_radius = envf("VIEW", 10.0) as i32;
    let (mut hs, mut ws) = (0u64, 0u64);
    let hh = if scene == "mix" { herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats.slept } else { 0 };
    let _ = hh;
    for t in 0..(warm + ticks) {
        if scene == "mix" {
            let half = (((n as f64).sqrt().ceil() * spacing + 4.0) / 2.0) as i32 + 1;
            let h = f64::from(half);
            for w in &mut walkers {
                w.0 = (w.0 + w.2).clamp(-h + 3.0, h - 3.0);
                w.1 = (w.1 + w.3).clamp(-h + 3.0, h - 3.0);
            }
            herd.set_players(walkers.iter().map(|w| (w.0, w.1)).collect());
            if t + 20 == warm {
                herd.set_simulation(zone_radius);
            }
        }
        if scene == "solicit" {
            for i in 0..walking {
                let (x, z) = spot(i);
                let phase = ((t / 100 + i) % 2) as f64 * 2.0 - 1.0;
                let dx = ((t % 100) as f64 * 0.1 - 5.0) * phase;
                herd.world.resource_mut::<Online>().set(Player { id: i as u32, x: x + dx, y: 1.0, z, main: hold_item, off: 0, ..Default::default() });
            }
            if t % 50 == 10 && t >= 50 {
                for _ in 0..hurt {
                    let e = cows[(seed.next() >> 11) as usize % cows.len()];
                    herd.world.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Entity);
                }
                for _ in 0..love {
                    let i = (seed.next() >> 11) as usize % (cows.len() - 1);
                    goals::feed(&mut herd.world, cows[i]);
                    goals::feed(&mut herd.world, cows[i + 1]);
                }
            }
        }
        if t == warm {
            if let Some(p) = prof.as_ref() { horizon_ecs_proto::profile::reset(p); }
            hs = status("VmHWM:");
            ws = status("VmRSS:");
        }
        let i0 = counter.as_ref().map_or(0, |c| c.read());
        let f0 = goals::FINDS.load(std::sync::atomic::Ordering::Relaxed);
        let t0 = Instant::now();
        herd.tick();
        let dt = t0.elapsed().as_secs_f64();
        if let Some(f) = tickstat.as_mut() {
            use std::io::Write;
            let _ = writeln!(f, "{t} {} {} {:.1}", goals::FINDS.load(std::sync::atomic::Ordering::Relaxed) - f0, counter.as_ref().map_or(0, |c| c.read()) - i0, dt * 1e6);
        }
        if let Some(f) = tickhash.as_mut() {
            use std::io::Write;
            let _ = writeln!(f, "{t} {:016x}", state_hash(&mut herd));
        }
        if t >= warm {
            times.push(dt);
            let di = counter.as_ref().map_or(0, |c| c.read()) - i0;
            instr_total += di;
            instr_ticks.push(di);
        }
    }
    let after = herd.counts().3;
    let animals1 = herd.world.query_filtered::<(), bevy_ecs::prelude::With<horizon_ecs_proto::cow::Stroll>>().iter(&herd.world).count();
    let asleep = herd.world.query_filtered::<(), bevy_ecs::prelude::With<bevy_ecs::entity_disabling::Disabled>>().iter(&herd.world).count();
    times.sort_by(f64::total_cmp);
    instr_ticks.sort_unstable();
    let q = |p: f64| times[(((times.len() - 1) as f64) * p).round() as usize] * 1e3;
    let mean = times.iter().sum::<f64>() / times.len() as f64 * 1e3;
    let (idle, following, moving, total) = herd.counts();
    // Checksum of the positions, in entity order, to compare the sequential and the parallel run.
    let mut ps: Vec<(u32, u64, u64, u64)> = herd.world.query::<(bevy_ecs::entity::Entity, &Pos)>().iter(&herd.world).map(|(e, p)| (e.index_u32(), p.0.x.to_bits(), p.0.y.to_bits(), p.0.z.to_bits())).collect();
    ps.sort_unstable();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in &ps {
        for v in [u64::from(p.0), p.1, p.2, p.3] {
            h = (h ^ v).wrapping_mul(0x100_0000_01b3);
        }
    }
    if let Some(p) = prof.as_ref() {
        let r = p.lock().unwrap();
        let t = r.ticks.max(1) as f64;
        let mut rows: Vec<_> = r.rows.iter().map(|x| (x.name.rsplit("::").next().unwrap_or(&x.name).to_string(), (x.run + x.apply) as f64 / t, x.visited as f64 / t)).collect();
        let sum: f64 = rows.iter().map(|x| x.1).sum();
        rows.sort_by(|a, b| b.1.total_cmp(&a.1));
        eprintln!("PROFILE per tick (n={n}): sum rows {:.0} instr/tick = {:.1} per entity; schedule total {:.0}", sum, sum / n as f64, r.total as f64 / t);
        for x in rows.iter().take(25) { eprintln!("  {:>10.0} {:>7.1}/ent visited {:>8.0}  {}", x.1, x.1 / n as f64, x.2, x.0); }
    }
    let nt = ticks as f64;
    eprintln!("layer (main thread): {:?} (sections, bytes)", horizon_ecs_proto::pathsearch::layer_size());
    println!(
        "scene={scene} n={n} threads={threads} ticks={ticks} warm={warm} players={npl} spacing={spacing} | wall ms/tick: median={:.3} p99={:.3} mean={mean:.3} max={:.3} | instr/tick={:.0} median={} instr/entity/tick={:.1} | rss_MB={} rss_before_window_MB={} hwm_MB={} rss_empty_MB={} | build_s={build_s:.1} | entities {before}->{after} animals {animals0}->{animals1} asleep={asleep} awake-ish idle={idle}/{total} following={following} moving={moving} | checksum={h:016x}",
        q(0.5),
        q(0.99),
        q(1.0),
        instr_total as f64 / nt,
        instr_ticks[instr_ticks.len() / 2],
        instr_total as f64 / nt / n as f64,
        status("VmRSS:") / 1024,
        ws / 1024,
        status("VmHWM:").max(hs) / 1024,
        rss0 / 1024,
    );
    if scene == "mix" {
        let z = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
        println!("zone stats at end: slept={} woke={}", z.slept, z.woke);
    }
}
