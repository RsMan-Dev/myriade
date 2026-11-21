//! `mem-bench HALF N [TICKS [WARMSPACING]]`, `HORIZON_TEST_RESOURCES` set, `HORIZON_SPECIES` the species. Where the memory goes:
//! the terrain of the bench world (no entity) apart from the entities; then N entities on the same terrain (the world is `HALF`
//! blocks each way, so a bigger N is denser); bytes per entity = the slope between N and 2N; the components of the tables (size x
//! carriers), by storage; the heap that is not in a table. Allocator counts every live byte (`Counting`); RSS from /proc.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::cow::Herd;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
/// Live blocks by exact size (sizes below `SIZES`) and by power of two above: where the heap of the entities goes (`HEAP=1` prints it).
const SIZES: usize = 4096;
static BLOCKS: [std::sync::atomic::AtomicIsize; SIZES] = [const { std::sync::atomic::AtomicIsize::new(0) }; SIZES];
static BIG: [(std::sync::atomic::AtomicIsize, std::sync::atomic::AtomicIsize); 48] =
    [const { (std::sync::atomic::AtomicIsize::new(0), std::sync::atomic::AtomicIsize::new(0)) }; 48];
fn block(size: usize, d: isize) {
    if size < SIZES {
        BLOCKS[size].fetch_add(d, Relaxed);
    } else {
        let c = (usize::BITS - size.leading_zeros()) as usize;
        BIG[c].0.fetch_add(d, Relaxed);
        BIG[c].1.fetch_add(d * size as isize, Relaxed);
    }
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Relaxed);
        block(l.size(), 1);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        block(l.size(), -1);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        LIVE.fetch_add(n, Relaxed);
        LIVE.fetch_sub(l.size(), Relaxed);
        block(l.size(), -1);
        block(n, 1);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;

fn rss() -> u64 {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))).unwrap_or(0) * 1024
}
fn mb(b: usize) -> f64 {
    b as f64 / 1048576.0
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let half: i32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let ticks: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    horizon_ecs_proto::bench_support::install_dummies(0);
    let _ = horizon_ecs_proto::species::install; // keep the registry linked
    let (r0, l0) = (rss(), LIVE.load(Relaxed));
    // Terrain alone: the leaked block world of the bench (`build_level`), no `Herd`.
    let (level, _s, _k) = horizon_ecs_proto::cow::build_level(half, false);
    let (r1, l1) = (rss(), LIVE.load(Relaxed));
    drop(level);
    println!("half={half} ({} x {} blocks, {:.1} M columns)", 2 * half, 2 * half, f64::from(2 * half).powi(2) / 1e6);
    println!("process before the world: rss {:.1} MB, live {:.1} MB", mb(r0 as usize), mb(l0));
    println!("terrain (build_level, leaked): rss +{:.1} MB, allocator +{:.1} MB (chunks, collision binding, light, facts)", mb((r1 - r0) as usize), mb(l1 - l0));
    let l2 = LIVE.load(Relaxed);
    let mut herd = Herd::with_slab_awake(half, true, false);
    let (r3, l3) = (rss(), LIVE.load(Relaxed));
    println!("empty Herd on its own terrain (second terrain included): allocator +{:.1} MB, rss {:.1} MB", mb(l3 - l2), mb(r3 as usize));
    let base_live = LIVE.load(Relaxed);
    let base_blocks: Vec<isize> = BLOCKS.iter().map(|b| b.load(Relaxed)).collect();
    let base_big: Vec<(isize, isize)> = BIG.iter().map(|b| (b.0.load(Relaxed), b.1.load(Relaxed))).collect();
    let base_rss = rss();
    let side = (n as f64).sqrt().ceil() as i32;
    let spacing = f64::from(2 * half - 6) / f64::from(side);
    let at: Vec<(f64, f64)> = (0..n as i32).map(|i| (-f64::from(half) + 3.0 + f64::from(i % side) * spacing + 0.5, -f64::from(half) + 3.0 + f64::from(i / side) * spacing + 0.5)).collect();
    herd.populate(&at, 7);
    // `ACTIVE=1`: the scene of `goal_bench` (N/40 players, 30 % holding the species' food, a tenth of them walking, every 50 ticks N/100 hits and N/100
    // couples fed): what makes the entities toggle their states.
    let active = std::env::var_os("ACTIVE").is_some();
    let ents: Vec<Entity> = herd.world.query::<Entity>().iter(&herd.world).collect();
    let np = if active { (n / 40).max(1) } else { 0 };
    let k = (np as f64).sqrt().ceil() as usize;
    let span = f64::from(2 * half - 8);
    let spot = |i: usize| (-f64::from(half) + 4.0 + span * ((i % k.max(1)) as f64 + 0.5) / k.max(1) as f64, -f64::from(half) + 4.0 + span * ((i / k.max(1)) as f64 + 0.5) / k.max(1) as f64);
    let item = horizon_ecs_proto::goals::item_id(&std::env::var("ITEM").unwrap_or_else(|_| "minecraft:wheat".into()));
    for i in 0..np {
        let (x, z) = spot(i);
        herd.world.resource_mut::<horizon_ecs_proto::goals::Online>().set(horizon_ecs_proto::goals::Player { id: i as u32, x, y: 1.0, z, main: if i < np * 3 / 10 { item } else { 0 }, off: 0, ..Default::default() });
    }
    if active {
        herd.set_players((0..np).map(spot).collect());
    }
    let mut sd = 0x9e37_79b9_u64;
    let mut next = |m: usize| {
        sd ^= sd << 13;
        sd ^= sd >> 7;
        sd ^= sd << 17;
        (sd >> 11) as usize % m.max(1)
    };
    for t in 0..ticks {
        if active && t % 50 == 10 && t >= 50 {
            for _ in 0..(n / 100).max(1) {
                let e = ents[next(ents.len())];
                herd.world.resource_mut::<horizon_ecs_proto::cow::DamageQueue>().add(e, 1.0, horizon_ecs_proto::cow::Cause::Entity);
            }
            for _ in 0..(n / 100).max(1) {
                let i = next(ents.len() - 1);
                horizon_ecs_proto::goals::feed(&mut herd.world, ents[i]);
                horizon_ecs_proto::goals::feed(&mut herd.world, ents[i + 1]);
            }
        }
        herd.tick();
    }
    let live = LIVE.load(Relaxed);
    println!("RSS_SLOPE n={n} rss +{:.2} MB = {:.0} bytes per entity", mb((rss() - base_rss) as usize), (rss() - base_rss) as f64 / n as f64);
    println!("RAW n={n} bytes={}", live - base_live);
    println!("N={n} spacing {spacing:.2}: allocator +{:.2} MB over the empty world = {:.0} bytes per entity (ticks run: {ticks})", mb(live - base_live), (live - base_live) as f64 / n as f64);
    // The components of the tables.
    if std::env::var_os("HEAP").is_some() {
        let mut gain: Vec<(usize, f64)> = (0..SIZES).map(|i| (i, (BLOCKS[i].load(Relaxed) - base_blocks[i]) as f64 / n as f64)).filter(|g| g.1.abs() > 0.02).collect();
        gain.sort_by(|a, b| (b.0 as f64 * b.1).total_cmp(&(a.0 as f64 * a.1)));
        for (c, b) in BIG.iter().enumerate() {
            let (dn, db) = (b.0.load(Relaxed) - base_big[c].0, b.1.load(Relaxed) - base_big[c].1);
            if db.abs() > 8 * n as isize / 4 {
                println!("HEAP big blocks of 2^{} bytes: {dn} blocks, {:.1} bytes per entity", c - 1, db as f64 / n as f64);
            }
        }
        let small: f64 = (0..SIZES).map(|i| i as f64 * (BLOCKS[i].load(Relaxed) - base_blocks[i]) as f64 / n as f64).sum();
        println!("HEAP all blocks under {SIZES} bytes: {small:.1} bytes per entity");
        for (size, per) in gain.iter().take(14) {
            println!("HEAP blocks of {size} bytes: {per:.2} per entity = {:.0} bytes per entity", *size as f64 * per);
        }
    }
    let world = &herd.world;
    let comps = world.components();
    let mut by: HashMap<String, (usize, usize, &'static str)> = HashMap::new();
    let mut entities = 0usize;
    let mut comps_per_entity = 0usize;
    let mut archetypes = 0usize;
    for arch in world.archetypes().iter() {
        let cnt = arch.entities().len();
        if cnt == 0 {
            continue;
        }
        archetypes += 1;
        entities += cnt;
        comps_per_entity += cnt * arch.components().len();
        for id in arch.components() {
            let info = comps.get_info(*id).unwrap();
            let e = by.entry(info.name().to_string()).or_insert((info.layout().size(), 0, if matches!(info.storage_type(), bevy_ecs::component::StorageType::Table) { "table" } else { "sparse" }));
            e.1 += cnt;
        }
    }
    let mut rows: Vec<_> = by.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.0 * r.1.1));
    let total: usize = rows.iter().map(|r| r.1.0 * r.1.1).sum();
    println!("entities in archetypes {entities}, archetypes {archetypes}, components per entity {:.1}, inline component bytes {:.2} MB = {:.0} per entity", comps_per_entity as f64 / entities as f64, mb(total), total as f64 / entities as f64);
    println!("{:<64} {:>6} {:>9} {:>8} {:>10}", "component", "size", "carriers", "storage", "bytes/ent");
    for (name, (size, cnt, st)) in rows.iter().take(45) {
        println!("{:<64} {size:>6} {cnt:>9} {st:>8} {:>10.1}", name.rsplit("::").take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("::"), (size * cnt) as f64 / entities as f64);
    }
    for (name, (size, cnt, st)) in rows.iter().filter(|r| r.1.0 == 0 || r.1.1 < entities) {
        println!("  marker/partial: {name} size {size} carriers {cnt} {st}");
    }
    let zero = rows.iter().filter(|r| r.1.0 == 0).count();
    println!("components of size 0 (markers): {zero}; components with a size: {}; the tail below the 45 shown: {:.1} bytes per entity", rows.len() - zero, rows.iter().skip(45).map(|r| r.1.0 * r.1.1).sum::<usize>() as f64 / entities as f64);
    {
        let (mut tabs, mut alive, mut dead, mut dead_rows, mut rows) = (0, 0, 0, 0usize, 0usize);
        for t in world.storages().tables.iter() {
            if t.entity_capacity() == 0 {
                continue;
            }
            tabs += 1;
            rows += t.entity_capacity();
            if t.entity_count() == 0 {
                dead += 1;
                dead_rows += t.entity_capacity();
            } else {
                alive += 1;
            }
        }
        println!("TABLES allocated {tabs}: alive {alive}, DEAD (no entity, capacity kept) {dead} with {dead_rows} rows of {rows}");
    }
    let (mut live, mut cap_total) = (0usize, 0usize);
    for (i, t) in world.storages().tables.iter().enumerate() {
        if t.entity_capacity() >= 100 {
            println!("table {i}: {} entities, capacity {}, {} columns", t.entity_count(), t.entity_capacity(), t.component_count());
            live += t.entity_count() as usize;
            cap_total += t.entity_capacity();
        }
    }
    println!("tables with capacity >= 100: entities {live}, capacity {cap_total} rows ({:.2} x the entities: the rest is retained by tables that entities left)", cap_total as f64 / live.max(1) as f64);
    {
        // What the big tables differ by (the components a table has that the biggest live one has not, and the reverse): a spawn that
        // passes through a table that its entities then leave keeps that table's rows.
        use std::collections::{BTreeSet, HashMap as Map};
        let mut by_table: Map<usize, BTreeSet<String>> = Map::new();
        for arch in world.archetypes().iter() {
            let set = by_table.entry(arch.table_id().as_u32() as usize).or_default();
            for c in arch.components() {
                if let Some(info) = comps.get_info(*c) {
                    set.insert(info.name().to_string());
                }
            }
        }
        let live = world.storages().tables.iter().enumerate().filter(|(_, t)| t.entity_count() > 0).max_by_key(|(_, t)| t.entity_count()).map(|x| x.0);
        if let Some(live) = live {
            for (i, t) in world.storages().tables.iter().enumerate() {
                if i != live && t.entity_capacity() >= 100 {
                    if let (Some(a), Some(b)) = (by_table.get(&i), by_table.get(&live)) {
                        println!("table {i} (capacity {}) has, and the live table {live} has not: {:?}; the live one has, it has not: {:?}", t.entity_capacity(), a.difference(b).collect::<Vec<_>>(), b.difference(a).collect::<Vec<_>>());
                    }
                }
            }
        }
    }
    {
        // The removals of each component over the run (the toggles of the states), by name.
        let mut rem: Vec<(String, usize, &'static str)> = comps
            .iter_registered()
            .map(|(id, info)| (info.name().to_string(), world.removed_with_id(id).count(), if matches!(info.storage_type(), bevy_ecs::component::StorageType::Table) { "table" } else { "sparse" }))
            .filter(|r| r.1 > 0)
            .collect();
        rem.sort_by_key(|r| std::cmp::Reverse(r.1));
        for (name, c, st) in rem.iter().take(30) {
            println!("REMOVALS {:<56} {st:>6} {c:>9} over {ticks} ticks ({:.4} per entity-tick)", name.rsplit("::").take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("::"), *c as f64 / (n * ticks.max(1)) as f64);
        }
    }
    println!("wheel entries: {}", herd.world.resource::<horizon_ecs_proto::cow::Wheel>().len());
}
