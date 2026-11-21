//! `floor-bench SCENE [N [TICKS [WARM]]]`, `HORIZON_TEST_RESOURCES` set. Where the instructions of a moving entity go
//! (suivi/ecs-echelle.md, "Plancher"). SCENE: `orb` (N orbs drawn by a player that never collects), `baby` (N babies following 20 walking
//! adults, `baby_follow_adult`), `walk` (N cows strolling on flat ground), `rough` (the same with 10 % of the cells one block high),
//! `adults` (the 20 adults of `baby` alone: its baseline), `micro` (the sweep alone, nothing in its way).
//! Prints the per-system table of the real schedule (`profile.rs`), the path searches per tick, and the REPLAY of the sweep
//! (`SceneCollisions::resolve_in_border_with`, what `do_move` calls) on the states of the moving entities sampled every 10th tick:
//! instructions per call and per tick, to subtract from the total.
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    behaviours::{Entry, baby_follow_adult},
    cow::{ActiveUntil, Body, Bounds, Contact, Herd, Level, Moving, Vel, spawn_species, species},
    goals::{self, Entered, Online, Player},
    profile::{self, Counter},
};

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

fn sweep_cost(herd: &Herd, c: &Counter, boxes: &[(Aabb, Vec3, f32, bool)]) -> (u64, u64) {
    let level = herd.world.resource::<Level>();
    let mut total = 0;
    for &(b, v, step, on_ground) in boxes {
        let scene = horizon_client_render::collisions::SceneCollisions { world: level.mw.0.world, binding: level.mw.0.collision, subject: None };
        let c0 = c.read();
        let r = level.occ.resolve(scene, b, v, step, on_ground);
        total += c.read() - c0;
        std::hint::black_box(r.ok());
    }
    (total, boxes.len() as u64)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let scene = a.get(1).map_or("orb", String::as_str);
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let ticks: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    let warm: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(200);
    let counter = Counter::new();
    let mut rng = Rng(0x9e37_79b9);
    horizon_ecs_proto::bench_support::install_dummies(0);
    let mut herd;
    let mut units = n; // the entities the per-entity figures divide by
    match scene {
        "micro" => {
            herd = Herd::with_slab_awake(30, true, false);
            let level = herd.world.resource::<Level>();
            let mw = level.mw.0;
            let occ = level.occ;
            for (label, y, ground, mv, w, h, step) in [
                ("cow in free air, moving 0.1 x 0.1 z, falling", 20.0, false, Vec3::new(0.1, -0.0784, 0.1), 0.9, 1.4, 0.6),
                ("cow on the floor, walking 0.1 x (floor under it, nothing ahead)", 1.0, true, Vec3::new(0.1, -0.0784, 0.0), 0.9, 1.4, 0.6),
                ("cow on the floor, walking 0.1 x 0.1 z", 1.0, true, Vec3::new(0.1, -0.0784, 0.1), 0.9, 1.4, 0.6),
                ("orb in free air, moving 0.05", 20.0, false, Vec3::new(0.05, -0.03, 0.05), 0.5, 0.5, 0.0),
                ("orb on the floor, moving 0.05", 1.0, true, Vec3::new(0.05, -0.03, 0.05), 0.5, 0.5, 0.0),
            ] {
                let mut total = 0u64;
                let mut total_ref = 0u64;
                let calls = 20_000u64;
                for i in 0..calls {
                    let (x, z) = (-10.0 + (i % 100) as f64 * 0.2, -10.0 + (i / 100 % 100) as f64 * 0.2);
                    let b = Aabb::new(Vec3::new(x - w / 2.0, y, z - w / 2.0), Vec3::new(x + w / 2.0, y + h, z + w / 2.0));
                    let scene = horizon_client_render::collisions::SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
                    let c0 = counter.read();
                    let r = occ.resolve(scene, b, mv, step, ground);
                    total += counter.read() - c0;
                    std::hint::black_box(r.ok());
                    let scene = horizon_client_render::collisions::SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
                    let c0 = counter.read();
                    let r2 = scene.resolve_in_border_with(b, mv, step, ground, None, &[]);
                    total_ref += counter.read() - c0;
                    std::hint::black_box(r2.ok());
                }
                println!("micro sweep: {label}: {:.0} instr per call with the masks (Occupancy::resolve, what do_move calls), {:.0} by the kernel alone (resolve_in_border_with)", total as f64 / calls as f64, total_ref as f64 / calls as f64);
            }
            let mut total = 0u64;
            for i in 0..20_000u64 {
                let (x, z) = (-10.0 + (i % 100) as f64 * 0.2, -10.0 + (i / 100 % 100) as f64 * 0.2);
                let b = Aabb::new(Vec3::new(x - 0.25, 20.0, z - 0.25), Vec3::new(x + 0.25, 20.5, z + 0.25));
                let c0 = counter.read();
                std::hint::black_box(occ.no_collision(horizon_client_render::collisions::SceneCollisions { world: mw.world, binding: mw.collision, subject: None }, b));
                total += counter.read() - c0;
            }
            println!("micro no_collision(orb box, free air): {:.0} instr per call", total as f64 / 20_000.0);
            return;
        }
        "orb" => {
            let np: usize = std::env::var("PLAYERS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
            let k = (np as f64).sqrt().ceil() as usize;
            let half = (((k as f64) * 24.0 / 2.0) as i32 + 20).max(30);
            herd = Herd::with_slab_awake(half, true, false);
            herd.world.insert_resource(horizon_ecs_proto::orb::NoXp);
            let spot = |i: usize| (-f64::from(half) + 24.0 * ((i % k) as f64 + 1.0), -f64::from(half) + 24.0 * ((i / k) as f64 + 1.0));
            for i in 0..np {
                let (x, z) = spot(i);
                herd.world.resource_mut::<Online>().set(Player { id: i as u32 + 1, x, y: 1.0, z, main: 0, off: 0, ..Default::default() });
            }
            herd.set_players((0..np).map(spot).collect());
            for i in 0..n {
                let (cx, cz) = spot(i % np);
                let (r, a) = (7.0 * rng.unit().sqrt(), rng.unit() * std::f64::consts::TAU);
                let at = Vec3::new(cx + r * a.cos(), 1.5 + rng.unit() * 1.5, cz + r * a.sin());
                horizon_ecs_proto::orb::spawn_orb(&mut herd.world, 1 + i as i32, 1, at, Vec3::ZERO);
            }
        }
        "baby" | "adults" | "walk" | "rough" => {
            let (adults, per) = (20usize, n / 20);
            if scene == "walk" || scene == "rough" {
                herd = Herd::new_awake(n, 6.0, true, false, 42);
            } else {
                herd = Herd::with_slab_awake(100, true, false);
            }
            herd.world.insert_resource(horizon_ecs_proto::orb::NoXp);
            if scene == "walk" || scene == "rough" {
                // Awake for good, as the tests keep an animal (`ecs_with`): the stroll goal runs without a player near.
                let all: Vec<Entity> = herd.world.query_filtered::<Entity, With<horizon_ecs_proto::cow::Stroll>>().iter(&herd.world).collect();
                for e in all {
                    herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
                    herd.world.resource_mut::<Entered>().0.push(e);
                }
            }
            if let Some(b) = std::env::var("BABIES").ok().and_then(|v| v.parse::<usize>().ok()) {
                // `BABIES=B`: B of the cows are babies of a million ticks (the `Babies` list of `follow_parent::cell_moves` holds B).
                let all: Vec<Entity> = herd.world.query_filtered::<Entity, With<horizon_ecs_proto::cow::Stroll>>().iter(&herd.world).collect();
                for &e in all.iter().take(b) {
                    horizon_ecs_proto::behaviours::follow_parent::make_baby(&mut herd.world, e, 1_000_000_000);
                }
            }
            if scene == "rough" {
                let half = herd.world.resource::<Level>().half;
                let side = (n as f64).sqrt().ceil() as i32;
                let o = -f64::from(side) * 6.0 / 2.0;
                let mut stones = 0;
                for x in -half + 2..half - 1 {
                    for z in -half + 2..half - 1 {
                        // Not at the cows' starting cells (a cow starts at o + k * 6 + 0.5).
                        let near = |v: i32, o: f64| ((f64::from(v) - o - 0.5).rem_euclid(6.0)).min(6.0 - (f64::from(v) - o - 0.5).rem_euclid(6.0)) < 1.6;
                        if !(near(x, o) && near(z, o)) && rng.unit() < 0.10 {
                            herd.set_block(x, 1, z, "minecraft:stone");
                            stones += 1;
                        }
                    }
                }
                eprintln!("rough: {stones} stones one block high");
            }
            if scene == "baby" || scene == "adults" {
                let mut at = Vec::new();
                let centres: Vec<(f64, f64)> = (0..adults).map(|i| (-70.0 + 35.0 * (i % 5) as f64, -60.0 + 30.0 * (i / 5) as f64)).collect();
                for &c in &centres {
                    at.push(c);
                }
                herd.populate(&at, 3);
                if scene == "baby" {
                    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&baby_follow_adult::MODULE, 4, &baby_follow_adult::Params::DEFAULT)]));
                    let mut sp = *species();
                    sp.goals = horizon_ecs_proto::species::Config { entries };
                    goals::set_lazy_look(&mut herd.world, false);
                    for &(cx, cz) in &centres {
                        for _ in 0..per {
                            let (r, ang) = (6.0 + 8.0 * rng.unit(), rng.unit() * std::f64::consts::TAU);
                            let e = spawn_species(&mut herd.world, &sp, cx + r * ang.cos(), cz + r * ang.sin(), rng.next());
                            herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
                            herd.world.resource_mut::<Entered>().0.push(e);
                            horizon_ecs_proto::behaviours::follow_parent::make_baby(&mut herd.world, e, 1_000_000_000);
                        }
                    }
                    units = adults * per;
                } else {
                    units = adults;
                }
            }
        }
        _ => panic!("orb | baby | adults | walk | rough | micro"),
    }
    for kind in ["minecraft:cow", "minecraft:armadillo"] {
        let _ = horizon_ecs_proto::loot::roll(&mut herd.world, kind, false, false, 1);
    }
    let shared = profile::instrument(&mut herd);
    let (mut sweep_instr, mut sweep_calls, mut samples) = (0u64, 0u64, 0u64);
    let finds0 = goals::FINDS.load(std::sync::atomic::Ordering::Relaxed);
    let mut finds_at_warm = finds0;
    let mut total = 0u64;
    let mut movers_sum = 0u64;
    for t in 0..(warm + ticks) {
        if t == warm {
            profile::reset(&shared);
            total = 0;
            finds_at_warm = goals::FINDS.load(std::sync::atomic::Ordering::Relaxed);
        }
        let c0 = counter.read();
        herd.tick();
        let d = counter.read() - c0;
        if t >= warm {
            total += d;
            if t % 10 == 0 {
                let boxes: Vec<(Aabb, Vec3, f32, bool)> = herd.world.query_filtered::<(&Bounds, &Vel, &Body, &Contact), With<Moving>>().iter(&herd.world).map(|(b, v, body, c)| (b.0, v.0, body.step_height, c.on_ground)).collect();
                let (i, k) = sweep_cost(&herd, &counter, &boxes);
                sweep_instr += i;
                sweep_calls += k;
                movers_sum += boxes.len() as u64;
                samples += 1;
            }
        }
    }
    if scene == "baby" {
        // Replay of `nearest_adult` (baby_follow_adult.rs) on the babies as they stand, in three parts: the spatial query, the
        // loop over its candidates (components read, no sight line), and the sight lines of the candidates that pass.
        use horizon_ecs_proto::cow::{Pos, SpeciesId};
        let w = &herd.world;
        let babies: Vec<Entity> = w.iter_entities().filter(|e| e.contains::<horizon_ecs_proto::cow::GrowAt>()).map(|e| e.id()).collect();
        let mw = w.resource::<Level>().mw.0;
        let (mut qa, mut qb, mut qc, mut cand, mut passed) = (0u64, 0u64, 0u64, 0u64, 0u64);
        for &e in &babies {
            let pos = w.get::<Pos>(e).unwrap().0;
            let sp = w.get::<SpeciesId>(e).unwrap().0;
            let eye = w.get::<horizon_ecs_proto::targeting::Eye>(e).unwrap().0;
            let c0 = counter.read();
            let mut near = Vec::new();
            w.resource::<horizon_ecs_proto::cow::Index>().for_radius(pos.x, pos.z, 16.0, |c| near.push(c));
            qa += counter.read() - c0;
            cand += near.len() as u64;
            let c0 = counter.read();
            let mut ok = Vec::new();
            for &c in &near {
                if c == e || w.get::<horizon_ecs_proto::cow::GrowAt>(c).is_some() || w.get::<SpeciesId>(c).map(|s| s.0) != Some(sp) {
                    continue;
                }
                ok.push(c);
            }
            qb += counter.read() - c0;
            passed += ok.len() as u64;
            let c0 = counter.read();
            for &c in &ok {
                let cp = w.get::<Pos>(c).unwrap().0;
                let ce = w.get::<horizon_ecs_proto::targeting::Eye>(c).unwrap().0;
                std::hint::black_box(mw.line_of_sight_for(Some(pos.y), Vec3::new(pos.x, pos.y + eye, pos.z), Vec3::new(cp.x, cp.y + ce, cp.z)));
            }
            qc += counter.read() - c0;
        }
        let b = babies.len().max(1) as f64;
        println!(
            "nearest_adult replay over {} babies: spatial query {:.0} instr, candidate loop {:.0}, sight lines {:.0} (candidates {:.1} per baby, adults passing {:.2}); total {:.0} per call",
            babies.len(), qa as f64 / b, qb as f64 / b, qc as f64 / b, cand as f64 / b, passed as f64 / b, (qa + qb + qc) as f64 / b
        );
    }
    let finds = goals::FINDS.load(std::sync::atomic::Ordering::Relaxed) - finds_at_warm;
    let r = shared.lock().unwrap();
    let t = r.ticks.max(1) as f64;
    let per_tick = total as f64 / t;
    println!("scene={scene} n={n} units={units} ticks={} instr/tick={per_tick:.0} instr/unit-tick={:.0}", r.ticks, per_tick / units as f64);
    let mut rows: Vec<_> = r.rows.iter().map(|x| (x.name.rsplit("::").next().unwrap_or(&x.name).to_string(), (x.run + x.apply) as f64 / t, x.visited as f64 / t)).collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!("{:<24} {:>12} {:>11} {:>10} {:>7}", "system", "instr/tick", "per unit", "visited", "share");
    for (name, per, v) in rows.iter().take(14) {
        println!("{name:<24} {per:>12.0} {:>11.1} {v:>10.1} {:>6.1}%", per / units as f64, 100.0 * per / per_tick);
    }
    let rest: f64 = rows.iter().skip(14).map(|x| x.1).sum();
    println!("{:<24} {rest:>12.0} {:>11.1} {:>10} {:>6.1}%", "(the others)", rest / units as f64, "", 100.0 * rest / per_tick);
    for r in r.rows.iter().filter(|x| x.name.ends_with("cell_moves")) {
        println!("ROW {} instr/tick {:.0} visited(moved)/tick {:.1}", r.name, (r.run + r.apply) as f64 / t, r.visited as f64 / t);
    }
    if samples > 0 {
        let calls = sweep_calls as f64 / samples as f64;
        let per_call = sweep_instr as f64 / sweep_calls.max(1) as f64;
        println!(
            "sweep replay: {calls:.0} moving entities per tick (sampled {samples} ticks), {per_call:.0} instr per resolve_in_border_with call = {:.0} instr/tick = {:.1} instr/unit-tick = {:.1} % of the tick; movers per tick (Moving) {:.0}",
            per_call * calls,
            per_call * calls / units as f64,
            100.0 * per_call * calls / per_tick,
            movers_sum as f64 / samples as f64
        );
    }
    println!("path searches (goals::FINDS) per tick: {:.2}; instr per search not separated here (see the `pathfind` row)", finds as f64 / t);
    let (idle, following, moving, tot) = herd.counts();
    println!("end: idle {idle} following {following} moving {moving} total {tot}");
}
