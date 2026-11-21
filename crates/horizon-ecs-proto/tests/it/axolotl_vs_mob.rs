//! The brain of the axolotl against the real one (`HORIZON_TEST_RESOURCES` set, process species `minecraft:axolotl`): the birth
//! of two animals in love (`make_love`: the tick of the baby is `60 + nextInt(50)` ticks after the start, which waits for the
//! first scan of the sensors), and what the walk toward the partner does. The hazard draws are the entities' own, so the
//! controls compare means over many pairs.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{animal_brain, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::breed::feed, brain::present, cow::*, goals::*};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:axolotl"])
}
fn mean(v: &[Option<i64>]) -> (f64, usize) {
    let n = v.iter().flatten().count();
    (v.iter().flatten().sum::<i64>() as f64 / n.max(1) as f64, n)
}
struct Pair {
    r: MobRef,
    h: Herd,
    /// Per pair: (mob index a, mob index b, entity a, entity b).
    ids: Vec<(usize, usize, Entity, Entity)>,
    routed: usize,
}
/// `n` pairs of axolotls `gap` blocks apart, in love on both sides.
fn pairs(n: usize, gap: f64, seed: u64) -> Pair {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let mut ids = Vec::new();
    for k in 0..n {
        let (x, z) = (-50.0 + 12.0 * (k % 4) as f64 + 0.5, -40.0 + 12.0 * (k / 4) as f64 + 0.5);
        let mut one = |dx: f64, salt: u32| {
            let i = r.add_id(Vec3::new(x + dx, 1.0, z), ((k as u32 * 2 + salt + 1).wrapping_mul(0x9E37_79B1) >> 4) | 1);
            let e = spawn_species(&mut h.world, &sp, x + dx, z, (k as u64 * 2 + u64::from(salt) + 1 + seed * 1000).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
            h.world.resource_mut::<Entered>().0.push(e);
            (i, e)
        };
        let (ia, ea) = one(0.0, 0);
        let (ib, eb) = one(gap, 1);
        ids.push((ia, ib, ea, eb));
    }
    Pair { r, h, ids, routed: 0 }
}
impl Pair {
    fn love(&mut self) {
        for &(ia, ib, ea, eb) in &self.ids {
            self.r.mobs[ia].extra.in_love = 600;
            self.r.mobs[ib].extra.in_love = 600;
            assert!(feed(&mut self.h.world, ea) && feed(&mut self.h.world, eb));
        }
    }
    /// A tick on both sides; the `Mob`'s events that the game sends to the partner (`BreedTarget`, `LockGaze`) reach it.
    fn step(&mut self) {
        self.r.step(None);
        let fresh: Vec<MobEvent> = self.r.events[self.routed..].to_vec();
        self.routed = self.r.events.len();
        for ev in fresh {
            match ev {
                MobEvent::BreedTarget { mob, partner } => {
                    // A love target is addressed to one animal, and to a free one.
                    let free = |m: &horizon_client_lib::mobs::Mob| !animal_brain::describe(m).is_some_and(|(a, b)| format!("{a};{b}").contains("breed_target="));
                    if let Some(m) = self.r.mobs.iter_mut().find(|m| m.id == mob && free(m)) {
                        animal_brain::set_breed_partner(m, partner);
                    }
                }
                MobEvent::LockGaze { mob, partner } => {
                    if let Some(m) = self.r.mobs.iter_mut().find(|m| m.id == mob) {
                        animal_brain::lock_gaze(m, partner);
                    }
                }
                _ => {}
            }
        }
        self.h.tick();
    }
}

/// One scene of 16 pairs 2 blocks apart (the birth needs less than 3), both in love, 12 blocks between pairs (inside the sight of an axolotl): the tick of the first baby on each side and the tick the first
/// `breed_target` appears.
fn scene(seed: u64) -> Vec<[Option<i64>; 4]> {
    let mut p = pairs(16, 2.0, seed);
    for _ in 0..5 {
        p.step();
    }
    p.love();
    let n = p.ids.len();
    let mut when = vec![[None::<i64>; 4]; n];
    let has = |m: &horizon_client_lib::mobs::Mob, name: &str| animal_brain::describe(m).is_some_and(|(a, b)| format!("{a};{b}").contains(&format!("{name}=")));
    for t in 1..=260i64 {
        p.step();
        let now: Vec<(f64, f64)> = p.h.world.query_filtered::<&Pos, With<GrowAt>>().iter(&p.h.world).map(|q| (q.0.x, q.0.z)).collect();
        for (k, &(ia, ib, ea, eb)) in p.ids.iter().enumerate() {
            let (a, b) = (p.r.mobs[ia].id, p.r.mobs[ib].id);
            if when[k][0].is_none() && p.r.events.iter().any(|e| matches!(e, MobEvent::Breed { parent, partner, .. } if [a, b].contains(parent) && [a, b].contains(partner))) {
                when[k][0] = Some(t);
            }
            if when[k][2].is_none() && (has(&p.r.mobs[ia], "breed_target") || has(&p.r.mobs[ib], "breed_target")) {
                when[k][2] = Some(t);
            }
            if when[k][3].is_none() && (present(&p.h.world, ea, &horizon_ecs_proto::brain::BREED_TARGET) || present(&p.h.world, eb, &horizon_ecs_proto::brain::BREED_TARGET)) {
                when[k][3] = Some(t);
            }
            let at = p.h.world.get::<Pos>(ea).unwrap().0;
            if when[k][1].is_none() && now.iter().any(|q| (q.0 - at.x).abs() < 4.0 && (q.1 - at.z).abs() < 4.0) {
                when[k][1] = Some(t);
            }
        }
    }
    when
}

/// 6 scenes (96 pairs): the tick of the first birth, `60 + nextInt(50)` after the start (which waits for the first scan of `visible_mobs`,
/// 0 to 19 ticks): the same mean on both sides, and the same start.
#[test]
fn the_birth_comes_as_late_as_the_mob_s() {
    if skip() {
        return;
    }
    let all: Vec<[Option<i64>; 4]> = (0..6).flat_map(scene).collect();
    let col = |c: usize| mean(&all.iter().map(|w| w[c]).collect::<Vec<_>>());
    let (m, e, ms, es) = (col(0), col(1), col(2), col(3));
    eprintln!("start of the love (96 pairs): mob {:.1} ({}), ecs {:.1} ({})", ms.0, ms.1, es.0, es.1);
    eprintln!("first birth (96 pairs): mob {:.1} ({} births), ecs {:.1} ({} births); expected {:.1} + start", m.0, m.1, e.0, e.1, 60.0 + 24.5);
    assert!(m.1 == all.len() && e.1 == all.len(), "every pair breeds: mob {} ecs {} of {}", m.1, e.1, all.len());
    assert!((ms.0 - es.0).abs() < 2.0, "start: mob {:.1}, ecs {:.1}", ms.0, es.0);
    assert!((m.0 - e.0).abs() < 4.0, "birth tick: mob {:.1}, ecs {:.1}", m.0, e.0);
}

/// The imposed route of `swim_controls_vs_mob`, for the axolotl, traced: where the `Mob` goes and what its brain holds.
#[test]
fn trace_the_imposed_route() {
    if skip() || std::env::var_os("TRACE_ROUTE").is_none() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    for x in -12..=-1 {
        for z in -12..=12 {
            for y in 1..=4 {
                r.set_block(x, y, z, "minecraft:water");
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    h.tick();
    let (s, g) = ([-8.5, 2.0, 5.5], [-3.5, 3.0, -6.5]);
    let reissue = std::env::var_os("REISSUE").is_some();
    let (i, e) = (r.add(Vec3::new(s[0], s[1], s[2])), ecs_cow(&mut h, s[0], s[1], s[2], 2100));
    r.mobs[i].ai.goals = Default::default();
    r.mobs[i].ai.targets = Default::default();
    if std::env::var_os("KEEP_BRAIN").is_none() {
        r.mobs[i].extra.animal_brain = None;
    }
    h.world.entity_mut(e).remove::<GoalSet>();
    h.world.entity_mut(e).remove::<horizon_ecs_proto::brain::Brain>();
    h.tick();
    h.tick();
    h.teleport(e, Vec3::new(s[0], s[1], s[2]));
    for _ in 0..8 {
        r.mobs[i].ai.no_action_time = 0;
        r.step(None);
        h.tick();
    }
    let mw = r.w;
    r.mobs[i].nav_move_to(mw, g, 1.0);
    horizon_ecs_proto::goals::go_to_at(&mut h.world, e, g, 1.0, 1);
    for t in 1..=120_usize {
        r.mobs[i].ai.no_action_time = 0;
        if reissue {
            r.mobs[i].nav_move_to(mw, g, 1.0);
        }
        r.step(None);
        h.tick();
        if t % 5 == 0 && t <= 60 {
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            eprintln!("t{t} walk {:?}", horizon_ecs_proto::brain::walk_target(&h.world, e));
            eprintln!("t{t} ECS needs_path {} following {} stroll {:?} nav {:?} vel {:?} fluid {:?} modes swim {} walk {}", h.world.get::<NeedsPath>(e).is_some(), h.world.get::<Following>(e).is_some(), h.world.get::<horizon_ecs_proto::cow::Stroll>(e).map(|s| (s.tx, s.ty, s.tz)), h.world.get::<horizon_ecs_proto::cow::Nav>(e).map(|n| (n.speed, n.reach)), h.world.get::<Vel>(e).map(|v| v.0), h.world.get::<FluidState>(e).map(|f| (f.in_water, f.water)), h.world.get::<horizon_ecs_proto::locomotion::SwimMode>(e).is_some(), h.world.get::<horizon_ecs_proto::locomotion::WalkMode>(e).is_some());
            eprintln!("t{t} MOB ({:.2},{:.2},{:.2}) nav_done {} | ECS ({:.2},{:.2},{:.2}) | {:?}", a.x, a.y, a.z, r.mobs[i].ai.nav.is_done(), b.x, b.y, b.z, animal_brain::describe(&r.mobs[i]).map(|d| d.1.chars().take(160).collect::<String>()));
        }
    }
}

fn pool(r: &MobRef, h: &mut Herd) {
    for x in -30..=-1 {
        for z in -30..=30 {
            for y in 1..=4 {
                r.set_block(x, y, z, "minecraft:water");
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    // A wall round the pool, on both sides: a swimmer pushed past the edge (the knock-back of a blow) would walk on the beach, where the
    // brain does other things; the comparison is about the water.
    for y in 1..=4 {
        for x in -31..=0 {
            for z in -31..=31 {
                if x == -31 || x == 0 || z == -31 || z == 31 {
                    r.set_block(x, y, z, "minecraft:stone");
                    h.set_block(x, y, z, "minecraft:stone");
                }
            }
        }
    }
    h.tick();
}

/// 20 axolotls swimming in a pool, brains alone (no goal, no imposed route: the brain of the `Mob` re-creates itself when it is taken away,
/// and its own walk targets then fight the route that `swim_controls_vs_mob` imposes): the walks they begin, the share of ticks with a walk
/// target, and the distance they cover in 400 ticks, on both sides.
#[test]
fn the_axolotl_swims_about_as_the_mob_does() {
    if skip() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    pool(&r, &mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let mut ids = Vec::new();
    for k in 0..20usize {
        let (x, z) = (-27.5 + 5.0 * (k % 5) as f64, -14.5 + 8.0 * (k / 5) as f64);
        let i = r.add_id(Vec3::new(x, 2.0, z), ((k as u32 + 1).wrapping_mul(0x9E37_79B1) >> 4) | 1);
        let e = spawn_species(&mut h.world, &sp, x, z, (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        h.teleport(e, Vec3::new(x, 2.0, z)); // Pos AND Bounds: the box of the water test must be at the spot
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
        ids.push((i, e));
    }
    let n = ids.len();
    if std::env::var_os("TRACE_SWIM").is_some() {
        let mw = h.world.resource::<Level>().mw.0;
        for y in [-5, -4, -1, 0, 1, 2, 4, 5] {
            eprintln!("ecs fluid at (-19,{y},-10): {:?} block {}", mw.fluid(-19, y, -10), mw.block_name(-19, y, -10));
        }
        for y in [-1, 0, 1, 2, 4, 5] {
            eprintln!("mob fluid at (-19,{y},-10): {:?} block {}", r.w.fluid(-19, y, -10), r.w.block_name(-19, y, -10));
        }
    }
    let has = |m: &horizon_client_lib::mobs::Mob, name: &str| animal_brain::describe(m).is_some_and(|(a, b)| format!("{a};{b}").contains(&format!("{name}=")));
    let (mut starts, mut ticks, mut dist) = ([0usize; 2], [0usize; 2], [0.0_f64; 2]);
    let mut was = vec![[false; 2]; n];
    // The ecs animal learns that it is in water at the end of its first tick, after its brain's first step (an animal born in water picks a
    // walk of the land and the sink holds it for 150 to 250 ticks): both sides settle for 300 ticks before the measure.
    for _ in 0..300 {
        r.step(None);
        h.tick();
    }
    let mut last: Vec<(Vec3, Vec3)> = ids.iter().map(|&(i, e)| (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0)).collect();
    for t in 1..=400 {
        r.step(None);
        h.tick();
        for (k, &(i, e)) in ids.iter().enumerate() {
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            if t > 0 {
                dist[0] += xz(a, last[k].0);
                dist[1] += xz(b, last[k].1);
                let on = [has(&r.mobs[i], "walk_target"), present(&h.world, e, &horizon_ecs_proto::brain::WALK_TARGET)];
                for s in 0..2 {
                    starts[s] += usize::from(on[s] && !was[k][s]);
                    ticks[s] += usize::from(on[s]);
                }
                was[k] = on;
            }
            last[k] = (a, b);
        }
        if std::env::var_os("TRACE_SWIM").is_some() && t % 10 == 0 && t <= 200 {
            let (i, e) = ids[0];
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            eprintln!("t{t} ECS needs_path {} following {} stroll {:?} nav {:?} vel {:?} fluid {:?} swim {} walk {}", h.world.get::<NeedsPath>(e).is_some(), h.world.get::<Following>(e).is_some(), h.world.get::<horizon_ecs_proto::cow::Stroll>(e).map(|s| (s.tx, s.ty, s.tz)), h.world.get::<horizon_ecs_proto::cow::Nav>(e).map(|n| (n.speed, n.reach)), h.world.get::<Vel>(e).map(|v| v.0), h.world.get::<FluidState>(e).map(|f| (f.in_water, f.water)), h.world.get::<horizon_ecs_proto::locomotion::SwimMode>(e).is_some(), h.world.get::<horizon_ecs_proto::locomotion::WalkMode>(e).is_some());
            eprintln!("t{t} MOB ({:.1},{:.1},{:.1}) {:?} | ECS ({:.1},{:.1},{:.1}) {:?}", a.x, a.y, a.z, animal_brain::describe(&r.mobs[i]).map(|d| (d.0, d.1.chars().filter(|_| true).take(110).collect::<String>())), b.x, b.y, b.z, horizon_ecs_proto::brain::describe(&h.world, e).map(|d| d.chars().take(200).collect::<String>()));
        }
    }
    eprintln!("walks begun (20 axolotls, 400 ticks after 300 of settling): mob {} ecs {}; ticks with a walk target: mob {} ecs {}; mean distance covered: mob {:.1} ecs {:.1}", starts[0], starts[1], ticks[0], ticks[1], dist[0] / n as f64, dist[1] / n as f64);
    let near = |a: f64, b: f64, tol: f64| (a - b).abs() <= tol * a.max(b);
    assert!(near(starts[0] as f64, starts[1] as f64, 0.25), "walks begun: mob {}, ecs {}", starts[0], starts[1]);
    assert!(near(ticks[0] as f64, ticks[1] as f64, 0.25), "ticks walking: mob {}, ecs {}", ticks[0], ticks[1]);
    assert!(near(dist[0], dist[1], 0.25), "distance: mob {:.1}, ecs {:.1}", dist[0], dist[1]);
}

/// One scene: `n` axolotls in water, each hit once by a player at tick 5 with `damage` (after `pre` damage that lowers its health, which
/// has no effect on the roll if not under half); how many play dead (the memory `play_dead_ticks`) 40 ticks later, how many of those have
/// Regeneration, and the tick the memory is gone, on both sides.
fn dead_scene(seed: u64, n: usize, damage: f32) -> ([usize; 2], [usize; 2], [Vec<i64>; 2]) {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    pool(&r, &mut h);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: 0, off: 0, ..Default::default() });
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let mut ids = Vec::new();
    for k in 0..n {
        let (x, z) = (-28.5 + 5.0 * (k % 6) as f64, -28.5 + 8.0 * (k / 6) as f64);
        let i = r.add_id(Vec3::new(x, 2.0, z), (((k as u32 + 1) * 7 + seed as u32 * 100_003).wrapping_mul(0x9E37_79B1) >> 4) | 1);
        let e = spawn_species(&mut h.world, &sp, x, z, (k as u64 + 1 + seed * 1000).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        h.teleport(e, Vec3::new(x, 2.0, z)); // Pos AND Bounds: the box of the water test must be at the spot
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
        ids.push((i, e));
    }
    let has = |m: &horizon_client_lib::mobs::Mob, name: &str| animal_brain::describe(m).is_some_and(|(a, b)| format!("{a};{b}").contains(&format!("{name}=")));
    let (mut dead, mut regen, mut gone) = ([0usize; 2], [0usize; 2], [vec![], vec![]]);
    let mut was = vec![[false; 2]; n];
    let view = player_view(Vec3::new(0.0, 1.0, 0.0), None);
    for t in 1..=260i64 {
        if t == 5 {
            for &(i, e) in &ids {
                r.hit(i, damage, [0.0, 1.0, 0.0]);
                h.hit_by(e, damage, 1);
            }
        }
        r.step(Some(&view));
        h.tick();
        for (k, &(i, e)) in ids.iter().enumerate() {
            let on = [has(&r.mobs[i], "play_dead_ticks"), present(&h.world, e, &horizon_ecs_proto::brain::behaviours::play_dead::PLAY_DEAD_TICKS)];
            if t == 45 {
                if on[0] && std::env::var_os("TRACE_DEAD").is_some() {
                    eprintln!("mob dead: {:?} effects {:?}", animal_brain::describe(&r.mobs[i]), r.mobs[i].has_effect("minecraft:regeneration"));
                }
                for s in 0..2 {
                    dead[s] += usize::from(on[s]);
                }
                regen[0] += usize::from(on[0] && r.mobs[i].has_effect("minecraft:regeneration"));
                regen[1] += usize::from(on[1] && horizon_ecs_proto::effects::has(&h.world, e, horizon_ecs_proto::effects::Kind::Regeneration));
            }
            for s in 0..2 {
                if was[k][s] && !on[s] {
                    if s == 1 && t < 100 && std::env::var_os("TRACE_DEAD").is_some() {
                        eprintln!("ecs early end t{t}: pos {:?} fluid {:?} vel {:?} brain {:?}", h.world.get::<Pos>(e).map(|p| p.0), h.world.get::<FluidState>(e).map(|f| (f.in_water, f.water)), h.world.get::<Vel>(e).map(|v| v.0), horizon_ecs_proto::brain::describe(&h.world, e));
                    }
                    gone[s].push(t);
                }
            }
            was[k] = on;
        }
    }
    (dead, regen, gone)
}

/// 300 axolotls hit for 2 (roll: 1/3 for the first draw, then `nextInt(3) < 2`: 2/9 = 22 %) on each side: how many play dead, with Regeneration,
/// and how long (the memory lasts 200 ticks after the hit).
#[test]
fn the_axolotl_plays_dead_as_often_as_the_mob_s() {
    if skip() {
        return;
    }
    let (mut dead, mut regen, mut gone) = ([0usize; 2], [0usize; 2], [vec![], vec![]]);
    for seed in 0..30 {
        let (d, g, e) = dead_scene(seed, 30, std::env::var("DAMAGE").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0));
        for s in 0..2 {
            dead[s] += d[s];
            regen[s] += g[s];
            gone[s].extend(e[s].iter());
        }
    }
    for s in 0..2 {
        let mut v = gone[s].clone();
        v.sort_unstable();
        eprintln!("{} ends sorted: {:?}", ["mob", "ecs"][s], v);
    }
    let mean_end = |v: &Vec<i64>| v.iter().sum::<i64>() as f64 / v.len().max(1) as f64;
    eprintln!("play dead (900 axolotls, damage 2, expected 22.2 %): mob {} ({:.1} %), ecs {} ({:.1} %); with regeneration: mob {} ecs {}; memory gone at: mob {:.1} ({}), ecs {:.1} ({})", dead[0], dead[0] as f64 / 9.0, dead[1], dead[1] as f64 / 9.0, regen[0], regen[1], mean_end(&gone[0]), gone[0].len(), mean_end(&gone[1]), gone[1].len());
    assert!(dead[0] > 30 && dead[1] > 30, "témoin : des axolotls font le mort : mob {} ecs {}", dead[0], dead[1]);
    assert!((dead[0] as f64 - dead[1] as f64).abs() <= 0.25 * dead[0] as f64, "mob {} ecs {}", dead[0], dead[1]);
    assert!(regen[0] as f64 >= 0.8 * dead[0] as f64 && regen[1] as f64 >= 0.8 * dead[1] as f64, "regeneration: mob {} of {}, ecs {} of {}", regen[0], dead[0], regen[1], dead[1]);
    assert!((mean_end(&gone[0]) - mean_end(&gone[1])).abs() < 6.0, "end: mob {:.1}, ecs {:.1}", mean_end(&gone[0]), mean_end(&gone[1]));
}

/// 6 scenes of 4 axolotls, each with a cod 6 blocks away in a pool (both sides alone: the cods swim as their own configuration says): the tick
/// each axolotl first holds an attack target and the tick its cod is dead, then the hunting cooldown that starts after (the axolotl does not
/// take the next prey for 2400 ticks). Means over the pairs that fought, on both sides.
#[test]
fn the_axolotl_hunts_a_cod_as_the_mob_does() {
    if skip() {
        return;
    }
    let mut rows: Vec<[Option<i64>; 4]> = Vec::new();
    let mut hurt_rows: Vec<[Option<i64>; 2]> = Vec::new();
    for seed in 0..6u64 {
        let mut r = MobRef::new(60, false);
        let mut h = Herd::with_slab_awake(60, true, false);
        pool(&r, &mut h);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let sp = *table.iter().find(|s| s.name == species().name).unwrap();
        let cod_sp = *table.iter().find(|s| s.name == "minecraft:cod").unwrap();
        let mut pairs = Vec::new();
        for k in 0..4usize {
            let (x, z) = (-27.5 + 7.0 * k as f64, -20.0 + 12.0 * seed as f64 % 50.0);
            let i = r.add_id(Vec3::new(x, 2.0, z), (((k as u32 + 1) * 11 + seed as u32 * 100_003).wrapping_mul(0x9E37_79B1) >> 4) | 1);
            let c = r.add_kind(Vec3::new(x + 4.0, 2.0, z + 4.0), mob_kind("minecraft:cod"));
            let e = spawn_species(&mut h.world, &sp, x, z, (k as u64 + 1 + seed * 1000).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let ce = spawn_species(&mut h.world, &cod_sp, x + 4.0, z + 4.0, (k as u64 + 77 + seed * 1000).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            for en in [e, ce] {
                h.world.get_mut::<Pos>(en).unwrap().0.y = 2.0;
                h.world.get_mut::<ActiveUntil>(en).unwrap().0 = u64::MAX;
                h.world.resource_mut::<Entered>().0.push(en);
            }
            pairs.push((i, c, e, ce));
        }
        let has = |m: &horizon_client_lib::mobs::Mob, name: &str| animal_brain::describe(m).is_some_and(|(a, b)| format!("{a};{b}").contains(&format!("{name}=")));
        let mut when = vec![[None::<i64>; 4]; pairs.len()];
        let mut hurt = vec![[None::<i64>; 2]; pairs.len()];
        for t in 1..=400i64 {
            r.step(None);
            h.tick();
            // The cods are held where they were put (each tick, on both sides): the species' own swimming of a cod is not what is measured.
            for (k, &(_, c, _, ce)) in pairs.iter().enumerate() {
                let home = Vec3::new(-27.5 + 7.0 * k as f64 + 4.0, 2.0, -20.0 + 12.0 * seed as f64 % 50.0 + 4.0);
                r.mobs[c].position = home;
                r.mobs[c].velocity = Vec3::ZERO;
                if h.world.get::<Pos>(ce).is_some() {
                    h.teleport(ce, home);
                    h.world.get_mut::<Vel>(ce).unwrap().0 = Vec3::ZERO;
                }
                if hurt[k][0].is_none() && r.mobs[c].health < r.mobs[c].attrs.max_health {
                    hurt[k][0] = Some(t);
                }
                if hurt[k][1].is_none() && h.world.get::<Health>(ce).is_none_or(|x| x.0 < 3.0) {
                    hurt[k][1] = Some(t);
                }
            }
            if std::env::var_os("TRACE_HUNT").is_some() && seed == 0 && t % 20 == 0 && t <= 200 {
                let (i, c, e, ce) = pairs[0];
                eprintln!("t{t} MOB ax ({:.1},{:.1},{:.1}) cod ({:.1},{:.1},{:.1}) h {} {:?} | ECS ax ({:.1},{:.1}) cod {:?} {:?} hp", r.mobs[i].position.x, r.mobs[i].position.y, r.mobs[i].position.z, r.mobs[c].position.x, r.mobs[c].position.y, r.mobs[c].position.z, r.mobs[c].health, animal_brain::describe(&r.mobs[i]).map(|d| d.0), h.world.get::<Pos>(e).unwrap().0.x, h.world.get::<Pos>(e).unwrap().0.z, h.world.get::<Pos>(ce).map(|p| (p.0.x as f32, p.0.z as f32)), h.world.get::<Health>(ce).map(|x| x.0));
            }
            for (k, &(i, c, e, ce)) in pairs.iter().enumerate() {
                if when[k][0].is_none() && has(&r.mobs[i], "attack_target") {
                    when[k][0] = Some(t);
                }
                if when[k][1].is_none() && present(&h.world, e, &horizon_ecs_proto::brain::ATTACK_TARGET) {
                    when[k][1] = Some(t);
                }
                if when[k][2].is_none() && (r.mobs[c].health <= 0.0 || r.mobs[c].removed) {
                    when[k][2] = Some(t);
                }
                if when[k][3].is_none() && h.world.get::<Health>(ce).is_none_or(|x| x.0 <= 0.0) {
                    when[k][3] = Some(t);
                }
            }
        }
        if std::env::var_os("TRACE_HUNT").is_some() {
            eprintln!("first hurt cod per pair (mob, ecs): {:?}", hurt);
        }
        hurt_rows.extend(hurt);
        rows.extend(when);
    }
    let col = |c: usize| mean(&rows.iter().map(|w| w[c]).collect::<Vec<_>>());
    let (ma, ea, md, ed) = (col(0), col(1), col(2), col(3));
    let hurt_tick = |c: usize| mean(&hurt_rows.iter().map(|w| w[c]).collect::<Vec<_>>());
    let (mh, eh) = (hurt_tick(0), hurt_tick(1));
    eprintln!("first bite (cod hurt): mob {:.1} ({}) ecs {:.1} ({})", mh.0, mh.1, eh.0, eh.1);
    eprintln!("24 axolotls with a cod: first target at mob {:.1} ({}) ecs {:.1} ({}); cod dead at mob {:.1} ({}) ecs {:.1} ({})", ma.0, ma.1, ea.0, ea.1, md.0, md.1, ed.0, ed.1);
    assert!(ma.1 > 12 && ea.1 > 12, "témoin : les axolotls attaquent : mob {} ecs {}", ma.1, ea.1);
}
