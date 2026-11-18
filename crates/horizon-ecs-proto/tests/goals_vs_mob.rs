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
        let (level, _s, kind) = build_level(half, patches);
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
    fn hurt(&mut self, i: usize, amount: f32) {
        let mut lvl = self.tl.level();
        self.mobs[i].damage(&mut lvl, amount, Damage::of(DamageKind::MobAttack));
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
fn ecs_cow(h: &mut Herd, x: f64, y: f64, z: f64, seed: u64) -> Entity {
    let e = h.world.spawn(cow_bundle(x, z, seed)).id();
    h.world.get_mut::<Pos>(e).unwrap().0.y = y;
    h.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(x, y, z), 0.9, 1.4);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
    e
}
fn health(h: &Herd, e: Entity) -> f32 {
    h.world.get::<Health>(e).map_or(0.0, |x| x.0)
}
fn mob_health(m: &Mob) -> f32 {
    if m.health <= 0.0 || m.removed { 0.0 } else { m.health }
}

/// A cow put in a cactus, in fire, in lava: its health at the ticks 40, 140 and 340, with the panic of each.
#[test]
fn cactus_fire_and_lava_hurt_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let spots = [("lava", -47.0, -47.0), ("fire", -47.0, -39.0), ("cactus", -39.0, -39.0)];
    let mut r = Ref::new(56, true);
    let mut h = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let ids: Vec<(usize, Entity)> = spots.iter().map(|s| (r.add(Vec3::new(s.1, 1.0, s.2)), ecs_cow(&mut h, s.1, 1.0, s.2, 77))).collect();
    let mut rows = Vec::new();
    for t in 1..=340 {
        r.step(None);
        h.tick();
        if [40, 140, 340].contains(&t) {
            rows.push((t, ids.iter().map(|&(m, e)| (mob_health(&r.mobs[m]), health(&h, e))).collect::<Vec<_>>()));
        }
    }
    for (t, row) in &rows {
        let line: Vec<String> = spots.iter().zip(row).map(|(s, (m, e))| format!("{} mob {m} ecs {e}", s.0)).collect();
        eprintln!("tick {t}: {}", line.join(", "));
    }
    for (t, row) in &rows {
        for (s, (m, e)) in spots.iter().zip(row) {
            assert!((m - e).abs() <= 2.0, "{} at tick {t}: mob {m}, ecs {e}", s.0);
        }
    }
}

#[test]
#[ignore = "trace"]
fn trace_cactus() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let e = ecs_cow(&mut h, -39.0, 1.0, -39.0, 77);
    for t in 1..=60 {
        h.tick();
        let g = h.world.get::<GoalSet>(e);
        if let Some(g) = g {
            let p = h.world.get::<Pos>(e).unwrap().0;
            eprintln!(
                "t={t} pos=({:.2},{:.2},{:.2}) hp={} panic={} stroll={} following={} needs={} moving={} path={}",
                p.x, p.y, p.z, health(&h, e), g.is_running(Kind::Panic), g.is_running(Kind::Stroll),
                h.world.get::<Following>(e).is_some(), h.world.get::<NeedsPath>(e).is_some(), h.world.get::<Moving>(e).is_some(),
                h.world.get::<Nav>(e).unwrap().path.is_some()
            );
        }
    }
}

#[test]
#[ignore = "trace"]
fn trace_mob_cactus() {
    if !have_data() {
        return;
    }
    let mut r = Ref::new(56, true);
    let i = r.add(Vec3::new(-39.0, 1.0, -39.0));
    for t in 1..=30 {
        r.step(None);
        let m = &r.mobs[i];
        eprintln!("mob t={t} pos=({:.2},{:.2},{:.2}) hp={} goals={:?} last_damage={:?} age={}", m.position.x, m.position.y, m.position.z, m.health, r.names(i), m.last_damage.map(|d| d.1), m.age);
    }
}
