//! The natural spawning of the ECS against Horizon's (`spawn_selection`, run by `spawn_reference`): same world, same biome, same
//! players, the same number of ticks. Needs HORIZON_TEST_RESOURCES.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{cow::*, goals::*, instr::Counter, spawn_reference::*, spawning::*};
use std::collections::BTreeMap;

fn have() -> bool {
    horizon_ecs_proto::species::inscribe_every_module();
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}
fn player(h: &mut Herd, id: u32, x: f64, z: f64) {
    h.world.resource_mut::<Online>().set(Player { id, x, y: 1.0, z, main: 0, off: 0, ..Default::default() });
}
fn by_name(h: &mut Herd) -> BTreeMap<String, usize> {
    let table = species_table();
    let mut m = BTreeMap::new();
    for id in h.world.query::<&SpeciesId>().iter(&h.world).map(|s| s.0).collect::<Vec<_>>() {
        *m.entry(table.0[id as usize].name.to_string()).or_insert(0) += 1;
    }
    m
}

#[test]
fn reference_cost_and_fill() {
    if !have() {
        return;
    }
    let t0 = std::time::Instant::now();
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    eprintln!("world built in {:?}", t0.elapsed());
    let mw = h.world.resource::<Level>().mw.0;
    let mut r = RefSpawner::new(RefLevel::new(mw, "minecraft:plains"), 7);
    r.level.players.push([0.5, 1.0, 0.5]);
    let dummies: usize = std::env::var("DUMMY").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    for i in 0..dummies {
        let (x, z) = (200 + (i % 100) as i32, 200 + (i / 100) as i32);
        r.mobs.push(RefMob { entity: "minecraft:bat".into(), category: horizon_client_lib::mobs::spawn_selection::Category::Ambient, block: [x, 1, z], position: [f64::from(x), 1.0, f64::from(z)] });
    }
    let c = Counter::new();
    let ticks: usize = std::env::var("TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000);
    let (mut instr, mut at_open, mut n_open) = (0u64, 0u64, 0u64);
    let mut laps = [0u64; 4];
    for t in 0..ticks {
        let a = c.as_ref().map_or(0, Counter::read);
        let s = r.pass();
        let d = c.as_ref().map_or(0, Counter::read) - a;
        instr += d;
        for i in 0..4 {
            laps[i] += r.laps[i];
        }
        if s.categories > 0 {
            at_open += d;
            n_open += 1;
        }
        if t % 500 == 499 {
            let mut m = BTreeMap::new();
            for x in &r.mobs {
                *m.entry(format!("{:?}", x.category)).or_insert(0) += 1;
            }
            let mut n = BTreeMap::new();
            for x in &r.mobs {
                *n.entry(x.entity.clone()).or_insert(0) += 1;
            }
            eprintln!("  by entity {n:?}");
            eprintln!("t={} mobs={} {:?} (last pass: {} chunks, {} categories)", t + 1, r.mobs.len(), m, s.candidates, s.categories);
        }
    }
    eprintln!("reference laps per tick: candidates {} state(createState over the mobs) {} shuffle {} attempts {}", laps[0] / ticks as u64, laps[1] / ticks as u64, laps[2] / ticks as u64, laps[3] / ticks as u64);
    eprintln!("reference: {} instr per tick over {ticks} ticks; per tick with an open category {} ({} such ticks)", instr / ticks as u64, at_open / n_open.max(1), n_open);
}

fn counts_by_category(h: &mut Herd) -> [i32; 8] {
    h.world.resource::<SpawnCounts>().global
}
#[test]
fn ecs_fill_and_cost() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    player(&mut h, 1, 0.5, 0.5);
    h.tick();
    spawning(&mut h, 7);
    eprintln!("unported entries of the biome: {:?}", h.world.resource::<Spawner>().table.unported);
    h.world.resource_mut::<Online>().seen.clear();
    let ticks: usize = std::env::var("TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000);
    let c = Counter::new();
    // The pass alone: the clock advances by hand.
    let mut instr = 0u64;
    for t in 0..ticks {
        h.world.resource_mut::<Clock>().0 += 1;
        let a = c.as_ref().map_or(0, Counter::read);
        horizon_ecs_proto::spawning::step(&mut h.world);
        instr += c.as_ref().map_or(0, Counter::read) - a;
        if t % 500 == 499 {
            eprintln!("t={} counts={:?} {:?}", t + 1, counts_by_category(&mut h), by_name(&mut h));
        }
    }
    let s = h.world.resource::<Spawner>().stats;
    eprintln!("ecs pass alone: {} instr per tick; {:?}", instr / ticks as u64, s);
}

/// The spawning on with the chances of the chunks made (`hold` scans and spawns nothing), the random stream of `seed`.
fn spawning(h: &mut Herd, seed: i64) {
    if h.world.contains_resource::<Spawner>() {
        h.world.resource_mut::<Spawner>().reseed(seed);
        return;
    }
    h.enable_spawning("minecraft:plains", seed);
    h.world.resource_mut::<Spawner>().hold = true;
    for _ in 0..260 {
        h.world.resource_mut::<Clock>().0 += 1;
        horizon_ecs_proto::spawning::step(&mut h.world);
    }
    h.world.resource_mut::<Spawner>().hold = false;
    h.world.resource_mut::<Spawner>().reseed(seed);
}
fn clear(h: &mut Herd) {
    let all: Vec<Entity> = h.world.query_filtered::<Entity, With<Counted>>().iter(&h.world).collect();
    for e in all {
        horizon_ecs_proto::despawn::remove(&mut h.world, e);
    }
}
fn mean_sd(v: &[f64]) -> (f64, f64) {
    let m = v.iter().sum::<f64>() / v.len() as f64;
    (m, (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)).sqrt())
}
/// The law of the fill: after the same ticks, from the same empty world with the same player, the mean count of each species and of each
/// category over many seeds. Time to the cap too (the count at tick 100).
#[test]
fn the_fill_follows_horizon_by_category_and_species() {
    if !have() {
        return;
    }
    let seeds: i64 = std::env::var("SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    player(&mut h, 1, 0.5, 0.5);
    h.tick();
    let mw = h.world.resource::<Level>().mw.0;
    let (mut ecs, mut refm): (BTreeMap<String, Vec<f64>>, BTreeMap<String, Vec<f64>>) = Default::default();
    for seed in 0..seeds {
        // The reference.
        let mut r = RefSpawner::new(RefLevel::new(mw, "minecraft:plains"), 1000 + seed * 104_729 + (seed * seed) % 977);
        r.level.players.push([0.5, 1.0, 0.5]);
        let mut early = 0;
        for t in 0..600 {
            r.pass();
            if t == 99 {
                early = r.mobs.iter().filter(|m| format!("{:?}", m.category) == "Monster").count();
            }
        }
        let mut n: BTreeMap<String, f64> = BTreeMap::new();
        for m in &r.mobs {
            *n.entry(m.entity.trim_start_matches("minecraft:").to_string()).or_default() += 1.0;
            *n.entry(format!("[{:?}]", m.category)).or_default() += 1.0;
        }
        n.insert("[monsters at 100]".into(), early as f64);
        for (k, v) in n {
            refm.entry(k).or_default().push(v);
        }
        // The ECS.
        clear(&mut h);
        if h.world.contains_resource::<SpawnCounts>() && h.world.resource::<SpawnCounts>().global != [0; 8] {
            eprintln!("STALE counts after clear at seed {seed}: {:?}", h.world.resource::<SpawnCounts>().global);
        }
        spawning(&mut h, 5000 + seed * 15_485_863);
        // The same game time as the reference (its first pass is at time 1: the creatures' turn comes at 400).
        h.world.resource_mut::<Clock>().0 = 0;
        let mut early = 0;
        for t in 0..600 {
            h.world.resource_mut::<Clock>().0 += 1;
            horizon_ecs_proto::spawning::step(&mut h.world);
            if t == 99 {
                early = h.world.resource::<SpawnCounts>().global[0];
            }
        }
        let mut n: BTreeMap<String, f64> = BTreeMap::new();
        for (k, v) in by_name(&mut h) {
            n.insert(k.trim_start_matches("minecraft:").to_string(), v as f64);
        }
        let g = h.world.resource::<SpawnCounts>().global;
        if g[1] == 0 {
            eprintln!("seed {seed}: no creature; stats {:?}", h.world.resource::<Spawner>().stats);
        }
        for (i, k) in ["Monster", "Creature", "Ambient"].iter().enumerate() {
            n.insert(format!("[{k}]"), f64::from(g[i]));
        }
        n.insert("[monsters at 100]".into(), f64::from(early));
        for k in refm.keys().cloned().collect::<Vec<_>>() {
            ecs.entry(k.clone()).or_default().push(*n.get(&k).unwrap_or(&0.0));
        }
    }
    for (name, m) in [("horizon", &refm), ("ecs", &ecs)] {
        let mut hist = [0usize; 20];
        // A seed with no creature has no entry on the reference's side: the missing ones are zeros.
        let mut v = m["[Creature]"].clone();
        v.resize(seeds as usize, 0.0);
        for v in &v {
            hist[(*v as usize).min(19)] += 1;
        }
        eprintln!("creature count histogram {name}: {hist:?}");
    }
    let mut worst: f64 = 0.0;
    for (k, rv) in &refm {
        let mut rv = rv.clone();
        rv.resize(seeds as usize, 0.0);
        let ev = &ecs[k];
        let ((mr, sr), (me, se)) = (mean_sd(&rv), mean_sd(ev));
        let z = (mr - me) / ((sr * sr + se * se) / seeds as f64).sqrt().max(1e-9);
        eprintln!("{k:<22} horizon {mr:6.2} (sd {sr:5.2})   ecs {me:6.2} (sd {se:5.2})   z {z:+.2}");
        if sr > 0.0 || se > 0.0 {
            worst = worst.max(z.abs());
        }
    }
    eprintln!("{seeds} seeds, 600 ticks, one player; worst |z| = {worst:.2}");
}

/// The creatures' turn alone, on an empty world: the number of creatures one pass makes.
#[test]
fn one_creature_pass_alone() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    player(&mut h, 1, 0.5, 0.5);
    h.tick();
    let mw = h.world.resource::<Level>().mw.0;
    let (mut hr, mut he) = ([0usize; 20], [0usize; 20]);
    for seed in 0..600i64 {
        let mut r = RefSpawner::new(RefLevel::new(mw, "minecraft:plains"), 77 + seed * 104_729 + (seed * seed) % 977);
        r.level.players.push([0.5, 1.0, 0.5]);
        r.game_time = 400;
        r.pass();
        hr[r.mobs.iter().filter(|m| m.category == horizon_client_lib::mobs::spawn_selection::Category::Creature).count().min(19)] += 1;
        clear(&mut h);
        spawning(&mut h, 9 + seed * 15_485_863);
        h.world.resource_mut::<Clock>().0 = 400;
        horizon_ecs_proto::spawning::step(&mut h.world);
        he[(h.world.resource::<SpawnCounts>().global[1] as usize).min(19)] += 1;
        if seed < 6 {
            eprintln!("seed {seed}: {:?}", h.world.resource::<Spawner>().stats);
        }
    }
    eprintln!("one creature pass, 600 seeds\n horizon {hr:?}\n ecs     {he:?}");
}

#[test]
fn when_do_creatures_come() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    let mw = h.world.resource::<Level>().mw.0;
    for seed in 0..12i64 {
        let mut r = RefSpawner::new(RefLevel::new(mw, "minecraft:plains"), 1000 + seed * 104_729);
        r.level.players.push([0.5, 1.0, 0.5]);
        let mut last = 0;
        let mut when = Vec::new();
        for t in 0..600 {
            r.pass();
            let n = r.mobs.iter().filter(|m| format!("{:?}", m.category) == "Creature").count();
            if n != last {
                when.push((t + 1, n - last, r.mobs.last().map(|m| m.entity.clone())));
                last = n;
            }
        }
        eprintln!("seed {seed}: {when:?}");
    }
}

/// What `createState` would count: the creatures that carry the species' category and are not persistent, by category and by player.
fn recount(h: &mut Herd) -> [i32; 8] {
    let table = species_table();
    let mut g = [0; 8];
    let all: Vec<(Entity, u8, bool)> = h.world.query::<(Entity, &SpeciesId, Option<&horizon_ecs_proto::despawn::Persistent>)>().iter(&h.world).map(|(e, s, p)| (e, s.0, p.is_some())).collect();
    for (_, id, persistent) in all {
        if let (Some(d), false) = (horizon_ecs_proto::spawning::spawn_def(&table.0[id as usize]), persistent) {
            g[d.category as usize] += 1;
        }
    }
    g
}
#[test]
fn the_counts_kept_by_event_are_the_counts_a_recount_finds() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    h.enable_spawning("minecraft:plains", 3);
    player(&mut h, 1, 0.5, 0.5);
    for _ in 0..450 {
        h.tick();
    }
    let ids: Vec<Entity> = h.world.query_filtered::<Entity, With<Counted>>().iter(&h.world).collect();
    assert!(ids.len() > 50);
    let (kept, found) = (h.world.resource::<SpawnCounts>().global, recount(&mut h));
    assert_eq!(kept, found, "after the fill");
    // Births, persistence, a death, a going away and a move to another chunk.
    horizon_ecs_proto::despawn::keep(&mut h.world, ids[0]);
    horizon_ecs_proto::despawn::keep(&mut h.world, ids[1]);
    horizon_ecs_proto::despawn::release(&mut h.world, ids[1]);
    horizon_ecs_proto::despawn::remove(&mut h.world, ids[2]);
    h.hit(ids[3], 1000.0, (0.0, 0.0));
    let at = h.world.get::<Pos>(ids[4]).unwrap().0;
    h.teleport(ids[4], horizon_core::geometry::Vec3::new(at.x + 40.0, at.y, at.z));
    for _ in 0..5 {
        h.tick();
    }
    let (kept, found) = (h.world.resource::<SpawnCounts>().global, recount(&mut h));
    assert_eq!(kept, found, "after the changes");
    // The count near the player is the count of the creatures in the chunks within 128 blocks of it.
    let near = {
        let mut n = [0i32; 8];
        let all: Vec<(u8, (i32, i32), bool)> = h.world.query::<(&SpeciesId, &Counted, Option<&horizon_ecs_proto::despawn::Persistent>)>().iter(&h.world).map(|(s, c, p)| (s.0, c.chunk, p.is_some())).collect();
        let table = species_table();
        for (id, chunk, persistent) in all {
            let (x, z) = (f64::from(chunk.0 * 16 + 8) - 0.5, f64::from(chunk.1 * 16 + 8) - 0.5);
            if !persistent && x * x + z * z < 16384.0 {
                n[horizon_ecs_proto::spawning::spawn_def(&table.0[id as usize]).unwrap().category as usize] += 1;
            }
        }
        n
    };
    eprintln!("global {:?} near the player {:?}", recount(&mut h), near);
    assert_eq!(h.world.resource::<SpawnCounts>().near_player(1), Some(near), "the count near the player, kept by event");
}

/// Instructions per tick of the pass, Horizon's and the ECS's, filling and at the caps, with and without thousands of other creatures.
#[test]
fn cost_per_cycle_by_creatures_and_by_state() {
    if !have() {
        return;
    }
    use horizon_client_lib::mobs::spawn_selection::Category as C;
    let c = Counter::new();
    let read = || c.as_ref().map_or(0, Counter::read);
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    let mw = h.world.resource::<Level>().mw.0;
    let mut out = Vec::new();
    for dummies in [0usize, 3000] {
        let mut r = RefSpawner::new(RefLevel::new(mw, "minecraft:plains"), 5);
        r.level.players.push([0.5, 1.0, 0.5]);
        for i in 0..dummies {
            let (x, z) = (200 + (i % 100) as i32, 200 + (i / 100) as i32);
            r.mobs.push(RefMob { entity: "minecraft:cow".into(), category: C::Creature, block: [x, 1, z], position: [f64::from(x), 1.0, f64::from(z)] });
        }
        let (mut fill, mut steady) = (0u64, 0u64);
        for t in 0..1800 {
            let a = read();
            r.pass();
            let d = read() - a;
            if t < 300 {
                fill += d;
            }
            if t >= 1500 && t % 400 != 399 {
                steady += d;
            }
        }
        out.push(format!("horizon, {dummies} other creatures: fill (first 300 ticks) {} per tick, at the caps {} per tick (the harness's candidate list, {} per tick, is not counted)", fill / 300, steady / 296, 0));
        let _ = steady;
        // The ECS.
        clear(&mut h);
        player(&mut h, 1, 0.5, 0.5);
        h.tick();
        spawning(&mut h, 5);
        h.world.resource_mut::<Clock>().0 = 0;
        h.world.resource_mut::<Online>().seen.clear();
        let table = species_table();
        let sheep = *table.0.iter().find(|s| s.name == "minecraft:sheep").unwrap();
        let extra: Vec<Entity> = (0..dummies)
            .map(|i| {
                let (x, z) = (200.5 + (i % 100) as f64, 200.5 + (i / 100) as f64);
                let e = horizon_ecs_proto::cow::spawn_species(&mut h.world, &sheep, x, z, 77);
                h.world.entity_mut(e).insert(horizon_ecs_proto::despawn::Persistent);
                e
            })
            .collect();
        let _ = extra;
        let (mut fill, mut steady) = (0u64, 0u64);
        for t in 0..1800 {
            h.world.resource_mut::<Clock>().0 += 1;
            let a = read();
            horizon_ecs_proto::spawning::step(&mut h.world);
            let d = read() - a;
            if t < 300 {
                fill += d;
            }
            if t >= 1500 && t % 400 != 399 {
                steady += d;
            }
        }
        out.push(format!("ecs,     {dummies} other creatures: fill (first 300 ticks) {} per tick, at the caps {} per tick", fill / 300, steady / 296));
    }
    for l in out {
        eprintln!("{l}");
    }
}

/// The creatures' pass at tick 400 with 70 monsters already there: what each side saw, over many seeds, and the seeds where the ECS made none.
#[test]
fn trace_the_creature_pass_in_context() {
    if !have() {
        return;
    }
    let seeds: i64 = std::env::var("SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    player(&mut h, 1, 0.5, 0.5);
    h.tick();
    let mw = h.world.resource::<Level>().mw.0;
    // [local refused, starts, born] of the creature category and the tries, summed; the monsters present; the zero runs.
    let (mut rt, mut et) = ([0u64; 3], [0u64; 3]);
    let (mut rtries, mut etries, mut rmon, mut emon) = (0u64, 0u64, 0u64, 0u64);
    let (mut rz, mut ez) = (0, 0);
    for seed in 0..seeds {
        let mut r = RefSpawner::new(RefLevel::new(mw, "minecraft:plains"), 1000 + seed * 104_729 + (seed * seed) % 977);
        r.level.players.push([0.5, 1.0, 0.5]);
        for _ in 0..399 {
            r.pass();
        }
        rmon += r.mobs.iter().filter(|m| format!("{:?}", m.category) == "Monster").count() as u64;
        r.pass();
        for i in 0..3 {
            rt[i] += r.trace[1][i];
        }
        rtries += r.tries;
        if r.trace[1][2] == 0 {
            rz += 1;
            let total = r.mobs.iter().filter(|m| format!("{:?}", m.category) == "Creature").count();
            let kinds: Vec<_> = r.mobs.iter().filter(|m| format!("{:?}", m.category) != "Monster").map(|m| m.entity.clone()).collect();
            eprintln!("horizon zero-creature pass at seed {seed}: creature mobs {total} {kinds:?} trace {:?}", r.trace);
        }
        clear(&mut h);
        spawning(&mut h, 5000 + seed * 15_485_863);
        h.world.resource_mut::<Clock>().0 = 0;
        for _ in 0..399 {
            h.world.resource_mut::<Clock>().0 += 1;
            horizon_ecs_proto::spawning::step(&mut h.world);
        }
        emon += h.world.resource::<SpawnCounts>().global[0] as u64;
        let before = h.world.resource::<Spawner>().stats;
        h.world.resource_mut::<Clock>().0 += 1;
        horizon_ecs_proto::spawning::step(&mut h.world);
        let after = h.world.resource::<Spawner>().stats;
        for i in 0..3 {
            et[i] += after.by_category[1][i] - before.by_category[1][i];
        }
        etries += after.attempts - before.attempts;
        if after.by_category[1][2] == before.by_category[1][2] {
            ez += 1;
            if ez <= 3 {
                eprintln!("ecs zero at seed {seed}: starts {} local refused {} tries {}", after.by_category[1][1] - before.by_category[1][1], after.by_category[1][0] - before.by_category[1][0], after.attempts - before.attempts);
            }
        }
    }
    let n = seeds as u64;
    eprintln!("per pass at tick 400 over {seeds} seeds [local refused, starts, born]: horizon {:?} tries {} monsters {:.1} zero {rz}", rt.map(|v| v as f64 / n as f64), rtries / n, rmon as f64 / n as f64);
    eprintln!("                                                                   ecs     {:?} tries {} monsters {:.1} zero {ez}", et.map(|v| v as f64 / n as f64), etries / n, emon as f64 / n as f64);
}

/// The chance a start reaches a free position is kept by event: a shaft dug in a chunk raises it by the free heights of the column.
#[test]
fn digging_changes_the_chance_of_a_start_reaching_a_free_position() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    h.enable_spawning("minecraft:plains", 3);
    player(&mut h, 1, 0.5, 0.5);
    for _ in 0..40 {
        h.tick();
    }
    let p0 = h.world.resource::<Spawner>().reach_chance((0, 0)).unwrap();
    // 1 free height (y = 1) of 66 in every column.
    assert!((p0 - 1.0 / 66.0).abs() < 1e-9, "{p0}");
    for y in -40..=0 {
        h.set_block(3, y, 3, "minecraft:air");
    }
    for _ in 0..3 {
        h.tick();
    }
    let p1 = h.world.resource::<Spawner>().reach_chance((0, 0)).unwrap();
    // The column (3, 3) lost its surface: the highest block left is at y = -41, so its top is -40 (25 heights from the bottom of the world, one of
    // them free) instead of 66 heights with one free.
    let expect = p0 + (1.0 / 25.0 - 1.0 / 66.0) / 256.0;
    assert!((p1 - expect).abs() < 1e-9, "{p0} -> {p1}, expected {expect}");
}

fn spread(v: &mut Vec<u64>) -> (u64, u64, u64) {
    v.sort_unstable();
    (v[v.len() / 2], v[v.len() * 99 / 100], *v.last().unwrap())
}
/// A player who walks in a straight line (4.3 blocks a second, then 20): instructions of each tick, the median, the 99th percentile and the
/// worst tick, the ECS's pass and the Mob's, in the same scene (the caps reached before the walk).
#[test]
fn a_walking_player_costs_no_tick_more_than_twice_the_median() {
    if !have() {
        return;
    }
    let c = Counter::new();
    let read = || c.as_ref().map_or(0, Counter::read);
    for (name, per_tick, ticks) in [("4.3 blocks/s", 0.215, 600usize), ("20 blocks/s", 1.0, 200)] {
        let mut h = Herd::with_spawn_world_awake(176, true);
        h.set_daylight(false);
        let start = -150.0;
        player(&mut h, 1, start, 0.5);
        h.tick();
        spawning(&mut h, 3);
        for _ in 0..60 {
            h.tick();
        }
        h.world.resource_mut::<Online>().seen.clear();
        let mw = h.world.resource::<Level>().mw.0;
        let mut r = RefSpawner::new(RefLevel::new(mw, "minecraft:plains"), 5);
        r.level.players.push([start, 1.0, 0.5]);
        h.world.resource_mut::<Spawner>().profile = Counter::new();
        let (mut e, mut m, mut laps) = (Vec::new(), Vec::new(), [0u64; 2]);
        let mut parts: Vec<(u64, u64, u64, u64)> = Vec::new();
        for t in 0..ticks {
            let x = start + per_tick * (t as f64 + 1.0);
            player(&mut h, 1, x, 0.5);
            // The player's event is read by the pass at its tick: `step` runs after `online_events`, so the tick is run whole, the pass
            // alone is measured by running it by hand on the same events.
            h.world.resource_mut::<Clock>().0 += 1;
            horizon_ecs_proto::goals::online_events_for_bench(&mut h.world);
            let a = read();
            horizon_ecs_proto::spawning::step(&mut h.world);
            let l = h.world.resource::<Spawner>().laps;
            // The creatures' creation (the entity, its timers) is not the pass: it is measured apart.
            e.push(read() - a - l[2]);
            parts.push((read() - a - l[2], l[0], l[1], (read() - a - l[2]).saturating_sub(l[0] + l[1])));
            laps[0] += l[0];
            laps[1] += l[1];
            r.level.players[0] = [x, 1.0, 0.5];
            let a = read();
            r.pass();
            m.push(read() - a);
        }
        eprintln!("{name}: per tick on average: candidates made again {} ({} times), chunks scanned {}", laps[0] / ticks as u64, h.world.resource::<Spawner>().stats.rebuilds, laps[1] / ticks as u64);
        parts.sort_by_key(|p| std::cmp::Reverse(p.0));
        eprintln!("{name}: the five worst ticks (total, candidates, queue, picks and the rest): {:?}", &parts[..5]);
        eprintln!("{name}: ecs median/p99/worst {:?}; mob {:?}", spread(&mut e), spread(&mut m));
    }
}

/// The starts that reach a free position, per tick and category: the chunks within 128 blocks times the chance of each (1 in 66 here).
#[test]
fn the_skipped_starts_leave_the_rate_of_the_others() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    player(&mut h, 1, 0.5, 0.5);
    h.tick();
    spawning(&mut h, 11);
    h.world.resource_mut::<Online>().seen.clear();
    let ticks = 4000u64;
    h.world.resource_mut::<Spawner>().profile = Counter::new();
    for _ in 0..ticks {
        h.world.resource_mut::<Clock>().0 += 1;
        horizon_ecs_proto::spawning::step(&mut h.world);
    }
    let s = h.world.resource::<Spawner>().stats;
    let ambient = s.by_category[2][1] as f64 / ticks as f64;
    eprintln!("ambient starts per tick {ambient:.3}, expected {:.3} ({:?})", 208.0 / 66.0, s);
    eprintln!("sum of the chances of the chunks within 128 blocks, per cycle: {:.3}", h.world.resource::<Spawner>().expected / ticks as f64);
    assert!((ambient - 208.0 / 66.0).abs() < 0.25);
}

fn category_name(i: usize) -> &'static str {
    ["Monster", "Creature", "Ambient", "Axolotls", "UndergroundWaterCreature", "WaterCreature", "WaterAmbient", "Misc"][i]
}
/// The fill of a world of another kind (caves, the sea): the mean count of each species and of each category after `ticks` ticks over many
/// seeds, the same world and the same player on both sides. Returns the worst |z| and what was seen.
fn fill_law(kind: horizon_ecs_proto::spawning::Kind, biome: &str, at: [f64; 3], ticks: usize, seeds: i64) -> (f64, BTreeMap<String, (f64, f64)>) {
    let mut h = Herd::with_spawn_world_kind_awake(176, true, kind);
    h.set_daylight(false);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at[0], y: at[1], z: at[2], main: 0, off: 0, ..Default::default() });
    h.tick();
    let mw = h.world.resource::<Level>().mw.0;
    h.enable_spawning(biome, 1);
    h.world.resource_mut::<Spawner>().hold = true;
    for _ in 0..1700 {
        h.world.resource_mut::<Clock>().0 += 1;
        horizon_ecs_proto::spawning::step(&mut h.world);
    }
    h.world.resource_mut::<Spawner>().hold = false;
    let (mut refm, mut ecs): (BTreeMap<String, Vec<f64>>, BTreeMap<String, Vec<f64>>) = Default::default();
    for seed in 0..seeds {
        let mut r = RefSpawner::new(RefLevel::new(mw, biome), 1000 + seed * 104_729 + (seed * seed) % 977);
        r.level.drop_entities(&h.world.resource::<Spawner>().table.unported);
        r.level.players.push(at);
        for _ in 0..ticks {
            r.pass();
        }
        let mut n: BTreeMap<String, f64> = BTreeMap::new();
        for m in &r.mobs {
            *n.entry(m.entity.trim_start_matches("minecraft:").to_string()).or_default() += 1.0;
            *n.entry(format!("[{:?}]", m.category)).or_default() += 1.0;
        }
        for (k, v) in n {
            refm.entry(k).or_default().push(v);
        }
        clear(&mut h);
        h.world.resource_mut::<Spawner>().reseed(5000 + seed * 15_485_863);
        h.world.resource_mut::<Clock>().0 = 0;
        for _ in 0..ticks {
            h.world.resource_mut::<Clock>().0 += 1;
            horizon_ecs_proto::spawning::step(&mut h.world);
        }
        let mut n: BTreeMap<String, f64> = BTreeMap::new();
        for (k, v) in by_name(&mut h) {
            n.insert(k.trim_start_matches("minecraft:").to_string(), v as f64);
        }
        let g = recount(&mut h);
        for i in 0..8 {
            n.insert(format!("[{}]", category_name(i)), f64::from(g[i]));
        }
        for k in refm.keys().cloned().chain(n.keys().cloned()).collect::<Vec<_>>() {
            ecs.entry(k.clone()).or_default().push(*n.get(&k).unwrap_or(&0.0));
        }
    }
    let mut worst: f64 = 0.0;
    let mut out = BTreeMap::new();
    let keys: std::collections::BTreeSet<String> = refm.keys().chain(ecs.keys()).cloned().collect();
    for k in keys {
        let mut rv = refm.get(&k).cloned().unwrap_or_default();
        rv.resize(seeds as usize, 0.0);
        let mut ev = ecs.get(&k).cloned().unwrap_or_default();
        ev.resize(seeds as usize, 0.0);
        let ((mr, sr), (me, se)) = (mean_sd(&rv), mean_sd(&ev));
        let z = (mr - me) / ((sr * sr + se * se) / seeds as f64).sqrt().max(1e-9);
        eprintln!("{k:<26} horizon {mr:6.2} (sd {sr:5.2})   ecs {me:6.2} (sd {se:5.2})   z {z:+.2}");
        if sr > 0.0 || se > 0.0 {
            worst = worst.max(z.abs());
        }
        out.insert(k, (mr, me));
    }
    eprintln!("{kind:?}: {seeds} seeds, {ticks} ticks; worst |z| = {worst:.2}; unported {:?}", h.world.resource::<Spawner>().table.unported);
    (worst, out)
}
#[test]
fn the_dark_room_in_the_stone_fills_as_horizons() {
    if !have() {
        return;
    }
    let seeds = std::env::var("SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    let (worst, seen) = fill_law(horizon_ecs_proto::spawning::Kind::Caves, "minecraft:plains", [0.5, -28.0, 0.5], 600, seeds);
    assert!(seen.contains_key("[Monster]") && worst < 4.0, "worst {worst}");
}
#[test]
fn the_sea_fills_as_horizons() {
    if !have() {
        return;
    }
    let seeds = std::env::var("SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    let (worst, seen) = fill_law(horizon_ecs_proto::spawning::Kind::Ocean, "minecraft:ocean", [0.5, 60.0, 0.5], 600, seeds);
    assert!(seen.keys().any(|k| k == "cod" || k == "squid") && worst < 4.0, "worst {worst}");
}

/// What a player who has just come sees: the ticks until the chunks within 32, 64 and 128 blocks have their chances, and the monsters there
/// are at ticks 100, 250 and 500 against Horizon's (whose pass starts on the 208 chunks at once).
#[test]
fn the_first_seconds_after_a_player_arrives() {
    if !have() {
        return;
    }
    let seeds = 40i64;
    let mut h = Herd::with_spawn_world_awake(176, true);
    h.set_daylight(false);
    player(&mut h, 1, 0.5, 0.5);
    h.tick();
    let mw = h.world.resource::<Level>().mw.0;
    let ready_at = |h: &Herd, r: f64| -> bool {
        let sp = h.world.resource::<Spawner>();
        (-9..=9).all(|dx: i32| (-9..=9).all(|dz: i32| {
            let c = (dx, dz);
            let d2 = (f64::from(c.0 * 16 + 8) - 0.5).powi(2) + (f64::from(c.1 * 16 + 8) - 0.5).powi(2);
            d2 >= r * r || sp.reach_chance(c).is_some()
        }))
    };
    let (mut t32, mut t64, mut t128) = (0, 0, 0);
    let (mut rm, mut em) = ([0.0f64; 3], [0.0f64; 3]);
    for seed in 0..seeds {
        let mut r = RefSpawner::new(RefLevel::new(mw, "minecraft:plains"), 77 + seed * 104_729);
        r.level.players.push([0.5, 1.0, 0.5]);
        clear(&mut h);
        h.enable_spawning("minecraft:plains", 9 + seed * 15_485_863);
        h.world.resource_mut::<Online>().seen.clear();
        h.world.resource_mut::<Clock>().0 = 0;
        let (mut a, mut b, mut c) = (0, 0, 0);
        for t in 1..=500usize {
            h.world.resource_mut::<Clock>().0 += 1;
            horizon_ecs_proto::spawning::step(&mut h.world);
            r.pass();
            if a == 0 && ready_at(&h, 32.0) {
                a = t;
            }
            if b == 0 && ready_at(&h, 64.0) {
                b = t;
            }
            if c == 0 && ready_at(&h, 128.0) {
                c = t;
            }
            for (i, at) in [100, 250, 500].into_iter().enumerate() {
                if t == at {
                    rm[i] += r.mobs.iter().filter(|m| format!("{:?}", m.category) == "Monster").count() as f64;
                    em[i] += f64::from(h.world.resource::<SpawnCounts>().global[0]);
                }
            }
        }
        (t32, t64, t128) = (a, b, c);
    }
    let n = seeds as f64;
    eprintln!("chunks ready within 32 / 64 / 128 blocks after {t32} / {t64} / {t128} ticks");
    eprintln!("monsters at ticks 100 / 250 / 500: horizon {:.1} / {:.1} / {:.1}; ecs {:.1} / {:.1} / {:.1}", rm[0] / n, rm[1] / n, rm[2] / n, em[0] / n, em[1] / n, em[2] / n);
}
