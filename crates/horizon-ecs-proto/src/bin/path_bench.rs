//! `path-bench CASE ITERS` (CASE: stroll | chase | wall), `HORIZON_TEST_RESOURCES` (and `HORIZON_SPECIES`): the real
//! `Finder` alone, from the walker of a species, over a slab: instructions per search, and what it found.
use myriade_engine_lib::mobs::path::{Evaluator, Finder, PathMob};
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
    if case == "blk" {
        let mut r = Rng(7);
        let cells: Vec<(i32, i32, i32)> = (0..4096).map(|_| ((r.range(-40.0, 40.0)) as i32, (r.range(-2.0, 3.0)) as i32, (r.range(-40.0, 40.0)) as i32)).collect();
        let c = Counter::new();
        let mut acc = 0u64;
        let c0 = c.read();
        for _ in 0..200 { for &(x, y, z) in &cells { acc = acc.wrapping_add(mw.world.block(x, y, z).0 as u64); } }
        let per_block = (c.read() - c0) / (200 * 4096);
        let c0 = c.read();
        for _ in 0..200 { for &(x, y, z) in &cells { acc = acc.wrapping_add(mw.data.state(mw.world.block(x, y, z)).top as u64); } }
        let per_state = (c.read() - c0) / (200 * 4096);
        let c0 = c.read();
        for _ in 0..200 { for &(x, y, z) in &cells { acc = acc.wrapping_add(mw.data.path_type(mw.world.block(x, y, z)) as u64); } }
        let per_ptype = (c.read() - c0) / (200 * 4096);
        let ch = mw.world.chunks.get(&(0, 0)).unwrap().clone();
        let c0 = c.read();
        for _ in 0..200 { for &(x, y, z) in &cells { acc = acc.wrapping_add(ch.block(x & 15, y, z & 15).0 as u64); } }
        let per_chunk = (c.read() - c0) / (200 * 4096);
        let c0 = c.read();
        for _ in 0..200 { for &(x, _, z) in &cells { acc = acc.wrapping_add(mw.world.chunks.get(&(x >> 4, z >> 4)).is_some() as u64); } }
        let per_map = (c.read() - c0) / (200 * 4096);
        println!("chunk.block (memoized chunk) {per_chunk}, chunks.get only {per_map}");
        let mut m: myriade_engine_lib::fast_hash::FxHashMap<(i32, i32, i32), u32> = Default::default();
        for (i, &k) in cells.iter().take(64).enumerate() { m.insert(k, i as u32); }
        let c0 = c.read();
        for _ in 0..200 { for &k in &cells { acc = acc.wrapping_add(*m.get(&k).unwrap_or(&0) as u64); } }
        let per_hash = (c.read() - c0) / (200 * 4096);
        println!("instr per: world.block {per_block}, +data.state {per_state}, +data.path_type {per_ptype}, FxHashMap get {per_hash} ({acc})");
        return;
    }
    let mut r = Rng(0x1234_5678_9abc_def1);
    let (mut ok, mut nodes, mut instr) = (0usize, 0usize, 0u64);
    let counter = Counter::new();
    let own = std::env::var_os("OWN").is_some();
    if std::env::var_os("NOLAYER").is_some() { horizon_ecs_proto::pathsearch::LAYER_ON.store(false, std::sync::atomic::Ordering::Relaxed); }
    if std::env::var_os("PATHCOUNT").is_some() { horizon_ecs_proto::pathsearch::ON.store(true, std::sync::atomic::Ordering::Relaxed); }
    if let Some(m) = std::env::var("SECMASK").ok().and_then(|v| v.parse::<u32>().ok()) { horizon_ecs_proto::pathsearch::MASK.store(m, std::sync::atomic::Ordering::Relaxed); for i in 0..16 { if m & (1 << i) != 0 { horizon_ecs_proto::pathsearch::set_ovh(i); } } }
    let mut sum = 0u64;
    let passes = if std::env::var_os("WARM").is_some() { 2 } else { 1 };
    for pass in 0..passes {
    if pass > 0 {
        r = Rng(0x1234_5678_9abc_def1);
        (ok, nodes, instr, sum) = (0, 0, 0, 0);
        for c in horizon_ecs_proto::pathsearch::C.iter().chain(horizon_ecs_proto::pathsearch::ACC.iter()).chain(horizon_ecs_proto::pathsearch::CNT.iter()) { c.store(0, std::sync::atomic::Ordering::Relaxed); }
    }
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
        let max_len = sp.follow_range.max(16.0);
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
    }
    if let Some(m) = std::env::var("SECMASK").ok().and_then(|v| v.parse::<u32>().ok()) {
        use horizon_ecs_proto::pathsearch as ps;
        let opened = ps::C[0].load(std::sync::atomic::Ordering::Relaxed).max(1) as f64;
        for (i, n) in ps::SEC.iter().enumerate() {
            if m & (1 << i) == 0 { continue; }
            let (v, c) = (ps::ACC[i].load(std::sync::atomic::Ordering::Relaxed) as f64, ps::CNT[i].load(std::sync::atomic::Ordering::Relaxed) as f64);
            println!("  SEC {n:>32}: calls/node {:>6.2} overhead/guard {} corrected {:>8.0}/search {:>7.0}/opened node", c / opened, ps::ovh(i), (v - c * ps::ovh(i) as f64) / iters as f64, (v - c * ps::ovh(i) as f64) / opened);
        }
    }
    if std::env::var_os("INVAL").is_some() {
        let c = Counter::new();
        let mut r2 = Rng(99);
        let c0 = c.read();
        for _ in 0..2000 { horizon_ecs_proto::pathsearch::layer_changed(r2.range(-40.0, 40.0) as i32, r2.range(0.0, 3.0) as i32, r2.range(-40.0, 40.0) as i32); }
        println!("layer_changed: {} instr per block change (layer {:?})", (c.read() - c0) / 2000, horizon_ecs_proto::pathsearch::layer_size());
    }
    println!("layer: {:?} (sections, bytes)", horizon_ecs_proto::pathsearch::layer_size());
    println!("{case} iters={iters} follow_range={} found={ok} avg_nodes={:.1} instr/search={} checksum={sum:x}", sp.follow_range, nodes as f64 / ok.max(1) as f64, instr / iters as u64);
}
