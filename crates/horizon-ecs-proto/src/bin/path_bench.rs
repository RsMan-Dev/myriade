//! `path-bench CASE ITERS` (CASE: stroll | chase | wall), `HORIZON_TEST_RESOURCES` (and `HORIZON_SPECIES`): the real
//! `Finder` alone, from the walker of a species, over a slab: instructions per search, and what it found.
use horizon_client_lib::mobs::path::{Evaluator, Finder, PathMob};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::{build_level, species_table},
    kernel_lab::Rng,
    profile::Counter,
};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let case = a.get(1).map_or("stroll", String::as_str);
    let iters: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let (level, species, _) = build_level(60, false);
    let mw = level.mw.0;
    let name = std::env::var("HORIZON_SPECIES").unwrap_or_else(|_| "minecraft:cow".into());
    let sp = *species_table().0.iter().find(|s| s.name == name).expect("a known species");
    let _ = species;
    let mut r = Rng(0x1234_5678_9abc_def1);
    let (mut ok, mut nodes, mut instr) = (0usize, 0usize, 0u64);
    let counter = Counter::new();
    let own = std::env::var_os("OWN").is_some();
    if std::env::var_os("PATHCOUNT").is_some() { horizon_ecs_proto::pathsearch::ON.store(true, std::sync::atomic::Ordering::Relaxed); }
    let mut sum = 0u64;
    for _ in 0..iters {
        let (x, z) = (r.range(-40.0, 40.0), r.range(-40.0, 40.0));
        let reach = match case { "chase" => 30.0, "near" => 1.5, _ => 10.0 };
        let (tx, tz) = (x + r.range(-reach, reach), z + r.range(-reach, reach));
        let (tx, tz) = if case == "wall" { (60.5 + r.range(0.0, 3.0), tz.clamp(-50.0, 50.0)) } else { (tx, tz) };
        let pm = PathMob {
            position: Vec3::new(x, 1.0, z),
            width: sp.width,
            height: sp.height,
            on_ground: true,
            in_water: false,
            max_up_step: 0.6,
            max_fall_distance: 3,
            malus: sp.malus,
            can_float: sp.can_float,
            can_open_doors: sp.can_open_doors,
            can_pass_doors: sp.can_pass_doors,
            can_walk_over_fences: sp.can_walk_over_fences,
            stands_on_lava: false,
        };
        let max_len = sp.follow_range;
        let c0 = counter.read();
        let target = [[tx.floor() as i32, 1, tz.floor() as i32]];
        let visited = (max_len * 16.0).floor() as i32;
        let p = if own { horizon_ecs_proto::pathsearch::Finder::new(mw, &pm, Evaluator::Walk).find(&target, max_len, 1, visited) } else { Finder::new(mw, &pm, Evaluator::Walk).find(&target, max_len, 1, visited) };
        instr += counter.read() - c0;
        if let Some(p) = p {
            ok += 1;
            nodes += p.nodes.len();
            for n in &p.nodes {
                sum = sum.wrapping_mul(31).wrapping_add((n.x as u64) ^ ((n.y as u64) << 20) ^ ((n.z as u64) << 40) ^ n.kind as u64);
            }
        }
    }
    if std::env::var_os("PATHCOUNT").is_some() {
        let opened = horizon_ecs_proto::pathsearch::C[0].load(std::sync::atomic::Ordering::Relaxed).max(1) as f64;
        for (i, n) in horizon_ecs_proto::pathsearch::NAMES.iter().enumerate() {
            let v = horizon_ecs_proto::pathsearch::C[i].load(std::sync::atomic::Ordering::Relaxed) as f64;
            println!("  {n:>32}: {:>10.1}/search {:>8.2}/opened node", v / iters as f64, v / opened);
        }
    }
    println!("{case} iters={iters} follow_range={} found={ok} avg_nodes={:.1} instr/search={} checksum={sum:x}", sp.follow_range, nodes as f64 / ok.max(1) as f64, instr / iters as u64);
}
