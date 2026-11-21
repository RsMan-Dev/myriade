//! `crowd-trace mob|ecs N SPACING TICKS`, `HORIZON_TEST_RESOURCES` set: a tight herd (no player, no walking) of the
//! process's species: every 10 ticks how many animals moved this tick, the largest step, the pairs that still overlap,
//! and the spread of the herd, to see how the `Mob` and the ecs settle.
use horizon_client_lib::{
    health::TestLevel,
    mobs::{self, Mob, MobEvent, MobWorld, PendingPushes, Seen},
};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use std::collections::HashMap;

fn report(label: &str, t: usize, prev: &[(f64, f64)], now: &[(f64, f64)], width: f64) {
    let steps: Vec<f64> = prev.iter().zip(now).map(|(a, b)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()).collect();
    let moving = steps.iter().filter(|&&s| s > 1.0e-9).count();
    let max = steps.iter().cloned().fold(0.0, f64::max);
    let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in now.iter().enumerate() {
        cells.entry((p.0.floor() as i64, p.1.floor() as i64)).or_default().push(i);
    }
    let mut pairs = 0usize;
    for (i, p) in now.iter().enumerate() {
        for dx in -1..=1 {
            for dz in -1..=1 {
                for &j in cells.get(&(p.0.floor() as i64 + dx, p.1.floor() as i64 + dz)).into_iter().flatten() {
                    if j > i && (now[j].0 - p.0).abs() < width && (now[j].1 - p.1).abs() < width {
                        pairs += 1;
                    }
                }
            }
        }
    }
    println!("{label} t={t:4} moving {moving:5} max step {max:.4} overlapping pairs {pairs:6}");
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).map_or("ecs", String::as_str).to_string();
    let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let spacing: f64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.5);
    let ticks: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(200);
    let side = (n as f64).sqrt().ceil() as i32;
    let o = -f64::from(side) * spacing / 2.0;
    // A lattice with a jitter of 0.1 block (a perfect lattice cancels every push in its interior).
    let jit = |i: usize, k: u64| (((i as u64 * 2654435761 + k * 40503) % 1000) as f64 / 1000.0 - 0.5) * 0.2;
    let at = |i: usize| (o + (i as i32 % side) as f64 * spacing + 0.5 + jit(i, 1), o + (i as i32 / side) as f64 * spacing + 0.5 + jit(i, 2));
    let half = ((f64::from(side) * spacing / 2.0) as i32 + 6).max(20);
    if mode == "mob" {
        let (level, species, kind) = build_level(half, false);
        let w: &MobWorld = level.mw.0;
        let items = w.items.expect("items");
        let mut mobs: Vec<Mob> = (0..n)
            .map(|i| {
                let p = at(i);
                let mut m = Mob::new(i as u32 + 1, kind.clone(), Vec3::new(p.0, 1.0, p.1), ((i * 37) % 360) as f32);
                m.persistent = true;
                m.ai.no_action_time = 100;
                m
            })
            .collect();
        let mut tl = TestLevel::new(7);
        let mut pushes = PendingPushes::default();
        let mut prev: Vec<(f64, f64)> = mobs.iter().map(|m| (m.position.x, m.position.z)).collect();
        for t in 1..=ticks {
            let mut snapshot: Vec<Seen> = mobs.iter().map(|m| m.seen(None, items)).collect();
            let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
            let mut events: Vec<MobEvent> = Vec::new();
            let mut lvl = tl.level();
            for slot in 0..mobs.len() {
                mobs::apply_mob_pushes(&mut mobs[slot], &mut pushes);
                let from = events.len();
                let view = MobWorld { others: &snapshot, others_grid: Some(&grid), others_solid: Some(&[]), game_time: t as i64, ..w.clone() };
                let kept = mobs::ai::tick(&mut mobs[slot], &view, &mut lvl, &mut events);
                mobs::take_mob_pushes(&mut events, from, &mut pushes);
                let mut s = mobs[slot].seen(None, items);
                s.alive &= kept;
                s.removed |= !kept;
                grid.update(slot, s.bounds);
                snapshot[slot] = s;
            }
            if t % 10 == 0 || t <= 3 {
                let now: Vec<(f64, f64)> = mobs.iter().map(|m| (m.position.x, m.position.z)).collect();
                report("mob", t, &prev, &now, f64::from(species.width));
            }
            prev = mobs.iter().map(|m| (m.position.x, m.position.z)).collect();
        }
    } else {
        let threads: usize = std::env::var("PAR_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        if threads > 0 {
            bevy_tasks::ComputeTaskPool::get_or_init(|| bevy_tasks::TaskPoolBuilder::new().num_threads(threads).build());
        }
        let mut herd = Herd::with_hazards_awake(n, spacing, true, threads > 0, 42, false);
        for i in 0..n {
            let e = herd.world.query_filtered::<bevy_ecs::prelude::Entity, bevy_ecs::prelude::With<ActiveUntil>>().iter(&herd.world).nth(i).unwrap();
            let p = at(i);
            let b = *herd.world.get::<Body>(e).unwrap();
            herd.world.get_mut::<Pos>(e).unwrap().0 = Vec3::new(p.0, 1.0, p.1);
            herd.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(p.0, 1.0, p.1), b.width, b.height);
        }
        let width = f64::from(species().width);
        let ids: Vec<bevy_ecs::prelude::Entity> = herd.world.query_filtered::<bevy_ecs::prelude::Entity, bevy_ecs::prelude::With<ActiveUntil>>().iter(&herd.world).collect();
        for &e in &ids {
            herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
        }
        let pos = |herd: &Herd| -> Vec<(f64, f64)> { ids.iter().map(|&e| herd.world.get::<Pos>(e).map_or((0.0, 0.0), |p| (p.0.x, p.0.z))).collect() };
        if std::env::var_os("ORDERED_PUSH").is_some() {
            herd.set_ordered_push(true);
        }
        let mut prev = pos(&herd);
        for t in 1..=ticks {
            herd.tick();
            let now = pos(&herd);
            if t % 10 == 0 || t <= 3 {
                report("ecs", t, &prev, &now, width);
                let (_, following, moving, total) = herd.counts();
                let s = herd.world.resource::<OrderedStats>();
                println!("      ordered pass: {} animals, {} islands (sizes 1 / 2-3 / 4-10 / 11-100 / >100 over the ticks: {:?}), largest {}", s.last, s.islands, s.sizes, s.largest);
                // Overlapping pairs whose two animals have no `Moving` (the push of the batch mode never visits them).
                let moving_set: std::collections::HashSet<_> = herd.world.query_filtered::<bevy_ecs::prelude::Entity, bevy_ecs::prelude::With<Moving>>().iter(&herd.world).collect();
                let mut both_still = 0;
                let mut any_pair = 0;
                for i in 0..ids.len() {
                    for j in i + 1..ids.len() {
                        if (now[i].0 - now[j].0).abs() < width && (now[i].1 - now[j].1).abs() < width {
                            any_pair += 1;
                            if !moving_set.contains(&ids[i]) && !moving_set.contains(&ids[j]) {
                                both_still += 1;
                            }
                        }
                    }
                }
                println!("      pairs {any_pair}, with both animals without Moving: {both_still}");
                println!("      ecs modules: physics active {moving}, following {following}, of {total}");
            }
            prev = now;
        }
    }
}
