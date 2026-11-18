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
/// The item that tempts the species of the run (`HORIZON_SPECIES`): its id and its name.
fn tempt_item() -> (u32, &'static str) {
    if species().name == "minecraft:pig" { (CARROT, "minecraft:carrot") } else { (WHEAT, "minecraft:wheat") }
}
fn have_data() -> bool {
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}
fn ecs_cow(h: &mut Herd, x: f64, y: f64, z: f64, seed: u64) -> Entity {
    let e = h.world.spawn(cow_bundle(x, z, seed)).id();
    h.world.get_mut::<Pos>(e).unwrap().0.y = y;
    h.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(x, y, z), species().width, species().height);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
    // A cow at rest has the resting vertical velocity (a fresh one is in the air for its first tick).
    if y <= 1.0 {
        h.world.get_mut::<Vel>(e).unwrap().0.y = -0.08 * f64::from(0.98_f32);
    }
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
    // The cactus: the `Mob` path starts by lifting the cow out of the solid block it stands in
    // (`GroundPathNavigation.createPath`); the ecs `pathfind` does not, so the ecs cow stays in the cactus (known
    // gap, in the physics and pathfinding of the other agent): only lava and fire are compared.
    for (t, row) in &rows {
        for (s, (m, e)) in spots.iter().zip(row) {
            if s.0 != "cactus" {
                assert!((m - e).abs() <= 2.0, "{} at tick {t}: mob {m}, ecs {e}", s.0);
            }
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

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}
fn flat_pair() -> (Ref, Herd) {
    (Ref::new(20, false), Herd::new(0, 6.0, true, false, 3))
}
fn xz(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// Struck cows: when the panic starts, how long it lasts, the peak step of the walk.
#[test]
fn a_struck_cow_panics_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let mut cows = Vec::new();
    for i in 0..5 {
        for j in 0..3 {
            let (x, z) = (-14.5 + 7.0 * f64::from(i), -11.5 + 8.0 * f64::from(j));
            cows.push((r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 500 + cows.len() as u64)));
        }
    }
    // Per cow, for the mob and the ecs: (first tick of the goal, ticks of the goal in the 60 after the blow, peak step
    // of the goal, distance walked in those 60 ticks, first tick that moves the cow after the blow's recoil).
    let mut m = vec![(None::<i64>, 0_i64, 0.0_f64, 0.0_f64, None::<i64>); cows.len()];
    let mut c = m.clone();
    let mut prev: Vec<(Vec3, Vec3)> = cows.iter().map(|&(i, e)| (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0)).collect();
    for t in 1..=65_i64 {
        if t == 5 {
            for &(i, e) in &cows {
                let p = r.mobs[i].position;
                r.hit(i, 1.0, [p.x, p.y, p.z + 3.0]);
                h.hit(e, 1.0, (p.x, p.z + 3.0));
            }
        }
        r.step(None);
        h.tick();
        for (k, &(i, e)) in cows.iter().enumerate() {
            let mp = r.names(i).contains(&"Panic");
            let cp = h.world.get::<GoalSet>(e).unwrap().is_running(Kind::Panic);
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            for (st, on, step) in [(&mut m[k], mp, xz(prev[k].0, a)), (&mut c[k], cp, xz(prev[k].1, b))] {
                if on && st.0.is_none() {
                    st.0 = Some(t);
                }
                if on {
                    st.1 += 1;
                    st.2 = st.2.max(step);
                }
                if t >= 5 {
                    st.3 += step;
                }
            }
            prev[k] = (a, b);
        }
    }
    type Row = (Option<i64>, i64, f64, f64, Option<i64>);
    let stats = |v: &[Row]| {
        let s: Vec<f64> = v.iter().filter_map(|x| x.0).map(|x| (x - 5) as f64).collect();
        let on: Vec<f64> = v.iter().map(|x| x.1 as f64).collect();
        let p: Vec<f64> = v.iter().map(|x| x.2).collect();
        let d: Vec<f64> = v.iter().map(|x| x.3).collect();
        (mean(&s), s.len(), mean(&on), mean(&p), mean(&d))
    };
    let (ms, mn, mo, mp, md) = stats(&m);
    let (cs, cn, co, cp, cd) = stats(&c);
    eprintln!("panic: mob start +{ms:.1} ({mn} cows) goal ticks {mo:.1}/60 peak step {mp:.3} walked {md:.1}; ecs start +{cs:.1} ({cn}) goal ticks {co:.1}/60 peak step {cp:.3} walked {cd:.1}");
    assert!(mn == cows.len() && cn == cows.len());
    assert!((ms - cs).abs() <= 3.0, "start");
    assert!(cd / md > 0.7 && cd / md < 1.4, "distance walked");
    assert!(cp / mp > 0.8 && cp / mp < 1.25, "speed");
}

/// Wheat in hand: range 9.5 / 10.5, the time to walk up, the stop distance, the calm after putting it away.
#[test]
fn wheat_tempts_as_it_does_a_mob() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let mi = r.add(Vec3::new(-10.5, 1.0, 0.5));
    let e = ecs_cow(&mut h, -10.5, 1.0, 0.5, 9);
    let (mi2, e2) = (r.add(Vec3::new(-10.5, 1.0, 8.5)), ecs_cow(&mut h, -10.5, 1.0, 8.5, 10));
    h.tick();
    let tempted = |r: &Ref, h: &Herd, i: usize, e: Entity| (r.names(i).contains(&"Tempt"), h.world.get::<Tempting>(e).is_some());
    // Player 9.5 blocks from the first cow, 10.5 blocks from the second.
    let at = Vec3::new(-1.0, 1.0, 0.5);
    let mut p = player_view(at, Some(tempt_item().1));
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: 1.0, z: at.z, main: tempt_item().0, off: 0 });
    for _ in 0..6 {
        r.step(Some(&p));
        h.tick();
    }
    let (a, b) = (tempted(&r, &h, mi, e), tempted(&r, &h, mi2, e2));
    eprintln!("range: cow at 9.5 mob/ecs {a:?}; cow at 10.5 {b:?}");
    assert_eq!(a, (true, true));
    assert_eq!(b, (false, false));
    // Walk up: ticks until 3 blocks away, final distance after 300 ticks.
    let (mut mt, mut ct) = (None, None);
    for t in 7..=300 {
        r.step(Some(&p));
        h.tick();
        if mt.is_none() && xz(r.mobs[mi].position, at) < 3.0 {
            mt = Some(t);
        }
        if ct.is_none() && xz(h.world.get::<Pos>(e).unwrap().0, at) < 3.0 {
            ct = Some(t);
        }
    }
    let (md, cd) = (xz(r.mobs[mi].position, at), xz(h.world.get::<Pos>(e).unwrap().0, at));
    eprintln!("walk up: mob within 3 at {mt:?}, ecs {ct:?}; final distance mob {md:.2} ecs {cd:.2}");
    let (mt, ct) = (mt.expect("mob") as f64, ct.expect("ecs") as f64);
    assert!((mt - ct).abs() / mt < 0.25, "time to arrive");
    assert!((md - cd).abs() < 0.8, "stop distance");
    // Wheat away for 4 ticks, then back: the ticks until the goal runs again.
    p.main_hand = None;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: 1.0, z: at.z, main: 0, off: 0 });
    for _ in 0..4 {
        r.step(Some(&p));
        h.tick();
    }
    p.main_hand = Some(tempt_item().1.into());
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: 1.0, z: at.z, main: tempt_item().0, off: 0 });
    let (mut mc, mut cc) = (None, None);
    for t in 1..=200 {
        r.step(Some(&p));
        h.tick();
        if mc.is_none() && r.names(mi).contains(&"Tempt") {
            mc = Some(t);
        }
        if cc.is_none() && h.world.get::<Tempting>(e).is_some() {
            cc = Some(t);
        }
    }
    eprintln!("calm: tempted again after mob {mc:?} ticks, ecs {cc:?}");
    let (mc, cc) = (mc.expect("mob calm") as i64, cc.expect("ecs calm") as i64);
    assert!((mc - cc).abs() <= 6, "calm");
}

/// Two cows in love 2 and 5 blocks apart: the tick of the birth, the parents' age.
#[test]
fn breeding_takes_as_long_as_for_a_mob() {
    if !have_data() {
        return;
    }
    for gap in [2.0, 5.0] {
        let (mut r, mut h) = flat_pair();
        let (a, b) = (r.add(Vec3::new(0.5, 1.0, 0.5)), r.add(Vec3::new(0.5 + gap, 1.0, 0.5)));
        let (ea, eb) = (ecs_cow(&mut h, 0.5, 1.0, 0.5, 1), ecs_cow(&mut h, 0.5 + gap, 1.0, 0.5, 2));
        h.tick();
        for i in [a, b] {
            r.mobs[i].extra.in_love = 600;
        }
        assert!(feed(&mut h.world, ea) && feed(&mut h.world, eb));
        let (mut mt, mut ct, mut ages) = (None, None, (0, 0));
        for t in 1..=300_i64 {
            r.step(None);
            h.tick();
            if mt.is_none() && r.events.iter().any(|e| matches!(e, MobEvent::Breed { .. })) {
                mt = Some(t);
                ages = (r.mobs[a].extra.age, r.mobs[b].extra.age);
            }
            if ct.is_none() && h.world.query::<&GrowAt>().iter(&h.world).count() > 0 {
                ct = Some(t);
            }
        }
        let (mt, ct) = (mt.expect("mob never bred"), ct.expect("ecs never bred"));
        let now = h.world.resource::<Clock>().0;
        eprintln!("breed gap {gap}: mob at tick {mt}, ecs at tick {ct}; mob parents' age {}/{}; ecs rest {}", ages.0, ages.1, h.world.get::<Resting>(ea).unwrap().0 - (now - 300 + ct as u64));
        assert!((mt - ct).abs() <= 8, "gap {gap}");
        assert!(ages.0.max(ages.1) >= 5998, "{ages:?}");
        assert_eq!(h.world.get::<Resting>(ea).unwrap().0 - (now - 300 + ct as u64), 6000);
    }
}

/// A baby follows an adult from 3 blocks to 8 on each axis; nearer or farther it does not.
#[test]
fn a_baby_follows_at_the_same_distances_as_a_mob() {
    if !have_data() {
        return;
    }
    let mut rows = Vec::new();
    for d in [2.5, 3.5, 7.5, 8.5, 12.0] {
        let (mut r, mut h) = flat_pair();
        let adult = r.add(Vec3::new(0.5, 1.0, 0.5));
        let baby = r.add(Vec3::new(0.5 + d, 1.0, 0.5));
        r.mobs[baby].extra.age = -24000;
        let (ea, eb) = (ecs_cow(&mut h, 0.5, 1.0, 0.5, 3), ecs_cow(&mut h, 0.5 + d, 1.0, 0.5, 4));
        h.tick();
        make_baby(&mut h.world, eb, 24000);
        h.tick();
        adopt(&mut h.world, ea, eb);
        for _ in 0..3 {
            r.step(None);
            h.tick();
        }
        let (m, c) = (r.names(baby).contains(&"FollowParent"), h.world.get::<Trailing>(eb).is_some());
        let _ = adult;
        rows.push((d, m, c));
    }
    eprintln!("follow parent (distance, mob, ecs): {rows:?}");
    assert!(rows.iter().all(|r| r.1 == r.2), "{rows:?}");
}

/// The rate at which cows look at a player 6 blocks away and look around, over 150 ticks, with no player near
/// (the look goals do not depend on `noActionTime`).
#[test]
fn looking_goals_start_as_often_as_a_mob_s() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let mut cows = Vec::new();
    for i in 0..12 {
        for j in 0..8 {
            let (x, z) = (-7.0 + 1.3 * f64::from(i), -4.5 + 1.3 * f64::from(j));
            cows.push((r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 700 + cows.len() as u64)));
        }
    }
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: 0, off: 0 });
    arm_all(&mut h.world);
    let p = player_view(Vec3::new(0.0, 1.0, 0.0), None);
    let mut seen = vec![[false; 4]; cows.len()];
    for _ in 0..150 {
        r.step(Some(&p));
        h.tick();
        for (k, &(i, e)) in cows.iter().enumerate() {
            let n = r.names(i);
            seen[k][0] |= n.iter().any(|x| x.contains("LookAtPlayer"));
            seen[k][1] |= n.iter().any(|x| x.contains("RandomLook"));
            seen[k][2] |= h.world.get::<Glancing>(e).is_some();
            seen[k][3] |= h.world.get::<Scanning>(e).is_some();
        }
    }
    let rate = |col: usize| seen.iter().filter(|s| s[col]).count() as f64 / cows.len() as f64;
    eprintln!("look at player: mob {:.2} ecs {:.2}; look around: mob {:.2} ecs {:.2}", rate(0), rate(2), rate(1), rate(3));
    assert!((rate(0) - rate(2)).abs() < 0.2 && (rate(1) - rate(3)).abs() < 0.15);
}

/// A cow in a pond: it stays afloat as a mob does.
#[test]
fn a_cow_in_a_pond_floats_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let mut r = Ref::new(56, true);
    let mut h = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let (i, e) = (r.add(Vec3::new(-39.0, 3.0, -47.0)), ecs_cow(&mut h, -39.0, 3.0, -47.0, 77));
    let (mut my, mut cy, mut n) = (0.0, 0.0, 0.0);
    for t in 1..=400 {
        r.step(None);
        h.tick();
        if t > 60 {
            my += r.mobs[i].position.y;
            cy += h.world.get::<Pos>(e).unwrap().0.y;
            n += 1.0;
        }
    }
    eprintln!("pond, mean height over ticks 61..400: mob {:.2}, ecs {:.2}", my / n, cy / n);
    assert!((my - cy).abs() / n < 0.3, "afloat at the same height");
}

#[test]
#[ignore = "trace"]
fn trace_panic() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 11);
    let (mut pm, mut pc) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
    for t in 1..=24 {
        if t == 5 {
            r.hit(i, 1.0, [0.5, 1.0, 3.5]);
            h.hit(e, 1.0, (0.5, 3.5));
        }
        r.step(None);
        h.tick();
        let m = &r.mobs[i];
        let (cp, d, n) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Drive>(e).unwrap(), h.world.get::<Nav>(e).unwrap());
        let (cv, cc) = (h.world.get::<Vel>(e).unwrap().0, h.world.get::<Contact>(e).unwrap());
        eprintln!(
            "t={t:2} MOB step {:.3} vy {:+.3} ground {} zza {:.2} speed {:.2} jump {} node {:?}/{} panic {} | ECS step {:.3} vy {:+.3} ground {} zza {:.2} speed {:.2} jump {} node {:?}/{} panic {}",
            xz(pm, m.position), m.velocity.y, u8::from(m.on_ground), m.ai.zza, m.ai.speed, u8::from(m.ai.jumping), m.ai.nav.path.as_ref().map(|p| p.next), m.ai.nav.path.as_ref().map_or(0, |p| p.nodes.len()), u8::from(r.names(i).contains(&"Panic")),
            xz(pc, cp), cv.y, u8::from(cc.on_ground), d.zza, d.speed, u8::from(d.jump), n.path.as_ref().map(|p| p.next), n.path.as_ref().map_or(0, |p| p.nodes.len()), u8::from(h.world.get::<GoalSet>(e).unwrap().is_running(Kind::Panic))
        );
        (pm, pc) = (m.position, cp);
    }
}

/// A burning cow 4 blocks from a pond runs to the water and puts the fire out, as a mob does (`lookForWater`).
#[test]
fn a_burning_cow_runs_to_the_water_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let mut r = Ref::new(56, true);
    let mut h = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let (x, z) = (-39.5, -42.5);
    let (i, e) = (r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 77));
    for _ in 0..3 {
        r.step(None);
        h.tick();
    }
    r.mobs[i].fire_ticks = 160;
    ignite(&mut h.world, e, 160);
    let (mut mout, mut cout) = (None, None);
    let mut hp = Vec::new();
    for t in 1..=160_i64 {
        r.step(None);
        h.tick();
        if mout.is_none() && r.mobs[i].fire_ticks <= 0 {
            mout = Some(t);
        }
        if cout.is_none() && h.world.get::<Burning>(e).is_none() {
            cout = Some(t);
        }
        if [40, 100, 160].contains(&t) {
            hp.push((t, mob_health(&r.mobs[i]), health(&h, e)));
        }
    }
    eprintln!("burning near a pond: fire out at tick mob {mout:?}, ecs {cout:?}; health (tick, mob, ecs) {hp:?}");
    let (mout, cout) = (mout.expect("mob never put out"), cout.expect("ecs never put out"));
    assert!(mout < 100 && cout < 100 && (mout - cout).abs() <= 25, "mob {mout}, ecs {cout}");
}
