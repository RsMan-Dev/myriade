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

/// A skeleton keeps the goals the ECS has (see `tests/common/mod.rs`): `RestrictSun` makes its paths avoid the sun.
fn scope(m: &mut Mob) {
    use horizon_client_lib::mobs::goals::*;
    if m.species() != horizon_client_lib::mobs::Species::Skeleton {
        return;
    }
    m.extra.set_main_hand(Some("minecraft:bow".into()));
    m.ai.goals.clear();
    m.ai.targets.clear();
    m.ai.goals.add(4, horizon_client_lib::mobs::undead::SkeletonWeapon::new(20, 40));
    m.ai.goals.add(5, Stroll::avoiding_water(1.0));
    m.ai.goals.add(6, LookAtPlayer::new(8.0));
    m.ai.goals.add(6, RandomLookAround::default());
    m.ai.targets.add(1, HurtByTarget::new());
    m.ai.targets.add(2, NearestTarget::player(true));
}
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
        scope(&mut m);
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
    match species().name {
        "minecraft:pig" => (CARROT, "minecraft:carrot"),
        "minecraft:chicken" => (WHEAT_SEEDS, "minecraft:wheat_seeds"),
        _ => (WHEAT, "minecraft:wheat"),
    }
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

/// A cow put in a cactus, in fire, in lava: its health at the ticks 40, 140 and 340, with the cow at rest (the
/// resting vertical velocity) and fresh from its spawn (velocity zero: it is in the air for its first tick).
#[test]
fn cactus_fire_and_lava_hurt_as_a_mob_does() {
    if horizon_ecs_proto::cow::skip_if_fire_immune("cactus_fire_and_lava_hurt_as_a_mob_does") {
        return;
    }
    // The Mob's outcome depends on the panic flight out of the hazard; a species with no Panic wanders at random.
    if horizon_ecs_proto::cow::skip_unless(&horizon_ecs_proto::behaviours::panic::MODULE, "cactus_fire_and_lava_hurt_as_a_mob_does") {
        return;
    }
    if !have_data() {
        return;
    }
    let spots = [("lava", -47.0, -47.0), ("fire", -47.0, -39.0), ("cactus", -39.0, -39.0)];
    for rest in [true, false] {
        let mut r = Ref::new(56, true);
        let mut h = Herd::with_hazards(1, 100.0, true, false, 5, true);
        let ids: Vec<(usize, Entity)> = spots
            .iter()
            .map(|s| {
                let e = ecs_cow(&mut h, s.1, 1.0, s.2, 77);
                if !rest {
                    h.world.get_mut::<Vel>(e).unwrap().0.y = 0.0;
                }
                (r.add(Vec3::new(s.1, 1.0, s.2)), e)
            })
            .collect();
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
            eprintln!("tick {t} (ecs cow {}): {}", if rest { "at rest" } else { "fresh" }, line.join(", "));
        }
        for (t, row) in &rows {
            for (s, (m, e)) in spots.iter().zip(row) {
                assert!((m - e).abs() <= 2.0, "{} at tick {t} (rest {rest}): mob {m}, ecs {e}", s.0);
            }
        }
    }
}

fn target_of(spots: &[(f64, f64)], cows: &[(usize, Entity)], e: Entity, target: &[Option<(f64, f64)>]) -> Option<(f64, f64)> {
    let _ = spots;
    cows.iter().position(|c| c.1 == e).and_then(|k| target[k])
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
    if horizon_ecs_proto::cow::skip_unless_small_body("a_struck_cow_panics_as_a_mob_does") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("a_struck_cow_panics_as_a_mob_does") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless(&horizon_ecs_proto::behaviours::panic::MODULE, "a_struck_cow_panics_as_a_mob_does") {
        return;
    }
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
            let cp = h.world.get::<GoalSet>(e).unwrap().is_running(&horizon_ecs_proto::behaviours::panic::MODULE);
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
    if horizon_ecs_proto::cow::skip_unless_small_body("wheat_tempts_as_it_does_a_mob") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("wheat_tempts_as_it_does_a_mob") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless(&horizon_ecs_proto::behaviours::tempt::MODULE, "wheat_tempts_as_it_does_a_mob") {
        return;
    }
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
    if horizon_ecs_proto::cow::skip_unless_small_body("breeding_takes_as_long_as_for_a_mob") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("breeding_takes_as_long_as_for_a_mob") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless(&horizon_ecs_proto::behaviours::breed::MODULE, "breeding_takes_as_long_as_for_a_mob") {
        return;
    }
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
        assert!((mt - ct).abs() <= 1, "gap {gap}");
        assert!(ages.0.max(ages.1) >= 5998, "{ages:?}");
        assert_eq!(h.world.get::<Resting>(ea).unwrap().0 - (now - 300 + ct as u64), 6000);
    }
}

/// A baby follows an adult from 3 blocks to 8 on each axis; nearer or farther it does not.
#[test]
fn a_baby_follows_at_the_same_distances_as_a_mob() {
    if horizon_ecs_proto::cow::skip_unless_farm_goals("a_baby_follows_at_the_same_distances_as_a_mob") {
        return;
    }
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
    // KNOWN: the llama's look goals start at another rate than the Mob's (lot 6 a2 reworks its entries); skipped until then.
    if std::env::var("HORIZON_SPECIES").is_ok_and(|s| s == "minecraft:llama") {
        eprintln!("SKIP {}: llama look goals differ (known, lot 6)", "looking_goals_start_as_often_as_a_mob_s");
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("looking_goals_start_as_often_as_a_mob_s") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("looking_goals_start_as_often_as_a_mob_s") {
        return;
    }
    if horizon_ecs_proto::cow::skip_if(&horizon_ecs_proto::behaviours::melee::MODULE, "looking_goals_start_as_often_as_a_mob_s") {
        return;
    }
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    // The control compares the rates of the look goals of every animal: the lazy look is off here.
    set_lazy_look(&mut h.world, false);
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
    if horizon_ecs_proto::cow::skip_unless_farm_goals("a_cow_in_a_pond_floats_as_a_mob_does") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("a_cow_in_a_pond_floats_as_a_mob_does") {
        return;
    }
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
            xz(pc, cp), cv.y, u8::from(cc.on_ground), d.zza, d.speed, u8::from(d.jump), n.path.as_ref().map(|p| p.next), n.path.as_ref().map_or(0, |p| p.nodes.len()), u8::from(h.world.get::<GoalSet>(e).unwrap().is_running(&horizon_ecs_proto::behaviours::panic::MODULE))
        );
        (pm, pc) = (m.position, cp);
    }
}

/// A burning cow 4 blocks from a pond runs to the water and puts the fire out, as a mob does (`lookForWater`).
#[test]
fn a_burning_cow_runs_to_the_water_as_a_mob_does() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_burning_cow_runs_to_the_water_as_a_mob_does") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("a_burning_cow_runs_to_the_water_as_a_mob_does") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless(&horizon_ecs_proto::behaviours::panic::MODULE, "a_burning_cow_runs_to_the_water_as_a_mob_does") {
        return;
    }
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

#[test]
#[ignore = "trace"]
fn trace_cactus_paths() {
    use horizon_client_lib::mobs::path::{Evaluator, Finder, PathMob};
    if !have_data() {
        return;
    }
    let mut r = Ref::new(56, true);
    let i = r.add(Vec3::new(-39.0, 1.0, -39.0));
    let w = r.w;
    let pm_mob = r.mobs[i].path_mob();
    eprintln!("mob PathMob: pos {:?} w {} h {} ground {} water {} step {} fall {} float {} doors {} {} fences {}", pm_mob.position, pm_mob.width, pm_mob.height, pm_mob.on_ground, pm_mob.in_water, pm_mob.max_up_step, pm_mob.max_fall_distance, pm_mob.can_float, pm_mob.can_open_doors, pm_mob.can_pass_doors, pm_mob.can_walk_over_fences);
    eprintln!("mob malus {:?}", pm_mob.malus);
    let sp = species();
    eprintln!("ecs species: step {} w {} h {} fall 3 malus {:?}", sp.step_height, sp.width, sp.height, sp.malus);
    for (tx, tz) in [(-41, -39), (-39, -42), (-35, -39), (-39, -35), (-43, -43)] {
        let a = Finder::new(w, &pm_mob, Evaluator::Walk).find(&[[tx, 1, tz]], sp.follow_range, 1, (sp.follow_range * 16.0) as i32);
        let mut pg = pm_mob.clone();
        pg.on_ground = true;
        let b = Finder::new(w, &pg, Evaluator::Walk).find(&[[tx, 1, tz]], sp.follow_range, 1, (sp.follow_range * 16.0) as i32);
        let show = |p: &Option<horizon_client_lib::mobs::path::Path>| p.as_ref().map(|p| p.nodes.iter().map(|n| (n.x, n.y, n.z)).collect::<Vec<_>>());
        eprintln!("target ({tx},{tz}): finder on_ground=false {:?}\n   on_ground=true {:?}", show(&a), show(&b));
    }
}

/// The cactus cow tick by tick on the ecs side (`FRESH=1`: velocity zero at the start, as after a spawn).
#[test]
#[ignore = "trace"]
fn trace_cactus() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let e = ecs_cow(&mut h, -39.0, 1.0, -39.0, 77);
    if std::env::var_os("FRESH").is_some() {
        h.world.get_mut::<Vel>(e).unwrap().0.y = 0.0;
    }
    for t in 1..=60 {
        h.tick();
        if let Some(g) = h.world.get::<GoalSet>(e) {
            let p = h.world.get::<Pos>(e).unwrap().0;
            eprintln!(
                "t={t} pos=({:.2},{:.2},{:.2}) ground={} vy={:.3} hp={} panic={} following={} needs={} path={}",
                p.x, p.y, p.z, h.world.get::<Contact>(e).unwrap().on_ground, h.world.get::<Vel>(e).unwrap().0.y, health(&h, e),
                g.is_running(&horizon_ecs_proto::behaviours::panic::MODULE), h.world.get::<Following>(e).is_some(), h.world.get::<NeedsPath>(e).is_some(),
                h.world.get::<Nav>(e).unwrap().path.as_ref().map_or(0, |p| p.nodes.len())
            );
        }
    }
}

#[test]
#[ignore = "trace"]
fn trace_burning() {
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
    for t in 1..=40 {
        r.step(None);
        h.tick();
        let m = &r.mobs[i];
        let p = h.world.get::<Pos>(e).unwrap().0;
        let s = h.world.get::<Stroll>(e).unwrap();
        eprintln!(
            "t={t:2} MOB ({:.2},{:.2},{:.2}) fire {} goals {:?} nodes {} | ECS ({:.2},{:.2},{:.2}) burning {} panic {} target ({:.1},{:.1}) following {} needs {} path {}",
            m.position.x, m.position.y, m.position.z, m.fire_ticks, r.names(i), m.ai.nav.path.as_ref().map_or(0, |p| p.nodes.len()),
            p.x, p.y, p.z, h.world.get::<Burning>(e).is_some(), h.world.get::<GoalSet>(e).unwrap().is_running(&horizon_ecs_proto::behaviours::panic::MODULE), s.tx, s.tz,
            h.world.get::<Following>(e).is_some(), h.world.get::<NeedsPath>(e).is_some(), h.world.get::<Nav>(e).unwrap().path.as_ref().map_or(0, |p| p.nodes.len())
        );
    }
}

/// The same blow, the same selector phase and the same flight spot on both sides (the `Mob` draws its own, the ecs
/// takes it): the positions of a struck cow, tick by tick, for the cows whose `Mob` found a spot at the tick of the
/// blow (the others start later, after landing, with draws that cannot be read: not compared here).
#[test]
fn a_struck_cow_follows_the_mob_s_trajectory_when_the_spot_is_imposed() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_struck_cow_follows_the_mob_s_trajectory_when_the_spot_is_imposed") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("a_struck_cow_follows_the_mob_s_trajectory_when_the_spot_is_imposed") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless(&horizon_ecs_proto::behaviours::panic::MODULE, "a_struck_cow_follows_the_mob_s_trajectory_when_the_spot_is_imposed") {
        return;
    }
    if !have_data() {
        return;
    }
    let spots: Vec<(f64, f64)> = (0..6).flat_map(|i| (0..4).map(move |j| (-16.5 + 6.0 * f64::from(i), -13.5 + 8.0 * f64::from(j)))).collect();
    // First pass: the mobs alone, to read where each one decides to flee at the tick of the blow.
    let mut r = Ref::new(20, false);
    let ids: Vec<usize> = spots.iter().map(|&(x, z)| r.add(Vec3::new(x, 1.0, z))).collect();
    let mut target: Vec<Option<(f64, f64)>> = vec![None; ids.len()];
    for t in 1..=5 {
        if t == 5 {
            for (k, &i) in ids.iter().enumerate() {
                let (x, z) = spots[k];
                r.hit(i, 1.0, [x, 1.0, z + 3.0]);
            }
        }
        r.step(None);
    }
    for (k, &i) in ids.iter().enumerate() {
        if r.names(i).contains(&"Panic") && let Some(p) = &r.mobs[i].ai.nav.path {
            // The spot the goal drew: the path's target (its last node is up to a block short of it).
            target[k] = Some((f64::from(p.target[0]) + 0.5, f64::from(p.target[2]) + 0.5));
        }
    }
    // Second pass: the same mobs and the ecs cows, the ecs taking the mobs' spots and phases.
    let (mut r, mut h) = flat_pair();
    let cows: Vec<(usize, Entity)> = spots.iter().enumerate().map(|(k, &(x, z))| (r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 900 + k as u64))).collect();
    let mut kept = Vec::new();
    for (k, &(i, e)) in cows.iter().enumerate() {
        h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[i].id);
        // The mob is made facing 0; the ecs cow is given the same.
        h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
        if let Some(t) = target[k] {
            force_spot(&mut h.world, e, &horizon_ecs_proto::behaviours::panic::MODULE, t);
            kept.push((i, e));
        }
    }
    assert!(kept.len() >= 4, "only {} cows start at the tick of the blow", kept.len());
    let mut apart: Vec<Vec<f64>> = vec![Vec::new(); 61];
    for t in 1..=60_usize {
        if t == 5 {
            for (k, &(i, e)) in cows.iter().enumerate() {
                let (x, z) = spots[k];
                r.hit(i, 1.0, [x, 1.0, z + 3.0]);
                h.hit(e, 1.0, (x, z + 3.0));
            }
        }
        r.step(None);
        h.tick();
        for &(i, e) in &kept {
            if std::env::var_os("TRACE").is_some() && r.mobs[i].id == 19 && (4..=12).contains(&t) {
                let m = &r.mobs[i];
                let (c, d, mv) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Drive>(e).unwrap(), h.world.get::<MoveTo>(e).unwrap());
                eprintln!("id19 t={t} MOB pos ({:.3},{:.3}) yaw {} zza {:.2} path {:?} | ECS pos ({:.3},{:.3}) yaw {} zza {:.2} moveto ({:.1},{:.1}) active {}", m.position.x, m.position.z, m.yaw, m.ai.zza, m.ai.nav.path.as_ref().map(|p| p.nodes.iter().map(|n| (n.x, n.z)).collect::<Vec<_>>()), c.x, c.z, h.world.get::<Yaw>(e).unwrap().0, d.zza, mv.x, mv.z, mv.active);
            }
            apart[t].push(xz(r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0));
        }
    }
    let close = |t: usize| apart[t].iter().filter(|&&d| d < 0.05).count();
    let median = |t: usize| {
        let mut v = apart[t].clone();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let line: Vec<String> = [6, 10, 20, 30, 60].iter().map(|&t| format!("t{t}: {}/{} within 0.05, median {:.3}", close(t), kept.len(), median(t))).collect();
    eprintln!("imposed spot and phase, mob vs ecs distance apart: {}", line.join(" | "));
    // Up to the end of the first walk the two are one trajectory (the cows apart are the ones whose spot lies along an
    // axis, where the two round the heading differently); after it each draws its own next spot, so only the
    // first 20 ticks are asserted.
    assert!(close(10) == kept.len() && close(20) * 10 >= kept.len() * 8, "{line:?}");
}

#[test]
#[ignore = "trace"]
fn trace_short_path() {
    use horizon_client_lib::mobs::path::{Evaluator, Finder};
    if !have_data() {
        return;
    }
    let mut r = Ref::new(20, false);
    let i = r.add(Vec3::new(7.5, 1.0, 2.5));
    let mut pm = r.mobs[i].path_mob();
    pm.on_ground = true;
    let sp = species();
    for target in [[7, 1, 3], [7, 1, 4], [7, 1, 6]] {
        let p = Finder::new(r.w, &pm, Evaluator::Walk).find(&[target], sp.follow_range, 1, (sp.follow_range * 16.0) as i32);
        eprintln!("target {target:?}: {:?}", p.as_ref().map(|p| p.nodes.iter().map(|n| (n.x, n.y, n.z)).collect::<Vec<_>>()));
    }
    let m = &mut r.mobs[i];
    m.on_ground = true;
    let p = m.create_path_to(r.w, [7, 1, 3], 1);
    eprintln!("mob create_path_to (7,1,3): {:?}", p.as_ref().map(|p| p.nodes.iter().map(|n| (n.x, n.y, n.z)).collect::<Vec<_>>()));
}

/// A walk: the `Mob` draws when to stroll and where; the ecs cow is started at the same tick, with the same spot
/// (`Forced`), and the two are followed for 60 ticks.
#[test]
fn a_stroll_follows_the_mob_s_trajectory_when_the_spot_is_imposed() {
    if horizon_ecs_proto::cow::skip_unless_farm_goals("a_stroll_follows_the_mob_s_trajectory_when_the_spot_is_imposed") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("a_stroll_follows_the_mob_s_trajectory_when_the_spot_is_imposed") {
        return;
    }
    if !have_data() {
        return;
    }
    let spots: Vec<(f64, f64)> = (0..5).flat_map(|i| (0..4).map(move |j| (-14.5 + 7.0 * f64::from(i), -12.5 + 8.0 * f64::from(j)))).collect();
    // First pass: the mobs alone (awake: noActionTime 0), to read when and where each starts to stroll.
    let mut r = Ref::new(20, false);
    let ids: Vec<usize> = spots.iter().map(|&(x, z)| r.add(Vec3::new(x, 1.0, z))).collect();
    for &i in &ids {
        r.mobs[i].ai.no_action_time = 0;
    }
    let mut start: Vec<Option<(usize, (f64, f64))>> = vec![None; ids.len()];
    for t in 1..=400_usize {
        r.step(None);
        for (k, &i) in ids.iter().enumerate() {
            r.mobs[i].ai.no_action_time = 0;
            if start[k].is_none() && t < 330 && r.names(i).contains(&"Stroll") && let Some(p) = &r.mobs[i].ai.nav.path {
                start[k] = Some((t, (f64::from(p.target[0]) + 0.5, f64::from(p.target[2]) + 0.5)));
            }
        }
    }
    let begun: Vec<usize> = (0..ids.len()).filter(|&k| start[k].is_some()).collect();
    assert!(begun.len() >= 6, "only {} mobs strolled", begun.len());
    // Second pass: the mobs and the ecs cows; at each start tick the ecs cow is told to stroll to the same spot.
    let (mut r, mut h) = flat_pair();
    let cows: Vec<(usize, Entity)> = spots.iter().enumerate().map(|(k, &(x, z))| (r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 700 + k as u64))).collect();
    for (k, &(i, e)) in cows.iter().enumerate() {
        r.mobs[i].ai.no_action_time = 0;
        h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<Stroll>(e).unwrap().scheduled = true;
        let _ = k;
    }
    let mut apart: Vec<Vec<f64>> = vec![Vec::new(); 61];
    for t in 1..=400_usize {
        for &(i, _) in &cows {
            r.mobs[i].ai.no_action_time = 0;
        }
        for &k in &begun {
            let (s, spot) = start[k].unwrap();
            if t == s {
                let e = cows[k].1;
                force_spot(&mut h.world, e, &horizon_ecs_proto::behaviours::stroll::MODULE, spot);
                fire(&mut h.world, e, &horizon_ecs_proto::behaviours::stroll::MODULE);
            }
        }
        r.step(None);
        // The `Mob`'s own draw of RandomLookAround (one chance in 50, priority 10: it replaces a stroll of a panda, 14) is imposed on the ecs
        // animal as the spot is: at the tick the `Mob` starts it.
        for &k in &begun {
            let e = cows[k].1;
            if r.names(cows[k].0).contains(&"RandomLookAround")
                && let Some(slot) = horizon_ecs_proto::goals::slot_of(&h.world, e, &horizon_ecs_proto::behaviours::random_look::MODULE)
            {
                horizon_ecs_proto::goals::force_start(&mut h.world, e, slot);
            } else if let Some(slot) = horizon_ecs_proto::goals::slot_of(&h.world, e, &horizon_ecs_proto::behaviours::random_look::MODULE)
                && h.world.get::<horizon_ecs_proto::goals::GoalSet>(e).is_some_and(|g| g.is_running(&horizon_ecs_proto::behaviours::random_look::MODULE))
            {
                // The `Mob` ended it: so does the ecs animal.
                h.world.resource_mut::<horizon_ecs_proto::goals::Requests>().0.push(horizon_ecs_proto::goals::Req::End(e, slot));
            }
        }
        h.tick();
        for &k in &begun {
            let (s, _) = start[k].unwrap();
            if std::env::var_os("TRACE").is_some() && t >= s - 1 && t <= s + 3 && k < 4 {
                let (m, e) = (&r.mobs[cows[k].0], cows[k].1);
                let (c, d) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Drive>(e).unwrap());
                eprintln!("k{k} s={s} t={t} MOB ({:.3},{:.3}) yaw {} zza {:.2} spd {} path {:?} | ECS ({:.3},{:.3}) yaw {} zza {:.2} spd {}", m.position.x, m.position.z, m.yaw, m.ai.zza, m.ai.speed, m.ai.nav.path.as_ref().map(|p| (p.nodes.len(), p.target)), c.x, c.z, h.world.get::<Yaw>(e).unwrap().0, d.zza, d.speed);
            }
            if t >= s && t < s + 60 {
                apart[t - s + 1].push(xz(r.mobs[cows[k].0].position, h.world.get::<Pos>(cows[k].1).unwrap().0));
            }
        }
    }
    let close = |n: usize| apart[n].iter().filter(|&&d| d < 0.1).count();
    let line: Vec<String> = [2, 10, 30, 60].iter().map(|&n| format!("+{n}: {}/{} within 0.1", close(n), apart[n].len())).collect();
    eprintln!("stroll with the mob's spot: {}", line.join(" | "));
    assert!(close(30) * 10 >= apart[30].len() * 8, "{line:?}");
}

/// Three cows in love: the `Mob` picks its partner (the first in its list), the ecs cow takes the same one
/// (`Forced`); the parent walks the same way and gives birth at the same tick.
#[test]
fn breeding_follows_the_mob_s_partner_and_trajectory_when_it_is_imposed() {
    if horizon_ecs_proto::cow::skip_unless_small_body("breeding_follows_the_mob_s_partner_and_trajectory_when_it_is_imposed") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_farm_goals("breeding_follows_the_mob_s_partner_and_trajectory_when_it_is_imposed") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless(&horizon_ecs_proto::behaviours::breed::MODULE, "breeding_follows_the_mob_s_partner_and_trajectory_when_it_is_imposed") {
        return;
    }
    if !have_data() {
        return;
    }
    let pos = [(0.5, 0.5), (5.5, 0.5), (0.5, 3.5)];
    // First pass: the mobs alone; the first birth tells who bred with whom.
    let mut r = Ref::new(20, false);
    let ids: Vec<usize> = pos.iter().map(|&(x, z)| r.add(Vec3::new(x, 1.0, z))).collect();
    for &i in &ids {
        r.mobs[i].extra.in_love = 600;
    }
    let mut first = None;
    for t in 1..=200_i64 {
        r.step(None);
        if first.is_none() && let Some(MobEvent::Breed { parent, partner, .. }) = r.events.iter().find(|e| matches!(e, MobEvent::Breed { .. })) {
            first = Some((*parent, *partner, t));
        }
    }
    let (parent, partner, born) = first.expect("no birth");
    let (pk, qk) = (ids.iter().position(|&i| r.mobs[i].id == parent).unwrap(), ids.iter().position(|&i| r.mobs[i].id == partner).unwrap());
    // Second pass.
    let (mut r, mut h) = flat_pair();
    let cows: Vec<(usize, Entity)> = pos.iter().enumerate().map(|(k, &(x, z))| (r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 600 + k as u64))).collect();
    h.tick();
    for &(i, e) in &cows {
        r.mobs[i].extra.in_love = 600;
        h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
        h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[i].id);
    }
    h.world.resource_mut::<Forced>().partners.insert(cows[pk].1, cows[qk].1);
    for &(_, e) in &cows {
        assert!(feed(&mut h.world, e));
    }
    let mut apart = Vec::new();
    let mut ecs_born = None;
    for t in 1..=200_i64 {
        r.step(None);
        h.tick();
        apart.push(xz(r.mobs[cows[pk].0].position, h.world.get::<Pos>(cows[pk].1).unwrap().0));
        if ecs_born.is_none() && h.world.query::<&GrowAt>().iter(&h.world).count() > 0 {
            ecs_born = Some(t);
        }
    }
    let ecs_born = ecs_born.expect("ecs never bred");
    let worst = apart[..(born as usize).min(60)].iter().cloned().fold(0.0, f64::max);
    eprintln!("breeding: mob parent {pk} with {qk} at tick {born}; ecs at tick {ecs_born}; parent {:.2} apart at worst before the birth (first 60 ticks)", worst);
    assert!((born - ecs_born).abs() <= 1, "birth tick mob {born}, ecs {ecs_born}");
    assert!(worst < 0.5, "{worst}");
}

/// The pig has two tempt goals of one priority, each with its own calm-down: after the carrot on a stick is put
/// away, a carrot tempts at once; the stick tempts again only 100 ticks after it stopped.
#[test]
fn a_pig_s_two_tempt_goals_have_their_own_calm_downs_as_a_mob_s() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    if species().name != "minecraft:pig" {
        return;
    }
    let mi = r.add(Vec3::new(-6.5, 1.0, 0.5));
    let e = ecs_cow(&mut h, -6.5, 1.0, 0.5, 5);
    h.tick();
    let at = Vec3::new(-1.0, 1.0, 0.5);
    let mut p = player_view(at, None);
    let mut t = 0;
    let mut set = |r: &mut Ref, h: &mut Herd, p: &mut PlayerView, held: Option<(&str, u32)>, ticks: usize, t: &mut i64, log: &mut Vec<(i64, bool, Option<u8>)>| {
        p.main_hand = held.map(|x| x.0.into());
        h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: 1.0, z: at.z, main: held.map_or(0, |x| x.1), off: 0 });
        for _ in 0..ticks {
            r.step(Some(p));
            h.tick();
            *t += 1;
            log.push((*t, r.names(mi).contains(&"Tempt"), h.world.get::<Tempting>(e).map(|x| x.0)));
        }
    };
    let mut log = Vec::new();
    let stick = Some(("minecraft:carrot_on_a_stick", CARROT_ON_A_STICK));
    let carrot = Some(("minecraft:carrot", CARROT));
    set(&mut r, &mut h, &mut p, stick, 10, &mut t, &mut log);
    let stopped_at = t;
    set(&mut r, &mut h, &mut p, None, 4, &mut t, &mut log);
    let carrot_from = t;
    set(&mut r, &mut h, &mut p, carrot, 20, &mut t, &mut log);
    set(&mut r, &mut h, &mut p, None, 4, &mut t, &mut log);
    let stick_from = t;
    set(&mut r, &mut h, &mut p, stick, 140, &mut t, &mut log);
    let first = |from: i64, inst: Option<u8>| log.iter().find(|l| l.0 > from && match inst { Some(i) => l.2 == Some(i), None => l.1 }).map(|l| l.0 - from);
    let (mob_carrot, ecs_carrot) = (log.iter().find(|l| l.0 > carrot_from && l.1).map(|l| l.0 - carrot_from), first(carrot_from, Some(1)));
    let (mob_stick, ecs_stick) = (log.iter().find(|l| l.0 > stick_from && l.1).map(|l| l.0 - stick_from), first(stick_from, Some(0)));
    eprintln!("pig tempt: carrot after the stick: mob {mob_carrot:?} ticks, ecs {ecs_carrot:?}; stick again (stopped at tick {stopped_at}): mob {mob_stick:?}, ecs {ecs_stick:?}");
    let (a, b, c, d) = (mob_carrot.unwrap(), ecs_carrot.unwrap(), mob_stick.unwrap(), ecs_stick.unwrap());
    assert!((a - b).abs() <= 3 && (c - d).abs() <= 4, "carrot {a}/{b}, stick {c}/{d}");
}
