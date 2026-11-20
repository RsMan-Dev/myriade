//! `terrain-bench [SEED [WALKERS [TICKS [THREADS [REGION]]]]]`, `HORIZON_TEST_RESOURCES` set: how often the occupancy-mask fast path
//! (`occupancy.rs`) answers on NATURAL terrain. Horizon's own generator (generation + decoration, offline) makes a REGION x REGION
//! chunk area around a position of each wanted biome; blocks are counted per class (empty / plain cube / "other", the class that sends a
//! call back to the full kernel), and walkers placed at random surface or cave positions walk with a cow's body: every collision call is
//! counted as served by the masks or returned to the kernel, and for the returned ones the "other" blocks in the box they swept.
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions, world::World};
use horizon_core::{
    geometry::{Aabb, Vec3},
    position::ChunkPos,
};
use horizon_ecs_proto::occupancy::Occupancy;
use horizon_world::{
    registry::BlockStateId,
    worldgen::generator::{GeneratorData, GeneratorWorker, OverworldGenerator, ProtoChunk},
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};

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

type Error = Box<dyn std::error::Error + Send + Sync>;

fn main() -> Result<(), Error> {
    let a: Vec<String> = std::env::args().collect();
    let seed: i64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let walkers: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(150);
    let ticks: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(400);
    let threads: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(12);
    let size: i32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(14);
    let root = std::path::PathBuf::from(std::env::var("HORIZON_TEST_RESOURCES")?);
    let assets = horizon_client_lib::assets::Assets::load(&root).map_err(|e| e.to_string())?;
    let registry = assets.registry;
    let binding = horizon_client_render::collisions::load(&root, registry).map_err(|e| e.to_string())?;
    let data = GeneratorData::load(&root.join("worldgen"), registry)?;
    let generator = OverworldGenerator::for_dimension(data.clone(), seed, "minecraft:normal", "minecraft:overworld")?;
    let occ = Occupancy::default();
    // 0 empty, 1 cube, 2 shaped (answered through the shape table), 3 hard (the kernel), 4 liquid.
    let class = |s: BlockStateId| -> u8 { occ.probe_class(&binding, s) };
    let name = |s: BlockStateId| -> String { registry.state(s).map_or("?".into(), |b| b.name.to_string()) };

    // The wanted biomes: (label, sampled y, names that match); `BIOMES=a,b` keeps the labels containing one of the words.
    let only: Vec<String> = std::env::var("BIOMES").map(|v| v.split(',').map(str::to_string).collect()).unwrap_or_default();
    let mut wanted: Vec<(&str, i32, &[&str])> = vec![
        ("plains", 70, &["minecraft:plains", "minecraft:sunflower_plains"]),
        ("forest", 70, &["minecraft:forest", "minecraft:birch_forest", "minecraft:dark_forest", "minecraft:flower_forest"]),
        ("jungle/bamboo", 70, &["minecraft:bamboo_jungle", "minecraft:jungle", "minecraft:sparse_jungle"]),
        ("snowy mountain", 110, &["minecraft:snowy_slopes", "minecraft:frozen_peaks", "minecraft:jagged_peaks", "minecraft:grove"]),
        ("snowy plains/taiga", 70, &["minecraft:snowy_plains", "minecraft:snowy_taiga", "minecraft:ice_spikes"]),
        ("desert/badlands", 70, &["minecraft:desert", "minecraft:badlands", "minecraft:eroded_badlands", "minecraft:wooded_badlands"]),
        ("swamp/mangrove", 66, &["minecraft:swamp", "minecraft:mangrove_swamp"]),
        ("dripstone caves", -20, &["minecraft:dripstone_caves"]),
        ("lush caves", -20, &["minecraft:lush_caves"]),
        ("ocean", 40, &["minecraft:ocean", "minecraft:deep_ocean", "minecraft:lukewarm_ocean", "minecraft:warm_ocean"]),
    ];
    // Where each is: a coarse grid over the world's first 5 000 blocks.
    wanted.retain(|(l, _, _)| only.is_empty() || only.iter().any(|o| l.contains(o.as_str())));
    let mut centers: Vec<(&str, i32, i32)> = Vec::new();
    {
        let mut worker = generator.worker();
        for (label, y, names) in &wanted {
            let mut found = 0;
            'scan: for r in (0..60).map(|k| k * 96) {
                for (gx, gz) in ring(r / 96, 96) {
                    let q = [[gx >> 2, y >> 2, gz >> 2]];
                    if let Some(Some(b)) = worker.noise_biomes(&q).first() {
                        if names.contains(&b.as_str()) {
                            centers.push((label, gx, gz));
                            found += 1;
                            if found >= 2 {
                                break 'scan;
                            }
                        }
                    }
                }
            }
            if found == 0 {
                eprintln!("no {label} within 5 700 blocks");
            }
        }
    }
    println!("centers: {centers:?}");
    let mut totals: BTreeMap<String, Totals> = BTreeMap::new();
    for (label, cx, cz) in &centers {
        let (x0, z0) = ((cx >> 4) - size / 2, (cz >> 4) - size / 2);
        let chunks = generate(&generator, x0, z0, size, threads)?;
        let n = size as usize;
        // Inner chunks (decorated): the margin of one chunk is not decorated and not used.
        let inner = |i: usize| -> bool {
            let (x, z) = (i % n, i / n);
            x >= 2 && z >= 2 && x < n - 2 && z < n - 2
        };
        let mut world = World::new(registry.default_state("minecraft:air").unwrap(), 0);
        let air = registry.default_state("minecraft:air").unwrap();
        for (i, c) in chunks.iter().enumerate() {
            let p = c.position();
            world.chunks.insert((p.x, p.z), Arc::new(Chunk::empty(p.x, p.z)));
            let t = &c.terrain;
            for y in t.min_y()..t.min_y() + t.height() {
                for z in 0..16 {
                    for x in 0..16 {
                        let s = t.block(x, y, z);
                        if s != air {
                            world.set(p.x * 16 + x, y, p.z * 16 + z, s).ok();
                        }
                    }
                }
            }
            let _ = i;
        }
        // Part 1: block counts per chunk, per biome of the column (at its surface), by class.
        for (i, c) in chunks.iter().enumerate() {
            if !inner(i) {
                continue;
            }
            let p = c.position();
            let t = &c.terrain;
            let mut by_name: BTreeMap<String, u64> = BTreeMap::new();
            let mut cells = [0u64; 5];
            for y in t.min_y()..t.min_y() + t.height() {
                for z in 0..16 {
                    for x in 0..16 {
                        let s = t.block(x, y, z);
                        let k = class(s);
                        cells[k as usize] += 1;
                        if k >= 2 {
                            *by_name.entry(name(s)).or_default() += 1;
                        }
                    }
                }
            }
            let tt = totals.entry((*label).to_string()).or_default();
            tt.chunks += 1;
            for k in 0..5 {
                tt.cells[k] += cells[k];
            }
            for (k, v) in by_name {
                *tt.other.entry(k).or_default() += v;
            }
            let _ = p;
        }
        // Part 2: walkers.
        let scene = || SceneCollisions { world: &world, binding: &binding, subject: None };
        let mut rng = Rng(0x9e37_79b9 ^ (*cx as u64) << 20 ^ *cz as u64);
        let (w, h, step) = (0.9_f64, 1.4_f64, 0.6_f32);
        let cave = label.contains("caves");
        let mut placed = 0;
        let mut tries = 0;
        while placed < walkers && tries < walkers * 400 {
            tries += 1;
            let i = (rng.next() % (n * n) as u64) as usize;
            if !inner(i) {
                continue;
            }
            let p = chunks[i].position();
            let (lx, lz) = ((rng.next() % 16) as i32, (rng.next() % 16) as i32);
            let (bx, bz) = (p.x * 16 + lx, p.z * 16 + lz);
            let t = &chunks[i].terrain;
            let y0 = if cave {
                -48 + (rng.next() % 90) as i32
            } else {
                t.min_y() + t.height() - 1
            };
            // The feet cell: the first free cell above a colliding one, going down from y0 (cave: going down from a random height).
            let mut feet = None;
            let mut y = y0;
            while y > t.min_y() + 1 {
                let here = class(t.block(lx, y, lz));
                let below = class(t.block(lx, y - 1, lz));
                if (here == 0 || here == 4) && below != 0 && below != 4 {
                    feet = Some(y);
                    break;
                }
                y -= 1;
            }
            let Some(fy) = feet else { continue };
            if !cave && fy < 60 && !label.contains("ocean") && !label.contains("swamp") {
                // A surface walker that landed in a cave under a cliff: not a surface position.
                continue;
            }
            let biome = chunks[i].biome_at(bx, fy, bz).to_string();
            let mut pos = Vec3::new(f64::from(bx) + 0.5, f64::from(fy), f64::from(bz) + 0.5);
            let mut heading = rng.unit() * std::f64::consts::TAU;
            let mut vy = 0.0;
            let mut on_ground = true;
            let before = (occ.fast.load(Ordering::Relaxed), occ.fallback.load(Ordering::Relaxed));
            let mut fb_blocks: BTreeMap<String, u64> = BTreeMap::new();
            let mut fb_calls = 0u64;
            let mut next_turn = 20 + (rng.next() % 40) as usize;
            for t_ in 0..ticks {
                if t_ == next_turn {
                    heading = rng.unit() * std::f64::consts::TAU;
                    next_turn += 20 + (rng.next() % 40) as usize;
                }
                vy = (vy - 0.08) * 0.98;
                let want = Vec3::new(heading.cos() * 0.1, vy, heading.sin() * 0.1);
                let b = Aabb::new(Vec3::new(pos.x - w / 2.0, pos.y, pos.z - w / 2.0), Vec3::new(pos.x + w / 2.0, pos.y + h, pos.z + w / 2.0));
                let f0 = occ.fallback.load(Ordering::Relaxed);
                let moved = occ.resolve(scene(), b, want, step, on_ground).unwrap_or(Vec3::ZERO);
                if occ.fallback.load(Ordering::Relaxed) > f0 {
                    fb_calls += 1;
                    // The "other" blocks in the swept box and its ring.
                    let (lo, hi) = (
                        [(pos.x - w / 2.0 - 1.0).floor() as i32, (pos.y - 1.0).floor() as i32, (pos.z - w / 2.0 - 1.0).floor() as i32],
                        [(pos.x + w / 2.0 + 1.0).floor() as i32, (pos.y + h + 1.0).floor() as i32, (pos.z + w / 2.0 + 1.0).floor() as i32],
                    );
                    let mut seen: Vec<String> = Vec::new();
                    for xx in lo[0]..=hi[0] {
                        for yy in lo[1]..=hi[1] {
                            for zz in lo[2]..=hi[2] {
                                let s = world.block(xx, yy, zz);
                                if class(s) == 3 {
                                    let n = name(s);
                                    if !seen.contains(&n) {
                                        seen.push(n);
                                    }
                                }
                            }
                        }
                    }
                    for n in &seen {
                        *fb_blocks.entry(n.clone()).or_default() += 1;
                    }
                    if seen.len() == 1 {
                        *fb_blocks.entry(format!("{} (alone)", seen[0])).or_default() += 1;
                    }
                }
                let hit_y = (moved.y - want.y).abs() > 1e-9;
                on_ground = hit_y && want.y < 0.0;
                if hit_y {
                    vy = 0.0;
                }
                if (moved.x - want.x).abs() > 1e-6 || (moved.z - want.z).abs() > 1e-6 {
                    heading = rng.unit() * std::f64::consts::TAU;
                }
                pos = Vec3::new(pos.x + moved.x, pos.y + moved.y, pos.z + moved.z);
                if pos.y < f64::from(t.min_y()) + 2.0 {
                    break;
                }
            }
            let after = (occ.fast.load(Ordering::Relaxed), occ.fallback.load(Ordering::Relaxed));
            let tt = totals.entry((*label).to_string()).or_default();
            tt.walkers += 1;
            tt.fast += after.0 - before.0;
            tt.fallback += after.1 - before.1;
            let _ = (biome, fb_calls);
            for (k, v) in fb_blocks {
                *tt.fb_by_block.entry(k).or_default() += v;
            }
            placed += 1;
        }
    }
    println!("\n== per biome group ({walkers} walkers x {ticks} ticks, a cow's body, 0.1 block/tick, region {size}x{size} chunks, seed {seed})");
    for (label, t) in &totals {
        let cells: u64 = t.cells.iter().sum();
        let calls = t.fast + t.fallback;
        println!(
            "{label}: chunks {} | cells: empty {:.1} %, cube {:.1} %, liquid {:.2} %, shaped (shape table) {:.3} % ({:.0}/chunk), HARD {:.4} % ({:.1}/chunk) | walkers {} calls {} served by the masks {:.1} %, returned to the kernel {:.1} %",
            t.chunks,
            100.0 * t.cells[0] as f64 / cells.max(1) as f64,
            100.0 * t.cells[1] as f64 / cells.max(1) as f64,
            100.0 * t.cells[4] as f64 / cells.max(1) as f64,
            100.0 * t.cells[2] as f64 / cells.max(1) as f64,
            t.cells[2] as f64 / t.chunks.max(1) as f64,
            100.0 * t.cells[3] as f64 / cells.max(1) as f64,
            t.cells[3] as f64 / t.chunks.max(1) as f64,
            t.walkers,
            calls,
            100.0 * t.fast as f64 / calls.max(1) as f64,
            100.0 * t.fallback as f64 / calls.max(1) as f64
        );
        let mut o: Vec<_> = t.other.iter().collect();
        o.sort_by_key(|x| std::cmp::Reverse(*x.1));
        println!("    shaped+hard cells per chunk, top: {}", o.iter().take(8).map(|(k, v)| format!("{} {:.1}", k.trim_start_matches("minecraft:"), **v as f64 / t.chunks.max(1) as f64)).collect::<Vec<_>>().join(", "));
        let mut f: Vec<_> = t.fb_by_block.iter().collect();
        f.sort_by_key(|x| std::cmp::Reverse(*x.1));
        println!("    kernel calls by block present (share of that group's calls): {}", f.iter().take(8).map(|(k, v)| format!("{} {:.1} %", k.trim_start_matches("minecraft:"), 100.0 * **v as f64 / calls.max(1) as f64)).collect::<Vec<_>>().join(", "));
    }
    Ok(())
}

#[derive(Default)]
struct Totals {
    chunks: u64,
    cells: [u64; 5],
    other: BTreeMap<String, u64>,
    walkers: u64,
    fast: u64,
    fallback: u64,
    fb_by_block: BTreeMap<String, u64>,
}

/// Grid positions at distance `r` (in steps) from the origin, in a ring.
fn ring(r: i32, step: i32) -> Vec<(i32, i32)> {
    if r == 0 {
        return vec![(0, 0)];
    }
    let mut v = Vec::new();
    for i in -r..=r {
        v.push((i * step, -r * step));
        v.push((i * step, r * step));
        if i.abs() != r {
            v.push((-r * step, i * step));
            v.push((r * step, i * step));
        }
    }
    v
}

/// `size` x `size` chunks from (x0, z0), generated and decorated (the outer ring of chunks is not decorated).
fn generate(generator: &OverworldGenerator, x0: i32, z0: i32, size: i32, threads: usize) -> Result<Vec<ProtoChunk>, Error> {
    let count = (size * size) as usize;
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<ProtoChunk>>> = (0..count).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| -> Result<(), Error> {
        let hs: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| -> Result<(), Error> {
                    let mut worker = generator.worker();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= count {
                            return Ok(());
                        }
                        let pos = ChunkPos::new(x0 + (i as i32 % size), z0 + (i as i32 / size));
                        *slots[i].lock().unwrap() = Some(worker.generate_proto(pos)?);
                    }
                })
            })
            .collect();
        hs.into_iter().try_for_each(|h| h.join().unwrap())
    })?;
    let mut chunks: Vec<Option<ProtoChunk>> = slots.into_iter().map(|m| m.into_inner().unwrap()).collect();
    for phase in 0..9 {
        let mut regions = Vec::new();
        for z in 1..size - 1 {
            for x in 1..size - 1 {
                if (x % 3) * 3 + z % 3 != phase {
                    continue;
                }
                let mut region = Vec::with_capacity(9);
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        region.push(chunks[((z + dz) * size + x + dx) as usize].take().unwrap());
                    }
                }
                regions.push(((x, z), region));
            }
        }
        let queue = Mutex::new(regions);
        let done = Mutex::new(Vec::new());
        std::thread::scope(|scope| -> Result<(), Error> {
            let hs: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| -> Result<(), Error> {
                        let mut worker: GeneratorWorker = generator.worker();
                        loop {
                            let Some((at, region)) = queue.lock().unwrap().pop() else { return Ok(()) };
                            let mut region: [ProtoChunk; 9] = region.try_into().map_err(|_| "region size")?;
                            worker.decorate(&mut region)?;
                            done.lock().unwrap().push((at, region));
                        }
                    })
                })
                .collect();
            hs.into_iter().try_for_each(|h| h.join().unwrap())
        })?;
        for ((x, z), region) in done.into_inner().unwrap() {
            for (i, chunk) in region.into_iter().enumerate() {
                let (dx, dz) = (i as i32 % 3 - 1, i as i32 / 3 - 1);
                chunks[((z + dz) * size + x + dx) as usize] = Some(chunk);
            }
        }
    }
    Ok(chunks.into_iter().map(|c| c.unwrap()).collect())
}
