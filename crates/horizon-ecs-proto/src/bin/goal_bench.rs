//! `goal-bench N TICKS PLAYERS HOLD_PCT HURT LOVE [SPACING]`, `HORIZON_TEST_RESOURCES` set: a herd of N cows with
//! PLAYERS players on a grid, HOLD_PCT % of them holding wheat (a tenth of the holders walk back and forth), and
//! every 50 ticks HURT cows struck and LOVE pairs of neighbours fed. Prints the wall time per tick (ticks after
//! the first 100) and how many cows have each module active at the end.
use horizon_ecs_proto::{
    cow::{Cause, Clock, DamageQueue, Herd},
    goals::{self, Online, Player},
};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let arg = |i: usize, d: f64| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (n, ticks, players) = (arg(1, 10000.0) as usize, arg(2, 200.0) as usize, arg(3, 0.0) as usize);
    let (hold, hurt, love, spacing) = (arg(4, 0.0) / 100.0, arg(5, 0.0) as usize, arg(6, 0.0) as usize, arg(7, 2.0));
    // The item the holders hold (`ITEM`, default wheat): the one that tempts the species of the run.
    let item = goals::item_id(&std::env::var("ITEM").unwrap_or_else(|_| "minecraft:wheat".into()));
    horizon_ecs_proto::bench_support::install_dummies(0);
    let mut herd = Herd::new(n, spacing, true, false, 42);
    if std::env::var_os("NO_XP").is_some() {
        herd.world.insert_resource(horizon_ecs_proto::orb::NoXp);
    }
    // The lazy look is on by default (owner decision); `EAGER=1` runs the look goals for every animal.
    if std::env::var_os("EAGER").is_some() {
        goals::set_lazy_look(&mut herd.world, false);
    }
    let cows: Vec<_> = herd.world.query::<bevy_ecs::entity::Entity>().iter(&herd.world).collect();
    let side = (n as f64).sqrt().ceil() * spacing;
    let k = ((players as f64).sqrt().ceil() as usize).max(1);
    let spot = |i: usize| {
        let (cx, cz) = ((i % k) as f64 + 0.5, (i / k) as f64 + 0.5);
        (-side / 2.0 + side * cx / k as f64, -side / 2.0 + side * cz / k as f64)
    };
    let holders = (players as f64 * hold) as usize;
    for i in 0..players {
        let (x, z) = spot(i);
        herd.world.resource_mut::<Online>().set(Player { id: i as u32, x, y: 1.0, z, main: if i < holders { item } else { 0 }, off: 0 });
    }
    let walkers = holders / 10;
    let mut seed = 0x9e37_79b9_u64;
    let mut next = |m: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as usize % m.max(1)
    };
    let warm: usize = std::env::var("WARM").ok().and_then(|v| v.parse().ok()).unwrap_or(100).min(ticks);
    let counter = horizon_ecs_proto::instr::Counter::new();
    let mut i0 = 0;
    let mut t0 = Instant::now();
    for t in 0..ticks {
        if t == warm {
            t0 = Instant::now();
            i0 = counter.as_ref().map_or(0, |c| c.read());
        }
        for i in 0..walkers {
            let (x, z) = spot(i);
            let phase = ((t / 100 + i) % 2) as f64 * 2.0 - 1.0;
            let dx = ((t % 100) as f64 * 0.1 - 5.0) * phase;
            herd.world.resource_mut::<Online>().set(Player { id: i as u32, x: x + dx, y: 1.0, z, main: item, off: 0 });
        }
        if t % 50 == 10 && t >= 50 {
            for _ in 0..hurt {
                let e = cows[next(cows.len())];
                herd.world.resource_mut::<DamageQueue>().add(e, 1.0, Cause::Entity);
            }
            for _ in 0..love {
                let i = next(cows.len() - 1);
                goals::feed(&mut herd.world, cows[i]);
                goals::feed(&mut herd.world, cows[i + 1]);
            }
        }
        herd.tick();
    }
    let dt = t0.elapsed();
    let instr = counter.as_ref().map_or(0, |c| c.read() - i0) / ((ticks - warm).max(1) * n) as u64;
    let (_, following, moving, total) = herd.counts();
    let now = herd.world.resource::<Clock>().0;
    let modules: Vec<String> = goals::counts(&mut herd.world).iter().map(|(k, v)| format!("{k}={v}")).collect();
    let a: Vec<u64> = goals::ASKS.iter().map(|x| x.load(std::sync::atomic::Ordering::Relaxed)).collect();
    let finds = goals::FINDS.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("path searches over the whole run ({} ticks): asked by stroll {} panic {} tempt {} breed {} follow-parent {} melee {}; searches run {finds}", ticks, a[0], a[1], a[2], a[3], a[4], a[5]);
    println!(
        "goals n={n} instr_per_entity_tick={instr} ticks={ticks} players={players} hold={hold} hurt={hurt} love={love} wall/tick={:.1}us cows_now={total} tick={now} following={following} moving={moving} {}",
        dt.as_secs_f64() * 1e6 / (ticks - warm).max(1) as f64,
        modules.join(" ")
    );
}
