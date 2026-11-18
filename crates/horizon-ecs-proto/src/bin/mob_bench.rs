//! `mob-bench N TICKS [PLAYERS [ESPACEMENT]]`, `HORIZON_TEST_RESOURCES` set: the same herd as `cow-bench` with
//! Horizon's `Mob` cows (the general code, one `Mob` per cow, the snapshot of the pass, the grid, the pushes),
//! in the same slab, with the same players: each cow is given the nearest player within 32 blocks as its
//! `nearest_player` (the despawn reference that resets `noActionTime`). `WARM` ticks are untimed.
use horizon_client_lib::{
    health::TestLevel,
    mobs::{self, Mob, MobEvent, MobWorld, PendingPushes, Seen},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_client_lib::mobs::PlayerView;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let ticks: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let players: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let spacing: f64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let warm: usize = std::env::var("WARM").ok().and_then(|v| v.parse().ok()).unwrap_or(100).min(ticks);
    let side = (n as f64).sqrt().ceil() as i32;
    let half = ((f64::from(side) * spacing / 2.0) as i32 + 6).max(20);
    let (level, _species, kind) = horizon_ecs_proto::cow::build_level(half, std::env::var("HAZARDS").is_ok());
    let w: &MobWorld = level.mw.0;
    let items = w.items.expect("items");
    let mut mobs: Vec<Mob> = (0..n)
        .map(|i| {
            let (cx, cz) = ((i as i32 % side) as f64, (i as i32 / side) as f64);
            let o = -f64::from(side) * spacing / 2.0;
            let mut m = Mob::new(i as u32 + 1, kind.clone(), Vec3::new(o + cx * spacing + 0.5, 1.0, o + cz * spacing + 0.5), ((i * 37) % 360) as f32);
            m.persistent = true;
            // SCOPE: only the goals the ECS zombie has (melee, stroll, the two looks; hurt-by and nearest player).
            if std::env::var_os("SCOPE").is_some() {
                use horizon_client_lib::mobs::goals::*;
                m.ai.goals.clear();
                m.ai.targets.clear();
                m.ai.goals.add(3, MeleeAttack::new(1.0, false).zombie());
                m.ai.goals.add(7, Stroll::avoiding_water(1.0));
                m.ai.goals.add(8, LookAtPlayer::new(8.0));
                m.ai.goals.add(8, RandomLookAround::default());
                m.ai.targets.add(1, HurtByTarget::new());
                m.ai.targets.add(2, NearestTarget::player(true));
            }
            m
        })
        .collect();
    let extent = f64::from(side) * spacing;
    let k = (players as f64).sqrt().ceil().max(1.0) as usize;
    let list: Vec<(f64, f64)> = (0..players)
        .map(|i| {
            let (cx, cz) = ((i % k) as f64 + 0.5, (i / k) as f64 + 0.5);
            (-extent / 2.0 + extent * cx / k as f64, -extent / 2.0 + extent * cz / k as f64)
        })
        .collect();
    // The players binned in 32-block cells.
    let bins = ((2.0 * f64::from(half) / 32.0).ceil() as i64).max(1) + 2;
    let bin = |v: f64| (((v + f64::from(half)) / 32.0).floor() as i64 + 1).clamp(0, bins - 1);
    let mut by_bin: Vec<Vec<(f64, f64)>> = vec![Vec::new(); (bins * bins) as usize];
    for &(px, pz) in &list {
        by_bin[(bin(pz) * bins + bin(px)) as usize].push((px, pz));
    }
    let nearest = |p: Vec3| -> Option<Vec3> {
        let (bx, bz) = (bin(p.x), bin(p.z));
        let mut best: Option<(f64, Vec3)> = None;
        for zz in bz - 1..=bz + 1 {
            for xx in bx - 1..=bx + 1 {
                if (0..bins).contains(&xx) && (0..bins).contains(&zz) {
                    for &(px, pz) in &by_bin[(zz * bins + xx) as usize] {
                        let d = (p.x - px).powi(2) + (p.z - pz).powi(2);
                        if d < 32.0 * 32.0 && best.is_none_or(|b| d < b.0) {
                            best = Some((d, Vec3::new(px, 1.0, pz)));
                        }
                    }
                }
            }
        }
        best.map(|b| b.1)
    };
    let mut test_level = TestLevel::new(7);
    let mut pushes = PendingPushes::default();
    let mut attacks = 0usize;
    let counter = horizon_ecs_proto::instr::Counter::new();
    let mut i0 = 0;
    let mut t0 = Instant::now();
    for t in 0..ticks {
        if t == warm {
            t0 = Instant::now();
            i0 = counter.as_ref().map_or(0, |c| c.read());
        }
        let mut snapshot: Vec<Seen> = mobs.iter().map(|m| m.seen(None, items)).collect();
        let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
        let mut events: Vec<MobEvent> = Vec::new();
        let mut level = test_level.level();
        for slot in 0..mobs.len() {
            mobs::apply_mob_pushes(&mut mobs[slot], &mut pushes);
            let from = events.len();
            let near = nearest(mobs[slot].position);
            let view = MobWorld {
                others: &snapshot,
                others_grid: Some(&grid),
                others_solid: Some(&[]),
                nearest_player: near,
                game_time: t as i64 + 1,
                player: if std::env::var_os("HUNT").is_some() { near.map(|p| player_view(p, None)) } else { None },
                ..w.clone()
            };
            let kept = mobs::ai::tick(&mut mobs[slot], &view, &mut level, &mut events);
            mobs::take_mob_pushes(&mut events, from, &mut pushes);
            let mut s = mobs[slot].seen(None, items);
            s.alive &= kept;
            s.removed |= !kept;
            grid.update(slot, s.bounds);
            snapshot[slot] = s;
        }
        attacks += events.iter().filter(|e| matches!(e, MobEvent::AttackPlayer(..))).count();
        std::hint::black_box(&events);
    }
    eprintln!("player hits {attacks}; with a target: {}", mobs.iter().filter(|m| m.ai.target.is_some()).count());
    println!("instr_per_entity_tick={}", counter.as_ref().map_or(0, |c| c.read() - i0) / ((ticks - warm).max(1) * n) as u64);
    let walking = mobs.iter().filter(|m| !m.ai.nav.is_done()).count();
    println!(
        "mob n={n} ticks={ticks} spacing={spacing} players={players} wall/tick={:.1}us walking={walking}",
        t0.elapsed().as_secs_f64() * 1e6 / (ticks - warm).max(1) as f64
    );
}

fn player_view(at: Vec3, item: Option<&str>) -> PlayerView {
    PlayerView {
        position: at,
        bounds: Aabb::new(Vec3::new(at.x - 0.3, at.y, at.z - 0.3), Vec3::new(at.x + 0.3, at.y + 1.8, at.z + 0.3)),
        eye: [at.x, at.y + 1.62, at.z],
        alive: true,
        invulnerable: false,
        creative: false,
        sneaking: false,
        swimming: false,
        look: [0.0; 3],
        main_hand: item.map(Into::into),
        off_hand: None,
        head: None,
        chest: None,
        legs: None,
        feet: None,
        armor_cover: 0.0,
        invisible: false,
        spectator: false,
        gold_armor: false,
        hurt_by: None,
        last_hurt: None,
        velocity: Vec3::ZERO,
        yaw: 0.0,
        sprinting: false,
        riding: false,
        hero: false,
        effects: Vec::new(),
        uuid: [0; 4],
        allies: Default::default(),
        remote: false,
        sleeping: false,
        sleep_timer: 0,
        passenger: false,
        riding_vehicle: None,
        vehicle: None,
    }
}
