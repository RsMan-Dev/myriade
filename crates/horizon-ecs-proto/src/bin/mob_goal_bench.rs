//! `mob-goal-bench N TICKS PLAYERS HOLD_PCT HURT LOVE [ESPACEMENT]`: `mob_bench`'s herd with the scene of `goal_bench`
//! (players, a share of them holding wheat, a tenth of the holders walking, every 50 ticks HURT cows struck and LOVE
//! pairs of neighbours put in love); each cow sees the nearest player within 32 blocks as `player`. Was `mob-bench N TICKS [PLAYERS [ESPACEMENT]]`, `HORIZON_TEST_RESOURCES` set: the same herd as `cow-bench` with
//! Horizon's `Mob` cows (the general code, one `Mob` per cow, the snapshot of the pass, the grid, the pushes),
//! in the same slab, with the same players: each cow is given the nearest player within 32 blocks as its
//! `nearest_player` (the despawn reference that resets `noActionTime`). `WARM` ticks are untimed.
use horizon_client_lib::{
    health::TestLevel,
    mobs::{self, Damage, DamageKind, Mob, MobEvent, MobWorld, PendingPushes, PlayerView, Seen},
};
use horizon_core::geometry::{Aabb, Vec3};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let ticks: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let players: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let hold: f64 = a.get(4).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) / 100.0;
    let (hurt, love): (usize, usize) = (a.get(5).and_then(|s| s.parse().ok()).unwrap_or(0), a.get(6).and_then(|s| s.parse().ok()).unwrap_or(0));
    let spacing: f64 = a.get(7).and_then(|s| s.parse().ok()).unwrap_or(2.0);
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
            if std::env::var_os("SCOPE").is_some() && m.species() == horizon_client_lib::mobs::Species::Skeleton {
                // The skeleton's goals the ECS has: the bow (at 4, the bow in hand), stroll, the two looks; hurt-by and the nearest player.
                use horizon_client_lib::mobs::goals::*;
                m.extra.set_main_hand(Some("minecraft:bow".into()));
                m.ai.goals.clear();
                m.ai.targets.clear();
                m.ai.goals.add(4, horizon_client_lib::mobs::undead::SkeletonWeapon::new(20, 40));
                m.ai.goals.add(5, Stroll::avoiding_water(1.0));
                m.ai.goals.add(6, LookAtPlayer::new(8.0));
                m.ai.goals.add(6, RandomLookAround::default());
                m.ai.targets.add(1, HurtByTarget::new());
                m.ai.targets.add(2, NearestTarget::player(true));
            } else if std::env::var_os("SCOPE").is_some() {
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
    let holders = (players as f64 * hold) as usize;
    let walkers = holders / 10;
    let item_name = std::env::var("ITEM").unwrap_or_else(|_| "minecraft:wheat".into());
    let view = |at: Vec3, wheat: bool| {
        let mut p = player_view(at);
        p.main_hand = wheat.then(|| item_name.as_str().into());
        p
    };
    let mut views: Vec<PlayerView> = list.iter().enumerate().map(|(i, &(x, z))| view(Vec3::new(x, 1.0, z), i < holders)).collect();
    // The players binned in 32-block cells.
    let bins = ((2.0 * f64::from(half) / 32.0).ceil() as i64).max(1) + 2;
    let bin = |v: f64| (((v + f64::from(half)) / 32.0).floor() as i64 + 1).clamp(0, bins - 1);
    let mut by_bin: Vec<Vec<(f64, f64, usize)>> = vec![Vec::new(); (bins * bins) as usize];
    for (i, &(px, pz)) in list.iter().enumerate() {
        by_bin[(bin(pz) * bins + bin(px)) as usize].push((px, pz, i));
    }
    let nearest = |p: Vec3| -> Option<usize> {
        let (bx, bz) = (bin(p.x), bin(p.z));
        let mut best: Option<(f64, usize)> = None;
        for zz in bz - 1..=bz + 1 {
            for xx in bx - 1..=bx + 1 {
                if (0..bins).contains(&xx) && (0..bins).contains(&zz) {
                    for &(px, pz, id) in &by_bin[(zz * bins + xx) as usize] {
                        let d = (p.x - px).powi(2) + (p.z - pz).powi(2);
                        if d < 32.0 * 32.0 && best.is_none_or(|b| d < b.0) {
                            best = Some((d, id));
                        }
                    }
                }
            }
        }
        best.map(|b| b.1)
    };
    let mut seed = 0x9e37_79b9_u64;
    let mut next = |m: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as usize % m.max(1)
    };
    let mut test_level = TestLevel::new(7);
    let mut pushes = PendingPushes::default();
    let counter = horizon_ecs_proto::instr::Counter::new();
    let mut i0 = 0;
    let mut t0 = Instant::now();
    for t in 0..ticks {
        if t == warm {
            t0 = Instant::now();
            i0 = counter.as_ref().map_or(0, |c| c.read());
        }
        // The walkers (as in `goal_bench`) move along x; they stay in their bin for the nearest-player lookup.
        for (i, v) in views.iter_mut().enumerate().take(walkers) {
            let phase = ((t / 100 + i) % 2) as f64 * 2.0 - 1.0;
            let dx = ((t % 100) as f64 * 0.1 - 5.0) * phase;
            let at = Vec3::new(list[i].0 + dx, 1.0, list[i].1);
            *v = view(at, true);
        }
        if t % 50 == 10 && t >= 50 {
            let mut lvl = test_level.level();
            for _ in 0..hurt {
                let i = next(mobs.len());
                mobs[i].damage(&mut lvl, 1.0, Damage::of(DamageKind::MobAttack));
            }
            for _ in 0..love {
                let i = next(mobs.len() - 1);
                for j in [i, i + 1] {
                    if mobs[j].extra.age == 0 {
                        mobs[j].extra.in_love = 600;
                    }
                }
            }
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
                nearest_player: near.map(|i| Vec3::new(list[i].0, 1.0, list[i].1)),
                player: near.map(|i| views[i].clone()),
                game_time: t as i64 + 1,
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
        std::hint::black_box(&events);
    }
    let instr = counter.as_ref().map_or(0, |c| c.read() - i0) / ((ticks - warm).max(1) * n) as u64;
    let breeding = mobs.iter().filter(|m| m.ai.goals.is_running("Breed")).count();
    let tempting = mobs.iter().filter(|m| m.ai.goals.is_running("Tempt")).count();
    let panicking = mobs.iter().filter(|m| m.ai.goals.is_running("Panic")).count();
    let walking = mobs.iter().filter(|m| !m.ai.nav.is_done()).count();
    eprintln!("path searches over the whole run ({ticks} ticks): {}", horizon_client_lib::mobs::nav::FINDER_CALLS.load(std::sync::atomic::Ordering::Relaxed));
    println!(
        "mob n={n} instr_per_entity_tick={instr} ticks={ticks} spacing={spacing} players={players} hold={hold} hurt={hurt} love={love} wall/tick={:.1}us walking={walking} tempting={tempting} breeding={breeding} panicking={panicking} cows_now={}",
        t0.elapsed().as_secs_f64() * 1e6 / (ticks - warm).max(1) as f64,
        mobs.len()
    );
}

fn player_view(at: Vec3) -> PlayerView {
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
        main_hand: None,
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
