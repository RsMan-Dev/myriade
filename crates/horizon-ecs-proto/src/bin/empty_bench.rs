//! `empty-bench N [TICKS]`, `HORIZON_TEST_RESOURCES` set: an EMPTY server (the zone of simulation on, no player) with N creatures
//! of every species of the registry in turn, asleep. Over TICKS ticks (6 000 by default; the daylight flips at the middle) it prints
//! the instructions per tick (first and last 500 ticks), the wakes of the zone, and the creatures that are not asleep at the end.
//! The cost per tick must not depend on N, and no creature may wake. (The ECS has no weather: rain is not part of the scene.)
use horizon_ecs_proto::{
    cow::{Herd, species_table, spawn_species},
    profile::Counter,
};

fn main() {
    horizon_ecs_proto::bench_support::require_optimized();
    let a: Vec<String> = std::env::args().collect();
    let n: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let ticks: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(6000);
    let spacing = 4.0;
    let side = (n as f64).sqrt().ceil() as i32;
    let half = ((f64::from(side) * spacing / 2.0) as i32 + 8).max(16);
    let mut herd = Herd::with_slab(half, true, false);
    let table = species_table().0;
    let mut seed = 0x9e37_79b9_u64;
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let (x, z) = (-f64::from(half) + 6.0 + f64::from(i as i32 % side) * spacing + 0.5, -f64::from(half) + 6.0 + f64::from(i as i32 / side) * spacing + 0.5);
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        ids.push(spawn_species(&mut herd.world, &table[i % table.len()], x, z, seed));
    }
    herd.admit_spawned(&ids);
    herd.set_players(vec![]);
    herd.tick();
    herd.tick();
    let counter = Counter::new();
    let before = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
    let (mut head, mut tail) = (0u64, 0u64);
    for t in 0..ticks {
        if t == ticks / 2 {
            let day = false;
            herd.set_daylight(day);
        }
        let c0 = counter.read();
        herd.tick();
        let c = counter.read() - c0;
        if t >= 100 && t < 600 {
            head += c;
        }
        if t >= ticks - 500 {
            tail += c;
        }
    }
    let st = herd.world.resource::<horizon_ecs_proto::cow::Index>().zone.stats;
    let asleep = ids.iter().filter(|&&e| herd.world.get_entity(e).is_ok_and(|m| m.contains::<horizon_ecs_proto::zone::Dormant>())).count();
    if n <= 200 {
        for (i, &e) in ids.iter().enumerate() {
            if herd.world.get_entity(e).is_ok_and(|m| !m.contains::<horizon_ecs_proto::zone::Dormant>()) {
                println!("awake: {}", table[i % table.len()].name);
            }
        }
    }
    let gone = ids.iter().filter(|&&e| herd.world.get_entity(e).is_err()).count();
    println!(
        "n={n} species={} ticks={ticks} instr/tick first500={} last500={} wakes={} sleeps_after_enable={} asleep_at_end={asleep} despawned={gone} awake={}",
        table.len(),
        head / 500,
        tail / 500,
        st.woke - before.woke,
        st.slept - before.slept,
        n - asleep - gone
    );
}
