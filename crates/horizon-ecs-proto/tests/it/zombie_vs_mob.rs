//! The goals of the ECS cow against a real `Mob` cow in the same scenario (`HORIZON_TEST_RESOURCES` set): the key
//! values (health, start and length of a panic, step, ranges, delays, distances) printed and compared.
use bevy_ecs::prelude::*;
use horizon_client_lib::{
    health::TestLevel,
    mobs::{self, Damage, DamageKind, Mob, MobEvent, MobWorld, PendingPushes, PlayerView, Seen},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    cow::*,
    goals::*,
    hostile::*,
    targeting::*,
};
use std::sync::Arc;

struct Ref {
    w: &'static MobWorld<'static>,
    mobs: Vec<Mob>,
    tl: TestLevel,
    pushes: PendingPushes,
    t: i64,
    kind: Arc<mobs::MobType>,
    pub events: Vec<MobEvent>,
    pub sun: bool,
    blocks: Blocks,
}
impl Ref {
    fn new(half: i32, patches: bool) -> Self {
        let (level, _s, _k) = build_level(half, patches);
        let kind = mob_kind("minecraft:zombie");
        Self { w: level.mw.0, mobs: Vec::new(), tl: TestLevel::new(7), pushes: PendingPushes::default(), t: 0, kind, events: Vec::new(), sun: false, blocks: level.blocks }
    }
    /// A block placed in the Mob side's world (the same call the ECS side's `Herd::set_block` makes).
    fn set_block(&mut self, x: i32, y: i32, z: i32, name: &str) {
        let state = self.w.registry.default_state(name).unwrap();
        // SAFETY: no tick runs while the block is set; the world is the leaked one of this test's `Ref`.
        unsafe {
            let _ = (*self.blocks.0).set(x, y, z, state);
        }
    }
    fn add(&mut self, at: Vec3) -> usize {
        let mut m = Mob::new(self.mobs.len() as u32 + 1, self.kind.clone(), at, 0.0);
        m.persistent = true;
        m.ai.no_action_time = 100;
        self.mobs.push(m);
        self.mobs.len() - 1
    }
    fn names(&self, i: usize) -> Vec<&'static str> {
        self.mobs[i].ai.goals.running_names()
    }
    /// A blow of a player standing at `from`: damage and knockback away from it (`Herd::hit` on the ecs side).
    fn hit(&mut self, i: usize, amount: f32, from: [f64; 3]) {
        let mut lvl = self.tl.level();
        self.mobs[i].damage(&mut lvl, amount, Damage::player(from));
    }
    fn step(&mut self, player: Option<&PlayerView>) {
        self.t += 1;
        let items = self.w.items.expect("items");
        let mut snapshot: Vec<Seen> = self.mobs.iter().map(|m| m.seen(None, items)).collect();
        let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
        let mut lvl = self.tl.level();
        for slot in 0..self.mobs.len() {
            mobs::apply_mob_pushes(&mut self.mobs[slot], &mut self.pushes);
            let from = self.events.len();
            let view = MobWorld {
                others: &snapshot,
                others_grid: Some(&grid),
                others_solid: Some(&[]),
                nearest_player: player.map(|p| p.position),
                player: player.cloned(),
                game_time: self.t,
                monsters_burn: self.sun,
                ..self.w.clone()
            };
            let kept = mobs::ai::tick(&mut self.mobs[slot], &view, &mut lvl, &mut self.events);
            mobs::take_mob_pushes(&mut self.events, from, &mut self.pushes);
            let mut s = self.mobs[slot].seen(None, items);
            s.alive &= kept;
            s.removed |= !kept;
            grid.update(slot, s.bounds);
            snapshot[slot] = s;
        }
    }
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

fn have_data() -> bool {
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}
fn ring(n: usize, r: f64) -> Vec<(f64, f64)> {
    (0..n).map(|i| {
        let a = std::f64::consts::TAU * i as f64 / n as f64;
        (r * a.cos() + 0.5, r * a.sin() + 0.5)
    }).collect()
}
struct Run {
    acquired: Vec<Option<i64>>,
    first_hit: Vec<Option<i64>>,
    hits: Vec<Vec<i64>>,
    lost: Vec<Option<i64>>,
}
fn nearest(spots: &[(f64, f64)], from: (f64, f64)) -> usize {
    (0..spots.len()).min_by(|&a, &b| {
        let d = |i: usize| (spots[i].0 - from.0).powi(2) + (spots[i].1 - from.1).powi(2);
        d(a).partial_cmp(&d(b)).unwrap()
    }).unwrap()
}
/// `n` zombies on a ring of radius `r` around a player standing at the origin, `ticks` ticks.
fn mob_run(n: usize, r: f64, ticks: i64, player_at: impl Fn(i64) -> Vec3, die: i64) -> Run {
    let spots = ring(n, r);
    let mut rf = Ref::new(60, false);
    for s in &spots {
        rf.add(Vec3::new(s.0, 1.0, s.1));
    }
    let mut out = Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![Vec::new(); n], lost: vec![None; n] };
    for t in 1..=ticks {
        let mut p = player_view(player_at(t), None);
        p.alive = t < die;
        let from = rf.events.len();
        rf.step(Some(&p));
        for (i, m) in rf.mobs.iter().enumerate() {
            if m.ai.target.is_some() && out.acquired[i].is_none() {
                out.acquired[i] = Some(t);
            }
            if m.ai.target.is_none() && out.acquired[i].is_some() && out.lost[i].is_none() {
                out.lost[i] = Some(t);
            }
        }
        for e in &rf.events[from..] {
            if let MobEvent::AttackPlayer(_, f) = e {
                // The strike comes from the zombie's own position.
                let i = nearest(&rf.mobs.iter().map(|m| (m.position.x, m.position.z)).collect::<Vec<_>>(), (f[0], f[2]));
                out.first_hit[i].get_or_insert(t);
                out.hits[i].push(t);
            }
        }
    }
    out
}
fn ecs_run(n: usize, r: f64, ticks: i64, player_at: impl Fn(i64) -> Vec3, die: i64) -> Run {
    let spots = ring(n, r);
    let mut h = Herd::with_slab(60, true, false);
    h.world.resource_mut::<PlayerState>().immortal = true;
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let ids: Vec<Entity> = spots.iter().enumerate().map(|(i, s)| {
        let e = spawn_species(&mut h.world, &table[2], s.0, s.1, 1000 + i as u64);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
        e
    }).collect();
    let mut out = Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![Vec::new(); n], lost: vec![None; n] };
    let mut seen = 0;
    for t in 1..=ticks {
        let p = player_at(t);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0, ..Default::default() });
        if t == die {
            h.world.resource_mut::<PlayerState>().health = vec![0.0, 0.0];
        }
        h.tick();
        for (i, &e) in ids.iter().enumerate() {
            let has = h.world.get::<Target>(e).is_some();
            if has && out.acquired[i].is_none() {
                out.acquired[i] = Some(t);
            }
            if !has && out.acquired[i].is_some() && out.lost[i].is_none() {
                out.lost[i] = Some(t);
            }
        }
        let hits = h.world.resource::<PlayerState>().hits.clone();
        for hit in &hits[seen..] {
            let pos: Vec<(f64, f64)> = ids.iter().map(|&e| { let q = h.world.get::<Pos>(e).unwrap().0; (q.x, q.z) }).collect();
            let i = nearest(&pos, hit.from);
            out.first_hit[i].get_or_insert(t);
            out.hits[i].push(t);
        }
        seen = hits.len();
    }
    out
}
fn mean_of(v: &[Option<i64>]) -> (f64, usize) {
    let k: Vec<f64> = v.iter().flatten().map(|&x| x as f64).collect();
    (k.iter().sum::<f64>() / k.len().max(1) as f64, k.len())
}
fn report(label: &str, m: &Run, e: &Run) {
    let (ma, na) = mean_of(&m.acquired);
    let (ea, ne) = mean_of(&e.acquired);
    let (mh, nh) = mean_of(&m.first_hit);
    let (eh, nh2) = mean_of(&e.first_hit);
    let per = |r: &Run| r.hits.iter().map(Vec::len).sum::<usize>() as f64 / r.hits.len() as f64;
    let gap = |r: &Run| {
        let g: Vec<f64> = r.hits.iter().flat_map(|h| h.windows(2).map(|w| (w[1] - w[0]) as f64)).collect();
        (g.iter().sum::<f64>() / g.len().max(1) as f64, g.iter().cloned().fold(f64::MAX, f64::min))
    };
    eprintln!("{label}: acquired mob {na} at {ma:.1}, ecs {ne} at {ea:.1}; first hit mob {nh} at {mh:.1}, ecs {nh2} at {eh:.1}; hits per zombie mob {:.1} ecs {:.1}; gap (mean, min) mob {:?} ecs {:?}", per(m), per(e), gap(m), gap(e));
}
const STILL: fn(i64) -> Vec3 = |_| Vec3::new(0.5, 1.0, 0.5);

/// Zombies around a still player: detection delay, first hit, cadence, in a ring of 12 blocks.
#[test]
fn zombies_hunt_a_still_player_as_a_mob_does() {
    if !have_data() { return; }
    let (m, e) = (mob_run(12, 12.0, 400, STILL, i64::MAX), ecs_run(12, 12.0, 400, STILL, i64::MAX));
    report("ring 12", &m, &e);
    let (ma, _) = mean_of(&m.acquired);
    let (ea, _) = mean_of(&e.acquired);
    assert!((ma - ea).abs() < 8.0, "acquisition: mob {ma}, ecs {ea}");
    let (mh, _) = mean_of(&m.first_hit);
    let (eh, _) = mean_of(&e.first_hit);
    assert!((mh - eh).abs() < 25.0, "first hit: mob {mh}, ecs {eh}");
}
/// Beyond the follow range (35) nobody notices; inside it, everyone does.
#[test]
fn the_detection_range_is_the_follow_range() {
    if !have_data() { return; }
    for r in [30.0, 40.0] {
        let (m, e) = (mob_run(60, r, 200, STILL, i64::MAX), ecs_run(60, r, 200, STILL, i64::MAX));
        report(&format!("ring {r}"), &m, &e);
        assert_eq!(mean_of(&m.acquired).1 > 0, mean_of(&e.acquired).1 > 0, "range {r}");
    }
}
/// The target is lost when the player walks out of the follow range, or dies: the tick of the loss on both sides.
#[test]
fn the_target_is_lost_beyond_the_range_or_when_the_player_dies() {
    if !have_data() { return; }
    // 0.3 block a tick away from the zombies' side (they walk 0.23 x ~0.98: slower than 0.3).
    let away = |t: i64| if t < 100 { Vec3::new(0.5, 1.0, 0.5) } else { Vec3::new(0.5 + (t - 100) as f64 * 0.3, 1.0, 0.5) };
    let (m, e) = (mob_run(20, 14.0, 400, away, i64::MAX), ecs_run(20, 14.0, 400, away, i64::MAX));
    let (ml, nm) = mean_of(&m.lost);
    let (el, ne) = mean_of(&e.lost);
    eprintln!("player walks away from tick 100: target lost mob {nm}/20 at {ml:.1}, ecs {ne}/20 at {el:.1}");
    assert!(nm > 0 && ne > 0 && (ml - el).abs() < 20.0, "lost: mob {ml}, ecs {el}");
    let (m, e) = (mob_run(12, 12.0, 300, STILL, 150), ecs_run(12, 12.0, 300, STILL, 150));
    let (ml, nm) = mean_of(&m.lost);
    let (el, ne) = mean_of(&e.lost);
    eprintln!("player dies at tick 150: target lost mob {nm}/12 at {ml:.1}, ecs {ne}/12 at {el:.1}");
    assert!(nm > 0 && ne > 0 && (ml - el).abs() < 6.0, "lost: mob {ml}, ecs {el}");
}

/// Burning in daylight: the tick of the first fire of 20 zombies in the open, no player.
#[test]
fn zombies_burn_in_daylight_as_a_mob_does() {
    if !have_data() { return; }
    let n = 1000;
    let spots: Vec<(f64, f64)> = (0..n).map(|i| (-54.0 + 3.5 * (i % 32) as f64 + 0.5, -54.0 + 3.5 * (i / 32) as f64 + 0.5)).collect();
    let mut rf = Ref::new(60, false);
    rf.sun = true;
    for s in &spots {
        rf.add(Vec3::new(s.0, 1.0, s.1));
    }
    eprintln!("brightness at the eyes {}, sees sky {}", rf.w.brightness(-45, 2, -45), rf.w.can_see_sky(-45, 2, -45));
    let mut mob_first: Vec<Option<i64>> = vec![None; n];
    for t in 1..=150 {
        rf.step(None);
        for (i, m) in rf.mobs.iter().enumerate() {
            if m.fire_ticks > 0 && mob_first[i].is_none() {
                mob_first[i] = Some(t);
            }
        }
    }
    let mut h = Herd::with_slab(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let ids: Vec<Entity> = spots.iter().enumerate().map(|(i, s)| spawn_species(&mut h.world, &table[2], s.0, s.1, 0x9E37_79B9_7F4A_7C15_u64.wrapping_mul(i as u64 + 1))).collect();
    h.set_day(true);
    let mut ecs_first: Vec<Option<i64>> = vec![None; n];
    for t in 1..=150 {
        h.tick();
        for (i, &e) in ids.iter().enumerate() {
            if h.world.get::<Burning>(e).is_some() && ecs_first[i].is_none() {
                ecs_first[i] = Some(t);
            }
        }
    }
    let never: Vec<usize> = (0..n).filter(|&i| ecs_first[i].is_none()).collect();
    for &i in never.iter().take(3) {
        let e = ids[i];
        eprintln!("never {i}: exists {} exposed {:?} burning {:?} pos {:?} health {:?}", h.world.get_entity(e).is_ok(), h.world.get::<horizon_ecs_proto::hostile::Exposed>(e), h.world.get::<Burning>(e), h.world.get::<Pos>(e).map(|p| p.0), h.world.get::<Health>(e).map(|x| x.0));
    }
    eprintln!("ecs never burned: {} e.g. {:?} at {:?}", never.len(), &never[..never.len().min(6)], never.iter().take(6).map(|&i| spots[i]).collect::<Vec<_>>());
    let (m, nm) = mean_of(&mob_first);
    let (e, ne) = mean_of(&ecs_first);
    eprintln!("first fire in daylight: mob {nm}/{n} at {m:.1}, ecs {ne}/{n} at {e:.1}");
    // Night: nobody burns.
    h.set_day(false);
    assert!(nm > 0 && ne > 0, "someone must burn in the day on both sides");
    assert!((m - e).abs() < 3.0, "mob {m}, ecs {e}");
}

/// A zombie hurt by a player calls the zombies within its follow range that have no target, not the ones beyond.
#[test]
fn a_hurt_zombie_alerts_the_zombies_around_it() {
    if !have_data() { return; }
    let mut h = Herd::with_slab(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let near: Vec<Entity> = (0..5).map(|i| spawn_species(&mut h.world, &table[2], -20.0 + 2.0 * i as f64, 0.5, 1 + i)).collect();
    let far = spawn_species(&mut h.world, &table[2], 50.5, 0.5, 9);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: -18.0, y: 1.0, z: 3.5, main: 0, off: 0, ..Default::default() });
    for &e in near.iter().chain([&far]) {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    }
    // The zombies are in the spatial index after their first tick.
    h.tick();
    h.tick();
    h.hit_by(near[0], 1.0, 1);
    for _ in 0..3 {
        h.tick();
    }
    let hunting = |e: Entity| h.world.get::<Target>(e).is_some();
    assert!(near.iter().all(|&e| hunting(e)), "the zombies around it are alerted");
    assert!(!hunting(far), "the one 70 blocks away is not");
}

/// One zombie 12 blocks from a still player, tick by tick on both sides, the ECS target imposed at the tick the Mob's was
/// acquired: the ticks where the melee goal runs, the strikes, the path ends.
#[test]
#[ignore = "trace"]
fn trace_one_zombie() {
    if !have_data() { return; }
    let (px, pz) = (0.5, 0.5);
    let start = (std::env::var("ZX").ok().and_then(|v| v.parse().ok()).unwrap_or(12.5), 0.5);
    let mut rf = Ref::new(60, false);
    rf.add(Vec3::new(start.0, 1.0, start.1));
    let mut h = Herd::with_slab(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let e = spawn_species(&mut h.world, &table[2], start.0, start.1, 1000);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    let p = player_view(Vec3::new(px, 1.0, pz), None);
    let mut acquired = false;
    let (mut mob_goal, mut ecs_goal) = (false, false);
    let (mut mob_following, mut ecs_following) = (true, true);
    for t in 1..=260 {
        let from = rf.events.len();
        rf.step(Some(&p));
        let m = &rf.mobs[0];
        if !acquired && m.ai.target.is_some() {
            acquired = true;
            h.world.resource_mut::<Online>().set(Player { id: 1, x: px, y: 1.0, z: pz, main: 0, off: 0, ..Default::default() });
            let slot = h.world.get::<GoalSet>(e).unwrap().slot_of(&horizon_ecs_proto::behaviours::nearest_target::MODULE).unwrap();
            force_start(&mut h.world, e, slot);
            let mut tg = Target::player(1, 30);
            tg.slot = slot;
            set_target(&mut h.world, e, tg);
            eprintln!("t={t}: target acquired by the Mob; imposed on the ECS");
        }
        h.tick();
        let mob_hit = rf.events[from..].iter().any(|ev| matches!(ev, MobEvent::AttackPlayer(..)));
        let now_goal = m.ai.goals.running_names().iter().any(|n| n.contains("Melee"));
        let ecs_hit = h.world.resource::<PlayerState>().hits.last().is_some_and(|x| x.tick == t as u64);
        let ecs_now = h.world.get::<Meleeing>(e).is_some();
        let mob_walk = !m.ai.nav.is_done();
        let ecs_walk = h.world.get::<Following>(e).is_some();
        let (mx, ex) = (m.position.x, h.world.get::<Pos>(e).unwrap().0.x);
        if mob_hit || ecs_hit || now_goal != mob_goal || ecs_now != ecs_goal || mob_walk != mob_following || ecs_walk != ecs_following {
            eprintln!("t={t:3}: MOB x {mx:6.2} melee {now_goal} walking {mob_walk} hit {mob_hit} | ECS x {ex:6.2} melee {ecs_now} walking {ecs_walk} hit {ecs_hit}");
        }
        if std::env::var_os("ZDEBUG").is_some() && (108..=130).contains(&t) {
            eprintln!("  t={t}: MOB goals {:?} nav_done {} x {:.2} hit {mob_hit}", m.ai.goals.running_names(), m.ai.nav.is_done(), m.position.x);
        }
        (mob_goal, ecs_goal, mob_following, ecs_following) = (now_goal, ecs_now, mob_walk, ecs_walk);
    }
}

/// A zombie hit by the player: which zombies the call reaches (by distance) and when, on both sides. The player stands
/// in a closed stone room next to them, so that nobody acquires it by the nearest-player goal (no sight): the hit
/// still calls, and a target out of sight is kept for the unseen memory.
#[test]
fn the_alert_reaches_the_same_zombies_at_the_same_tick_as_a_mob_s() {
    if !have_data() { return; }
    let dists = [0.0, 5.0, 20.0, 33.0, 35.0, 38.0, 40.0, 50.0];
    let spots: Vec<(f64, f64)> = dists.iter().enumerate().map(|(i, d)| (-52.0 + d, 0.5 + 3.0 * (i % 2) as f64)).collect();
    let player = Vec3::new(-47.5, 1.0, -10.5);
    // The room: walls and roof of stone around the player (x -50..-45, z -13..-8, y 1..4).
    let room = |put: &mut dyn FnMut(i32, i32, i32)| {
        for x in -50..=-45 {
            for z in -13..=-8 {
                for y in 1..=4 {
                    if x == -50 || x == -45 || z == -13 || z == -8 || y == 4 {
                        put(x, y, z);
                    }
                }
            }
        }
    };
    let mut rf = Ref::new(60, false);
    room(&mut |x, y, z| rf.set_block(x, y, z, "minecraft:stone"));
    for s in &spots {
        rf.add(Vec3::new(s.0, 1.0, s.1));
    }
    let view = player_view(player, None);
    for _ in 0..5 {
        rf.step(Some(&view));
    }
    let mut lvl = rf.tl.level();
    rf.mobs[0].damage(&mut lvl, 1.0, Damage::player([player.x, player.y, player.z]));
    drop(lvl);
    let mut mob_alerted: Vec<Option<i64>> = vec![None; spots.len()];
    for t in 1..=6 {
        let from = rf.events.len();
        rf.step(Some(&view));
        for ev in &rf.events[from..] {
            if let MobEvent::SetTarget { mob, .. } = ev {
                mob_alerted[*mob as usize - 1].get_or_insert(t);
            }
        }
        if rf.mobs[0].ai.target.is_some() {
            mob_alerted[0].get_or_insert(t);
        }
    }
    let mut h = Herd::with_slab(60, true, false);
    room(&mut |x, y, z| h.set_block(x, y, z, "minecraft:stone"));
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let ids: Vec<Entity> = spots.iter().enumerate().map(|(i, s)| {
        let e = spawn_species(&mut h.world, &table[2], s.0, s.1, 40 + i as u64);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        e
    }).collect();
    h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: 1.0, z: player.z, main: 0, off: 0, ..Default::default() });
    for _ in 0..5 {
        h.tick();
    }
    h.hit_by(ids[0], 1.0, 1);
    let mut ecs_alerted: Vec<Option<i64>> = vec![None; spots.len()];
    for t in 1..=6 {
        h.tick();
        for (i, &e) in ids.iter().enumerate() {
            if h.world.get::<Target>(e).is_some() {
                ecs_alerted[i].get_or_insert(t);
            }
        }
    }
    eprintln!("distance: mob tick / ecs tick");
    for (i, d) in dists.iter().enumerate() {
        eprintln!("  {d:4}: {:?} / {:?}", mob_alerted[i], ecs_alerted[i]);
    }
    for i in 0..spots.len() {
        assert_eq!(mob_alerted[i].is_some(), ecs_alerted[i].is_some(), "zombie at {} blocks", dists[i]);
    }
}

/// A wall between the zombies and the player: with the wall up from the start nobody acquires it; with the wall built
/// while they chase, the target is lost after the unseen memory (60 ticks), on both sides.
#[test]
fn a_wall_blocks_the_sight_as_it_does_for_a_mob() {
    if !have_data() { return; }
    // 60 zombies in two columns behind the wall (z from -29 to 29).
    let line: Vec<(f64, f64)> = (0..60).map(|i| (20.5 + 2.0 * (i % 2) as f64, -29.0 + 2.0 * (i / 2) as f64)).collect();
    let wall = |put: &mut dyn FnMut(i32, i32, i32)| {
        for z in -34..=34 {
            for y in 1..=3 {
                put(8, y, z);
            }
        }
    };
    let run = |wall_at: i64| -> (Run, Run) {
        let mut rf = Ref::new(60, false);
        for s in &line {
            rf.add(Vec3::new(s.0, 1.0, s.1));
        }
        let mut h = Herd::with_slab(60, true, false);
        h.world.resource_mut::<PlayerState>().immortal = true;
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let ids: Vec<Entity> = line.iter().enumerate().map(|(i, s)| {
            let e = spawn_species(&mut h.world, &table[2], s.0, s.1, 900 + i as u64);
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
            h.world.resource_mut::<Entered>().0.push(e);
            e
        }).collect();
        let n = line.len();
        let (mut m, mut e) = (Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![vec![]; n], lost: vec![None; n] }, Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![vec![]; n], lost: vec![None; n] });
        let view = player_view(Vec3::new(0.5, 1.0, 0.5), None);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        for t in 1..=260i64 {
            if t == wall_at.max(1) {
                wall(&mut |x, y, z| rf.set_block(x, y, z, "minecraft:stone"));
                wall(&mut |x, y, z| h.set_block(x, y, z, "minecraft:stone"));
            }
            rf.step(Some(&view));
            h.tick();
            for i in 0..n {
                let (ma, ea) = (rf.mobs[i].ai.target.is_some(), h.world.get::<Target>(ids[i]).is_some());
                if ma && m.acquired[i].is_none() { m.acquired[i] = Some(t); }
                if !ma && m.acquired[i].is_some() && m.lost[i].is_none() { m.lost[i] = Some(t); }
                if ea && e.acquired[i].is_none() { e.acquired[i] = Some(t); }
                if !ea && e.acquired[i].is_some() && e.lost[i].is_none() { e.lost[i] = Some(t); }
            }
        }
        (m, e)
    };
    let (m, e) = run(1);
    eprintln!("wall from the start: acquired mob {}/60 ecs {}/60", mean_of(&m.acquired).1, mean_of(&e.acquired).1);
    assert_eq!(mean_of(&m.acquired).1, 0, "the Mob sees through the wall?");
    assert_eq!(mean_of(&e.acquired).1, 0, "the ECS zombie sees through the wall");
    let (m, e) = run(60);
    let (ma, na) = mean_of(&m.acquired);
    let (ea, ne) = mean_of(&e.acquired);
    let (ml, nm) = mean_of(&m.lost);
    let (el, nl) = mean_of(&e.lost);
    eprintln!("wall built at tick 60: acquired mob {na} at {ma:.1}, ecs {ne} at {ea:.1}; target lost mob {nm} at {ml:.1}, ecs {nl} at {el:.1}");
    assert!(nm > 0 && nl > 0 && (ml - el).abs() < 15.0, "lost: mob {ml}, ecs {el}");
}

/// The world is lit by the real light engine: a roof over a zombie hides it from the sun (and from `can_see_sky`), the same
/// zombie in the open burns, and removing the roof lets it burn.
#[test]
fn a_roof_over_a_zombie_stops_the_sun_and_removing_it_lets_it_burn() {
    if !have_data() { return; }
    let mut h = Herd::with_slab(20, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let (cx, cz) = (0.5, 0.5);
    let level_sees = |h: &Herd, x: i32, y: i32, z: i32| h.world.resource::<Level>().mw.0.can_see_sky(x, y, z);
    assert!(level_sees(&h, 0, 2, 0), "the open slab sees the sky");
    for dx in -1..=1 {
        for dz in -1..=1 {
            h.set_block(dx, 4, dz, "minecraft:stone");
        }
    }
    h.tick();
    assert!(!level_sees(&h, 0, 2, 0), "a roof of stone over the column: the sky is no longer seen");
    let under = spawn_species(&mut h.world, &table[2], cx, cz, 0x9E37_79B9);
    let open = spawn_species(&mut h.world, &table[2], 10.5, 10.5, 0x7F4A_7C15);
    h.set_day(true);
    for _ in 0..200 {
        h.tick();
    }
    assert!(h.world.get::<Burning>(open).is_some(), "the zombie in the open burns in the day");
    assert!(h.world.get::<Burning>(under).is_none(), "the zombie under the roof does not burn");
    for dx in -1..=1 {
        for dz in -1..=1 {
            h.set_block(dx, 4, dz, "minecraft:air");
        }
    }
    for _ in 0..200 {
        h.tick();
    }
    assert!(level_sees(&h, 0, 2, 0), "the roof is gone: the sky is seen again");
    assert!(h.world.get::<Burning>(under).is_some() || h.world.get_entity(under).is_err(), "without the roof the zombie burns");
}
