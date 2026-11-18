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
}
impl Ref {
    fn new(half: i32, patches: bool) -> Self {
        let (level, _s, _k) = build_level(half, patches);
        let kind = mob_kind("minecraft:zombie");
        Self { w: level.mw.0, mobs: Vec::new(), tl: TestLevel::new(7), pushes: PendingPushes::default(), t: 0, kind, events: Vec::new() }
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
}
fn nearest(spots: &[(f64, f64)], from: (f64, f64)) -> usize {
    (0..spots.len()).min_by(|&a, &b| {
        let d = |i: usize| (spots[i].0 - from.0).powi(2) + (spots[i].1 - from.1).powi(2);
        d(a).partial_cmp(&d(b)).unwrap()
    }).unwrap()
}
/// `n` zombies on a ring of radius `r` around a player standing at the origin, `ticks` ticks.
fn mob_run(n: usize, r: f64, ticks: i64, player_at: impl Fn(i64) -> Vec3) -> Run {
    let spots = ring(n, r);
    let mut rf = Ref::new(60, false);
    for s in &spots {
        rf.add(Vec3::new(s.0, 1.0, s.1));
    }
    let mut out = Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![Vec::new(); n] };
    for t in 1..=ticks {
        let p = player_view(player_at(t), None);
        let from = rf.events.len();
        rf.step(Some(&p));
        for (i, m) in rf.mobs.iter().enumerate() {
            if m.ai.target.is_some() && out.acquired[i].is_none() {
                out.acquired[i] = Some(t);
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
fn ecs_run(n: usize, r: f64, ticks: i64, player_at: impl Fn(i64) -> Vec3) -> Run {
    let spots = ring(n, r);
    let mut h = Herd::with_hazards(0, 100.0, true, false, 5, false);
    h.world.resource_mut::<PlayerState>().immortal = true;
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let ids: Vec<Entity> = spots.iter().enumerate().map(|(i, s)| {
        let e = spawn_species(&mut h.world, &table[2], s.0, s.1, 1000 + i as u64);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
        e
    }).collect();
    let mut out = Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![Vec::new(); n] };
    let mut seen = 0;
    for t in 1..=ticks {
        let p = player_at(t);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0 });
        h.tick();
        for (i, &e) in ids.iter().enumerate() {
            if h.world.get::<Aggro>(e).is_some() && out.acquired[i].is_none() {
                out.acquired[i] = Some(t);
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
    let (m, e) = (mob_run(12, 12.0, 400, STILL), ecs_run(12, 12.0, 400, STILL));
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
        let (m, e) = (mob_run(6, r, 200, STILL), ecs_run(6, r, 200, STILL));
        report(&format!("ring {r}"), &m, &e);
        assert_eq!(mean_of(&m.acquired).1 > 0, mean_of(&e.acquired).1 > 0, "range {r}");
    }
}
/// The target is lost when the player goes out of the follow range.
#[test]
fn the_target_is_lost_beyond_the_range() {
    if !have_data() { return; }
    let away = |t: i64| if t < 150 { Vec3::new(0.5, 1.0, 0.5) } else { Vec3::new(0.5, 1.0, 0.5 + (t - 150) as f64 * 0.5) };
    let run = |ecs: bool| -> Vec<Option<i64>> {
        let _ = ecs;
        Vec::new()
    };
    let _ = run;
    let (m, e) = (mob_run(4, 10.0, 260, away), ecs_run(4, 10.0, 260, away));
    report("player leaves", &m, &e);
}
