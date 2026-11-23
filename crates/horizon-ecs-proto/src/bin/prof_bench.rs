//! `prof-bench SCENE N TICKS [PLAYERS [WARM]]` (SCENE: idle | active | hunt | arrows: one animal and N arrows shot into the ground), `HORIZON_TEST_RESOURCES` set (and
//! `HORIZON_SPECIES` for the species of the herd): the instructions per tick of every system of the real schedule
//! (`profile.rs`), the entities it visits per tick, instructions per visited entity, and the share of the tick.
use horizon_ecs_proto::{
    cow::{Cause, DamageQueue, Herd},
    goals::{self, Online, Player, wheat},
    profile::{self, Counter},
};

fn main() {
    horizon_ecs_proto::bench_support::require_optimized();
    let a: Vec<String> = std::env::args().collect();
    let scene = a.get(1).map_or("idle", String::as_str);
    let arg = |i: usize, d: usize| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (n, ticks) = (arg(2, 3000), arg(3, 300));
    let players = arg(4, if scene == "idle" { 0 } else { n / 10 });
    let spacing: f64 = std::env::var("SPACING").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);
    let mix = if scene == "active" && std::env::var_os("NOMIX").is_none() { 4 } else { 0 };
    horizon_ecs_proto::behaviours::long_jump::LJ_ON.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut herd = Herd::with_mix_awake(if scene == "arrows" || scene.starts_with("item") || scene.starts_with("orb") { 1 } else { n }, spacing, true, false, 42, false, mix);
    if scene == "arrows" {
        // N arrows shot at the ground from 3 blocks up, spread over the slab: they stick in the first ticks and then sleep.
        for i in 0..n {
            let (x, z) = (-14.0 + 28.0 * ((i * 37 % 101) as f64 / 101.0), -14.0 + 28.0 * ((i * 53 % 103) as f64 / 103.0));
            horizon_ecs_proto::projectile::launch(&mut herd.world, horizon_ecs_proto::projectile::Flight::ARROW, horizon_core::geometry::Vec3::new(x, 3.0, z), horizon_core::geometry::Vec3::new(0.05, -0.5, 0.0), None);
        }
    }
    // ITEMS: N full stacks (which never merge) dropped from 1.5 blocks over the slab: `items` lets them land and sleep, `itemfall` drops N/50
    // more each tick; `itemsref` runs Horizon's own `ItemEntity.tick` on every item every tick (the same scene, the cost to compare).
    let spot_of = |i: usize| (-14.0 + 28.0 * ((i * 37 % 101) as f64 / 101.0), -14.0 + 28.0 * ((i * 53 % 103) as f64 / 103.0));
    let stack = || myriade_engine_lib::items::ItemStack::new("minecraft:stone", 64);
    if scene == "items" {
        for i in 0..n {
            let (x, z) = spot_of(i);
            horizon_ecs_proto::item::drop_item(&mut herd.world, stack(), horizon_core::geometry::Vec3::new(x, 1.5, z), horizon_core::geometry::Vec3::ZERO, 0);
        }
    }
    if scene == "orbs" {
        for i in 0..n {
            let (x, z) = spot_of(i);
            horizon_ecs_proto::orb::spawn_orb(&mut herd.world, 1, 1, horizon_core::geometry::Vec3::new(x, 1.5, z), horizon_core::geometry::Vec3::ZERO);
        }
    }
    if scene == "orbsref" {
        let mw = herd.world.resource::<horizon_ecs_proto::cow::Level>().mw.0;
        let mut o = horizon_ecs_proto::item_reference::OrbsOriginal { mw, orbs: Vec::new(), players: Vec::new() };
        for i in 0..n {
            let (x, z) = spot_of(i);
            o.orbs.push(myriade_engine_lib::entities::ExperienceOrb::new(i as u32 + 1, horizon_core::geometry::Vec3::new(x, 1.5, z), horizon_core::geometry::Vec3::ZERO, 1 + (i % 11) as i32));
        }
        let counter = Counter::new();
        let warm = arg(5, 100).min(ticks / 2);
        let mut total = 0u64;
        for t in 0..ticks {
            let c0 = counter.read();
            o.tick();
            if t >= warm {
                total += counter.read() - c0;
            }
        }
        println!("scene=orbsref n={n} instr/tick={:.0} instr/orb-tick={:.0}", total as f64 / (ticks - warm) as f64, total as f64 / (ticks - warm) as f64 / n as f64);
        return;
    }
    if scene == "itemsref" {
        let mw = herd.world.resource::<horizon_ecs_proto::cow::Level>().mw.0;
        let mut o = horizon_ecs_proto::item_reference::Original { mw, items: Vec::new() };
        for i in 0..n {
            let (x, z) = spot_of(i);
            o.items.push(horizon_ecs_proto::item_reference::original(i as u32 + 1, stack(), horizon_core::geometry::Vec3::new(x, 1.5, z), horizon_core::geometry::Vec3::ZERO, 0));
        }
        let counter = Counter::new();
        let warm = arg(5, 100).min(ticks / 2);
        let mut total = 0u64;
        for t in 0..ticks {
            let c0 = counter.read();
            o.tick();
            if t >= warm {
                total += counter.read() - c0;
            }
        }
        println!("scene=itemsref n={n} ticks_measured={} instr/tick={:.0} instr/item-tick={:.0}", ticks - warm, total as f64 / (ticks - warm) as f64, total as f64 / (ticks - warm) as f64 / n as f64);
        return;
    }
    if std::env::var_os("POOL").is_some() {
        herd.fill_water(4);
        herd.tick();
    }
    let shared = profile::instrument(&mut herd);
    let cows: Vec<_> = herd.world.query::<bevy_ecs::entity::Entity>().iter(&herd.world).collect();
    let side = (n as f64).sqrt().ceil() * spacing;
    let k = ((players as f64).sqrt().ceil() as usize).max(1);
    let spot = |i: usize| {
        let (cx, cz) = ((i % k) as f64 + 0.5, (i / k) as f64 + 0.5);
        (-side / 2.0 + side * cx / k as f64, -side / 2.0 + side * cz / k as f64)
    };
    let list: Vec<(f64, f64)> = (0..players).map(spot).collect();
    let holders = if scene == "active" { players * 3 / 10 } else { 0 };
    if scene == "hunt" {
        herd.set_online(&list);
    } else {
        for i in 0..players {
            let (x, z) = spot(i);
            herd.world.resource_mut::<Online>().set(Player { id: i as u32, x, y: 1.0, z, main: if i < holders { wheat() } else { 0 }, off: 0, ..Default::default() });
        }
    }
    herd.set_players(list);
    // ZONE=r: the simulated chunks on (r chunks around the players): with no player every creature sleeps (the cost of a dormant one).
    if let Some(r) = std::env::var("ZONE").ok().and_then(|v| v.parse().ok()) {
        herd.set_simulation(r);
    }
    let walkers = holders / 10;
    let mut seed = 0x9e37_79b9_u64;
    let mut next = |m: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as usize % m.max(1)
    };
    let warm = arg(5, 100).min(ticks / 2);
    let counter = Counter::new();
    let mut total = 0u64;
    for t in 0..ticks {
        if t == warm {
            profile::reset(&shared);
            horizon_ecs_proto::brain::bprobe::reset();
            total = 0;
        }
        if scene == "itemfall" {
            for k in 0..(n / 50).max(1) {
                let (x, z) = spot_of(t * 97 + k);
                horizon_ecs_proto::item::drop_item(&mut herd.world, stack(), horizon_core::geometry::Vec3::new(x, 5.0, z), horizon_core::geometry::Vec3::ZERO, 0);
            }
        }
        if scene == "active" {
            for i in 0..walkers {
                let (x, z) = spot(i);
                let phase = ((t / 100 + i) % 2) as f64 * 2.0 - 1.0;
                let dx = ((t % 100) as f64 * 0.1 - 5.0) * phase;
                herd.world.resource_mut::<Online>().set(Player { id: i as u32, x: x + dx, y: 1.0, z, main: wheat(), off: 0, ..Default::default() });
            }
            if t % 50 == 10 && t >= 50 {
                for _ in 0..(n / 150).max(1) {
                    let e = cows[next(cows.len())];
                    herd.world.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Entity);
                }
                for _ in 0..(n / 150).max(1) {
                    let i = next(cows.len() - 1);
                    for e in [cows[i], cows[i + 1]] {
                        if herd.world.get::<goals::BreedData>(e).is_some() {
                            goals::feed(&mut herd.world, e);
                        }
                    }
                }
            }
        }
        let c0 = counter.read();
        herd.tick();
        total += counter.read() - c0;
    }
    let r = shared.lock().unwrap();
    let t = r.ticks.max(1) as f64;
    let sum: u64 = r.rows.iter().map(|x| x.run + x.apply).sum();
    println!("scene={scene} n={n} players={players} ticks_measured={} read_cost={} instr/tick(whole Schedule::run)={:.0} sum_rows/tick={:.0} outside_rows/tick={:.0}", r.ticks, r.read_cost, total as f64 / t, sum as f64 / t, (total.saturating_sub(sum)) as f64 / t);
    println!("{:>3} {:<22} {:>12} {:>12} {:>10} {:>12} {:>7}", "#", "system", "instr/tick", "of which apply", "visited/tick", "instr/visited", "share");
    for (i, x) in r.rows.iter().enumerate() {
        let name = x.name.rsplit("::").next().unwrap_or(&x.name);
        let per = (x.run + x.apply) as f64 / t;
        let v = x.visited as f64 / t;
        println!("{i:>3} {name:<22} {per:>12.0} {:>12.0} {v:>10.1} {:>12} {:>6.1}%", x.apply as f64 / t, if v > 0.0 { format!("{:.0}", per / v) } else { "-".into() }, 100.0 * per / (total as f64 / t));
    }
    for (k, calls, instr) in horizon_ecs_proto::brain::bprobe::dump().into_iter().take(70) {
        println!("BRAIN {k:<48} calls/tick={:>9.1} instr/tick={:>11.0} instr/call={:>8.0}", calls as f64 / t, instr as f64 / t, instr as f64 / calls as f64);
    }
    let l: Vec<u64> = horizon_ecs_proto::behaviours::long_jump::LJ.iter().map(|c| c.load(std::sync::atomic::Ordering::Relaxed)).collect();
    if l[0] > 0 {
        println!(
            "LJ starts={} picks={} drawn={} own_cell={} not_acceptable={} no_arc={} searches={} reached={} chosen={} mean_dist={:.2} found_nothing={} on_earlier_path={}",
            l[0], l[1], l[2], l[3], l[4], l[5], l[6], l[7], l[8], l[9] as f64 / 100.0 / l[8].max(1) as f64, l[10], l[13]
        );
    }
    let h: Vec<u64> = horizon_ecs_proto::behaviours::long_jump::LJH.iter().map(|c| c.load(std::sync::atomic::Ordering::Relaxed)).collect();
    if l[0] > 0 {
        println!("LJH drawn/50 {:?} searches/10 {:?} chosen-by-drawn {:?}", &h[0..9], &h[9..18], &h[18..27]);
    }
}
