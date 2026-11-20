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

fn mob_run(n: usize, r: f64, ticks: i64) -> Run {
    let spots = ring(n, r);
    let mut rf = MobRef::new(60, false);
    for s in &spots {
        rf.add_kind(Vec3::new(s.0, 1.0, s.1), mob_kind("minecraft:hoglin"));
    }
    let mut out = Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![Vec::new(); n] };
    for t in 1..=ticks {
        let p = player_view(STILL(t), None);
        let from = rf.events.len();
        rf.step(Some(&p));
        for (i, m) in rf.mobs.iter().enumerate() {
            if m.ai.target.is_some() && out.acquired[i].is_none() {
                out.acquired[i] = Some(t);
            }
        }
        if std::env::var_os("DBG_HOGLIN").is_some() && [100, 250, 290, 330, 400].contains(&t) {
            let d: Vec<String> = rf.mobs.iter().map(|m| format!("{:.1}/{}", ((m.position.x - 0.5).powi(2) + (m.position.z - 0.5).powi(2)).sqrt(), m.ai.target.is_some())).collect();
            eprintln!("t={t} dist/target: {}", d.join(" "));
        }
        for e in &rf.events[from..] {
            if let MobEvent::AttackPlayer(_, f) = e {
                let pos: Vec<(f64, f64)> = rf.mobs.iter().map(|m| (m.position.x, m.position.z)).collect();
                let i = nearest(&pos, (f[0], f[2]));
                out.first_hit[i].get_or_insert(t);
                out.hits[i].push(t);
            }
        }
    }
    out
}

fn ecs_run(n: usize, r: f64, ticks: i64) -> Run {
    let spots = ring(n, r);
    let mut h = Herd::with_slab(60, true, false);
    h.world.resource_mut::<PlayerState>().immortal = true;
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
    let mut out = Run { acquired: vec![None; n], first_hit: vec![None; n], hits: vec![Vec::new(); n] };
    let mut seen = 0;
    for t in 1..=ticks {
        let p = STILL(t);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0, ..Default::default() });
        h.tick();
        for (i, &e) in ids.iter().enumerate() {
            if h.world.get::<Target>(e).is_some() && out.acquired[i].is_none() {
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
    let (m, e) = (mob_run(12, 12.0, 400), ecs_run(12, 12.0, 400));
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

/// `StopAttackingIfTargetInvalid` (jar: `CANT_REACH_WALK_TARGET_SINCE` older than 200 ticks): in the `Mob` all twelve hoglins have stopped striking by t=290
/// (the last strikes are at 158..285, four never struck) while they stand 0.9 to 3.2 blocks from the player with a target; the ECS keeps eight striking
/// after t=300. OPEN divergence (suivi/ecs-ecarts.md, 2026-11-20 hoglin): `Nav.cant_reach_since` and `melee::Params.forget_unreachable` exist, but the ECS
/// does not repath while adjacent, so its clock is rarely started; the `Mob`'s path attempts that fail next to the player are not reproduced.
#[test]
#[ignore = "open divergence: the unreachable clock of the Mob hoglins starts where the ECS makes no path (see ecs-ecarts.md)"]
fn hoglins_forget_an_unreachable_target_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let (m, e) = (mob_run(12, 12.0, 400), ecs_run(12, 12.0, 400));
    // `StopAttackingIfTargetInvalid`: a target no path reaches for 200 ticks is forgotten, so the hoglins that cannot reach the player stop
    // touching it. The count of those still striking after t=300, and the tick of the last strike of each, are compared.
    let late = |r: &Run| r.hits.iter().filter(|h| h.iter().any(|&t| t > 300)).count();
    let lasts = |r: &Run| -> Vec<i64> { let mut v: Vec<i64> = r.hits.iter().map(|h| h.last().copied().unwrap_or(0)).collect(); v.sort_unstable(); v };
    eprintln!("still striking after t=300: mob {} ecs {}; last strikes mob {:?} ecs {:?}", late(&m), late(&e), lasts(&m), lasts(&e));
    assert_eq!(late(&m), late(&e), "hoglins still striking after t=300 (the unreachable ones forget their target)");
}
