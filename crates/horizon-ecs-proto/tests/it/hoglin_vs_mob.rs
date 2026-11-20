//! The ECS hoglin against a real `Mob` hoglin in the same scenario (`HORIZON_TEST_RESOURCES` set): a ring of hoglins around a
//! still player. Acquisition, first strike and the gap between strikes are compared, as the zombie test does.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::MobEvent;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*, hostile::*, targeting::*};

const STILL: fn(i64) -> Vec3 = |_| Vec3::new(0.5, 1.0, 0.5);

struct Run {
    acquired: Vec<Option<i64>>,
    first_hit: Vec<Option<i64>>,
    hits: Vec<Vec<i64>>,
}

fn ring(n: usize, r: f64) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            (r * a.cos() + 0.5, r * a.sin() + 0.5)
        })
        .collect()
}

fn nearest(spots: &[(f64, f64)], from: (f64, f64)) -> usize {
    let d = |i: usize| (spots[i].0 - from.0).powi(2) + (spots[i].1 - from.1).powi(2);
    (0..spots.len()).min_by(|&a, &b| d(a).partial_cmp(&d(b)).unwrap()).unwrap()
}

fn mob_run(n: usize, r: f64, ticks: i64, seed: u32) -> Run {
    let spots = ring(n, r);
    let mut rf = MobRef::new(60, false);
    for s in &spots {
        rf.add_kind_id(Vec3::new(s.0, 1.0, s.1), mob_kind("minecraft:hoglin"), seed * 100 + rf.mobs.len() as u32 + 1);
    }
    let mut out = Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![Vec::new(); n] };
    for t in 1..=ticks {
        let p = player_view(STILL(t), None);
        let from = rf.events.len();
        // The strike is logged at the position the attacker had when it struck: before the tick moved it (the crowd is a few tenths wide).
        let before: Vec<(f64, f64)> = rf.mobs.iter().map(|m| (m.position.x, m.position.z)).collect();
        rf.step(Some(&p));
        for (i, m) in rf.mobs.iter().enumerate() {
            if m.ai.target.is_some() && out.acquired[i].is_none() {
                out.acquired[i] = Some(t);
            }
        }
        for e in &rf.events[from..] {
            if let MobEvent::AttackPlayer(_, f) = e {
                let i = nearest(&before, (f[0], f[2]));
                out.first_hit[i].get_or_insert(t);
                out.hits[i].push(t);
            }
        }
    }
    out
}

fn ecs_run(n: usize, r: f64, ticks: i64, seed: u64) -> Run {
    let spots = ring(n, r);
    let mut h = Herd::with_slab(60, true, false);
    h.world.resource_mut::<PlayerState>().immortal = true;
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin = table.iter().position(|s| s.name == "minecraft:hoglin").expect("the hoglin species is registered");
    let ids: Vec<Entity> = spots
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let e = spawn_species(&mut h.world, &table[hoglin], s.0, s.1, 1000 + seed * 100 + i as u64);
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
            h.world.resource_mut::<Entered>().0.push(e);
            e
        })
        .collect();
    let mut out = Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![Vec::new(); n] };
    let mut seen = 0;
    for t in 1..=ticks {
        let p = STILL(t);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0, ..Default::default() });
        let before: Vec<(f64, f64)> = ids.iter().map(|&e| { let q = h.world.get::<Pos>(e).unwrap().0; (q.x, q.z) }).collect();
        h.tick();
        for (i, &e) in ids.iter().enumerate() {
            if h.world.get::<Target>(e).is_some() && out.acquired[i].is_none() {
                out.acquired[i] = Some(t);
            }
        }
        let hits = h.world.resource::<PlayerState>().hits.clone();
        for hit in &hits[seen..] {
            let i = nearest(&before, hit.from);
            out.first_hit[i].get_or_insert(t);
            out.hits[i].push(t);
        }
        seen = hits.len();
    }
    out
}

fn mean_of(v: &[Option<i64>]) -> f64 {
    let k: Vec<f64> = v.iter().flatten().map(|&x| x as f64).collect();
    k.iter().sum::<f64>() / k.len().max(1) as f64
}

fn gap(r: &Run) -> (f64, i64) {
    let g: Vec<f64> = r.hits.iter().flat_map(|h| h.windows(2).map(|w| (w[1] - w[0]) as f64)).collect();
    (g.iter().sum::<f64>() / g.len().max(1) as f64, g.iter().map(|&x| x as i64).min().unwrap_or(0))
}

/// Hoglins around a still player: acquisition, first strike, cadence, in a ring of 12 blocks.
#[test]
fn hoglins_hunt_a_still_player_as_a_mob_does() {
    if !have_data() {
        return;
    }
    // The ring packs twelve wide animals around one player: which ones reach it and in which order depends on the draws (the ids and seeds
    // of the animals), so a single run is one sample of a chaotic statistic (the ecs gap goes from 41.0 to 50.2 over seven seeds; pooled, 44.0
    // against 43.0 for the Mob; each hoglin strikes every 41 ticks on both sides, the jar's 40 + 1). Both sides are pooled over the same seeds.
    let pool = |runs: Vec<Run>| {
        let mut all = Run { acquired: Vec::new(), first_hit: Vec::new(), hits: Vec::new() };
        for r in runs {
            all.acquired.extend(r.acquired);
            all.first_hit.extend(r.first_hit);
            all.hits.extend(r.hits);
        }
        all
    };
    let (m, e) = (pool((0..7u32).map(|k| mob_run(12, 12.0, 400, k)).collect()), pool((0..7u32).map(|k| ecs_run(12, 12.0, 400, u64::from(k))).collect()));
    eprintln!(
        "ring 12 hoglin: acquired mob {:.1} ecs {:.1}; first hit mob {:.1} ecs {:.1}; gap (mean, min) mob {:?} ecs {:?}",
        mean_of(&m.acquired),
        mean_of(&e.acquired),
        mean_of(&m.first_hit),
        mean_of(&e.first_hit),
        gap(&m),
        gap(&e),
    );
    let (ma, ea) = (mean_of(&m.acquired), mean_of(&e.acquired));
    assert!((ma - ea).abs() < 8.0, "acquisition: mob {ma}, ecs {ea}");
    let (mh, eh) = (mean_of(&m.first_hit), mean_of(&e.first_hit));
    assert!((mh - eh).abs() < 25.0, "first hit: mob {mh}, ecs {eh}");
    let ((mg, _), (eg, _)) = (gap(&m), gap(&e));
    assert!((mg - eg).abs() < 4.0, "gap between strikes: mob {mg}, ecs {eg}");
}

/// `Hoglin.aiStep` (jar: `timeInOverworld` over 300): outside the Nether the hoglins become zoglins. In the `Mob` the twelve strikers stop at t=300
/// because they convert (`MobEvent::Convert` at t=301), not because they forget an unreachable target (the `Mob` has no such memory: its goal ends
/// on the target alone). The tick of every conversion is compared, equal to the tick.
#[test]
fn hoglins_convert_after_300_ticks_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let spots = ring(12, 12.0);
    let mut rf = MobRef::new(60, false);
    for s in &spots {
        rf.add_kind(Vec3::new(s.0, 1.0, s.1), mob_kind("minecraft:hoglin"));
    }
    let mut mob_ticks = Vec::new();
    for t in 1..=400 {
        let p = player_view(STILL(t), None);
        let from = rf.events.len();
        rf.step(Some(&p));
        for e in &rf.events[from..] {
            if matches!(e, MobEvent::Convert { .. }) {
                mob_ticks.push(t);
            }
        }
    }
    let mut h = Herd::with_slab(60, true, false);
    h.world.insert_resource(UnsafePlace(|_, _| true));
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin = table.iter().position(|s| s.name == "minecraft:hoglin").expect("the hoglin species is registered");
    let ids: Vec<Entity> = spots
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let e = spawn_species(&mut h.world, &table[hoglin], s.0, s.1, 1000 + i as u64);
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
            h.world.resource_mut::<Entered>().0.push(e);
            e
        })
        .collect();
    let mut ecs_ticks = Vec::new();
    let mut gone = vec![false; ids.len()];
    for t in 1..=400 {
        let p = STILL(t);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0, ..Default::default() });
        h.tick();
        for (i, &e) in ids.iter().enumerate() {
            if !gone[i] && h.world.get_entity(e).is_err() {
                gone[i] = true;
                ecs_ticks.push(t);
            }
        }
    }
    eprintln!("conversion ticks: mob {mob_ticks:?} ecs {ecs_ticks:?}");
    assert_eq!(mob_ticks, ecs_ticks, "the tick of every hoglin conversion");
}

/// `HoglinAi.wasHurtBy`: a baby retreats from the attacker (`AVOID_TARGET`, flees away), an adult does not retreat but retaliates. A player
/// at x=-3 hits a hoglin at x=0.5: both babies flee east, the draws (level random for the Mob, own generator here) only set the duration.
#[test]
fn a_hurt_baby_hoglin_flees_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:hoglin"));
    horizon_client_lib::mobs::finalize::set_age(&mut r.mobs[i], -24000, None);
    let mut h = Herd::with_slab(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let hoglin = table.iter().position(|s| s.name == "minecraft:hoglin").unwrap();
    let e = spawn_species(&mut h.world, &table[hoglin], 0.5, 0.5, 1000);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    horizon_ecs_proto::behaviours::follow_parent::make_baby(&mut h.world, e, 24000);
    let at = Vec3::new(-3.0, 1.0, 0.5);
    h.set_online(&[(-3.0, 0.5)]);
    let view = player_view(at, None);
    for _ in 0..3 {
        r.step(Some(&view));
        h.tick();
    }
    r.hit(i, 1.0, [-3.0, 1.0, 0.5]);
    h.hit_by(e, 1.0, 1);
    let (mut mob_peak, mut ecs_peak) = (0.5f64, 0.5f64);
    let (mut mob_avoid, mut ecs_avoid) = (false, false);
    for _ in 0..300 {
        r.step(Some(&view));
        h.tick();
        mob_peak = mob_peak.max(r.mobs[i].position.x);
        ecs_peak = ecs_peak.max(h.world.get::<Pos>(e).map_or(ecs_peak, |p| p.0.x));
        mob_avoid |= r.mobs[i].extra.piglin.avoid.is_some();
        ecs_avoid |= horizon_ecs_proto::species::hoglin::retreating(&h.world, e);
    }
    eprintln!("baby flee: mob peak {mob_peak} avoid {mob_avoid}; ecs peak {ecs_peak} avoid {ecs_avoid}");
    assert!(mob_avoid && ecs_avoid, "both keep away from the attacker (mob {mob_avoid}, ecs {ecs_avoid})");
    assert!(mob_peak > 3.0 && ecs_peak > 3.0, "both flee east, away from x=-3 (mob {mob_peak}, ecs {ecs_peak})");
}

fn ecs_of(h: &mut Herd, name: &str, x: f64, z: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// `HoglinAi.onHitTarget` and `broadcastRetreat`: an adult hoglin that strikes a piglin while the adult piglins it sees outnumber the adult
/// hoglins by more than one (4 against 1 here) retreats from it and tells the adult hoglins it sees to. `HoglinAi.wasHurtBy` and
/// `broadcastAttackTarget`: the hit hoglin retaliates and the adult hoglins it sees take the attacker as their target.
#[test]
fn hoglins_retreat_from_piglins_that_outnumber_them_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let hog = [(0.5, 0.5), (0.5, 3.5)];
    let pig = [(3.5, 0.5), (3.5, 3.0), (3.5, -2.0), (6.0, 0.5)];
    let mi: Vec<usize> = hog.iter().map(|p| r.add_kind(Vec3::new(p.0, 1.0, p.1), mob_kind("minecraft:hoglin"))).collect();
    let mp: Vec<usize> = pig.iter().map(|p| r.add_kind(Vec3::new(p.0, 1.0, p.1), mob_kind("minecraft:piglin"))).collect();
    let ei: Vec<Entity> = hog.iter().enumerate().map(|(k, p)| ecs_of(&mut h, "minecraft:hoglin", p.0, p.1, 100 + k as u64)).collect();
    let ep: Vec<Entity> = pig.iter().enumerate().map(|(k, p)| ecs_of(&mut h, "minecraft:piglin", p.0, p.1, 200 + k as u64)).collect();
    let (mut mob_avoid, mut ecs_avoid) = ([None; 2], [None; 2]);
    let (mut mob_tgt, mut ecs_tgt) = ([None; 2], [None; 2]);
    for t in 1..=200i64 {
        if t == 25 {
            r.hit_by_mob(mi[0], 1.0, mp[0]);
            h.world.resource_mut::<DamageQueue>().add_by_entity(ei[0], 1.0, (3.5, 0.5), ep[0]);
        }
        let from = r.events.len();
        r.step(None);
        h.tick();
        // The game's dispatch of the two broadcasts, which the harness does not run.
        for ev in r.events[from..].to_vec() {
            match ev {
                MobEvent::Retreat { mob, from, at } => {
                    if let Some(k) = r.mobs.iter().position(|m| m.id == mob) {
                        horizon_client_lib::mobs::piglin::receive_retreat(&mut r.mobs[k], from, at);
                    }
                }
                MobEvent::HoglinAttackTarget { mob, target, .. } => {
                    if let Some(k) = r.mobs.iter().position(|m| m.id == mob) {
                        let pos: Vec<_> = r.mobs.iter().map(|m| (m.id, m.position)).collect();
                        let at = |w: horizon_client_lib::mobs::Who| match w {
                            horizon_client_lib::mobs::Who::Mob(id) => pos.iter().find(|p| p.0 == id).map(|p| p.1),
                            _ => None,
                        };
                        if let Some(t) = horizon_client_lib::mobs::piglin::receive_attack_target(&r.mobs[k], target, |w, _| at(w)) {
                            r.mobs[k].ai.target = Some(t);
                        }
                    }
                }
                _ => {}
            }
        }
        for k in 0..2 {
            if mob_avoid[k].is_none() && r.mobs[mi[k]].extra.piglin.avoid.is_some() {
                mob_avoid[k] = Some(t);
            }
            if ecs_avoid[k].is_none() && horizon_ecs_proto::species::hoglin::retreating(&h.world, ei[k]) {
                ecs_avoid[k] = Some(t);
            }
            if mob_tgt[k].is_none() && r.mobs[mi[k]].ai.target.is_some() {
                mob_tgt[k] = Some(t);
            }
            if ecs_tgt[k].is_none() && h.world.get::<Target>(ei[k]).is_some() {
                ecs_tgt[k] = Some(t);
            }
        }
    }
    eprintln!("outnumbered: avoid mob {mob_avoid:?} ecs {ecs_avoid:?}; target mob {mob_tgt:?} ecs {ecs_tgt:?}");
    let _ = ep;
    assert!(mob_avoid.iter().all(Option::is_some), "the Mob's two hoglins retreat ({mob_avoid:?})");
    assert!(ecs_avoid.iter().all(Option::is_some), "the ecs's two hoglins retreat ({ecs_avoid:?})");
    assert!(mob_tgt[1].is_some() == ecs_tgt[1].is_some(), "the visible adult hoglin takes the target as the Mob's does (mob {mob_tgt:?}, ecs {ecs_tgt:?})");
}

/// `BecomePassiveIfMemoryPresent` in the fight activity: a hoglin that already hunts a player drops the target and is pacified for 200 ticks
/// when a repellent block (a warped fungus) comes within reach, at the next scan of the sensor (every 20 ticks).
#[test]
fn a_repellent_pacifies_a_hoglin_that_is_already_attacking_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let i = r.add_kind(Vec3::new(8.5, 1.0, 0.5), mob_kind("minecraft:hoglin"));
    let e = ecs_of(&mut h, "minecraft:hoglin", 8.5, 0.5, 5);
    h.set_online(&[(0.5, 0.5)]);
    let view = player_view(Vec3::new(0.5, 1.0, 0.5), None);
    let (mut mob_drop, mut ecs_drop) = (None, None);
    let (mut mob_had, mut ecs_had) = (false, false);
    for t in 1..=120i64 {
        if t == 30 {
            r.set_block(12, 1, 0, "minecraft:warped_fungus");
            r.apply_block_events();
            h.set_block(12, 1, 0, "minecraft:warped_fungus");
        }
        r.step(Some(&view));
        h.tick();
        mob_had |= r.mobs[i].ai.target.is_some();
        ecs_had |= h.world.get::<Target>(e).is_some();
        if t > 30 && mob_had && mob_drop.is_none() && r.mobs[i].ai.target.is_none() {
            mob_drop = Some(t);
        }
        if t > 30 && ecs_had && ecs_drop.is_none() && h.world.get::<Target>(e).is_none() {
            ecs_drop = Some(t);
        }
    }
    eprintln!("pacify while attacking: had target mob {mob_had} ecs {ecs_had}; dropped mob {mob_drop:?} ecs {ecs_drop:?} pacified ecs {}", horizon_ecs_proto::species::hoglin::pacified(&h.world, e));
    assert!(mob_had && ecs_had, "both hunt the player first (mob {mob_had}, ecs {ecs_had})");
    let (m, c) = (mob_drop.expect("the Mob drops its target"), ecs_drop.expect("the ecs drops its target"));
    assert!((m - c).abs() <= 21, "the target is dropped within a scan period of each other (mob {m}, ecs {c})");
}
