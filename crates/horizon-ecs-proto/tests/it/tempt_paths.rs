//! One cow tempted by one player who walks a fixed line then turns (200 ticks), on both sides: the ticks where a path
//! is asked and where the `Finder` really runs, with the player's block, the path's target, whether it is done, the
//! distance and the nodes left.
use crate::common;
use common::*;
use horizon_client_lib::mobs::nav::FINDER_CALLS;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*};
use std::sync::atomic::Ordering::Relaxed;

#[derive(Debug, Clone, Default)]
struct Row {
    t: i64,
    finder: u64,
    asked: u64,
    player: (i32, i32),
    target: Option<[i32; 3]>,
    done: bool,
    left: usize,
    dist: f64,
}
fn player_at(t: i64) -> (f64, f64) {
    if t <= 100 { (-2.0 + 0.15 * t as f64, 0.5) } else { (13.0, 0.5 + 0.15 * (t - 100) as f64) }
}

#[test]
#[ignore = "trace"]
fn trace_tempt_paths() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let start = Vec3::new(-8.5, 1.0, 0.5);
    let i = r.add(start);
    let e = ecs_cow(&mut h, start.x, start.y, start.z, 5);
    h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[i].id);
    h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
    h.tick();
    let (item, name) = tempt_item();
    let mut p = player_view(Vec3::new(-2.0, 1.0, 0.5), Some(name));
    let (mut mob, mut ecs): (Vec<Row>, Vec<Row>) = (Vec::new(), Vec::new());
    let (mut f0, mut a0, mut s0) = (FINDER_CALLS.load(Relaxed), ASKS[2].load(Relaxed), FINDS.load(Relaxed));
    for t in 1..=200_i64 {
        let (x, z) = player_at(t);
        p = player_view(Vec3::new(x, 1.0, z), Some(name));
        h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z, main: item, off: 0, ..Default::default() });
        r.step(Some(&p));
        h.tick();
        let (f1, a1, s1) = (FINDER_CALLS.load(Relaxed), ASKS[2].load(Relaxed), FINDS.load(Relaxed));
        let m = &r.mobs[i];
        mob.push(Row {
            t,
            finder: f1 - f0,
            asked: 0,
            player: (x.floor() as i32, z.floor() as i32),
            target: m.ai.nav.path.as_ref().map(|p| p.target),
            done: m.ai.nav.path.as_ref().is_none_or(|p| p.is_done()),
            left: m.ai.nav.path.as_ref().map_or(0, |p| p.nodes.len() - p.next.min(p.nodes.len())),
            dist: xz(m.position, Vec3::new(x, 1.0, z)),
        });
        let nav = h.world.get::<Nav>(e).unwrap();
        ecs.push(Row {
            t,
            finder: s1 - s0,
            asked: a1 - a0,
            player: (x.floor() as i32, z.floor() as i32),
            target: nav.path.as_ref().map(|p| p.target),
            done: nav.path.as_ref().is_none_or(|p| p.is_done()),
            left: nav.path.as_ref().map_or(0, |p| p.nodes.len() - p.next.min(p.nodes.len())),
            dist: xz(h.world.get::<Pos>(e).unwrap().0, Vec3::new(x, 1.0, z)),
        });
        (f0, a0, s0) = (f1, a1, s1);
    }
    let _ = p;
    let (mf, ef) = (mob.iter().filter(|r| r.finder > 0).count(), ecs.iter().filter(|r| r.finder > 0).count());
    eprintln!("ticks with a search in 200: mob {mf}, ecs {ef}");
    for k in 0..200 {
        if mob[k].finder > 0 || ecs[k].finder > 0 || ecs[k].asked > 0 {
            eprintln!("t={:3} MOB finder {} player {:?} target {:?} done {} left {} dist {:.2} | ECS asked {} finder {} target {:?} done {} left {} dist {:.2}", mob[k].t, mob[k].finder, mob[k].player, mob[k].target, mob[k].done, mob[k].left, mob[k].dist, ecs[k].asked, ecs[k].finder, ecs[k].target, ecs[k].done, ecs[k].left, ecs[k].dist);
        }
    }
}

/// 40 cows around a player who stands still: the searches per cow, both sides, and where the cows end up.
#[test]
#[ignore = "trace"]
fn trace_tempt_crowd() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let (item, name) = tempt_item();
    let mut cows = Vec::new();
    for k in 0..40 {
        let a = f64::from(k) * 0.157 * 2.0;
        let rad = 5.0 + f64::from(k % 4) * 1.2;
        let (x, z) = (rad * a.cos(), rad * a.sin());
        let (i, e) = (r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 600 + u64::from(k as u32)));
        h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[i].id);
        h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
        cows.push((i, e));
    }
    h.tick();
    let p = player_view(Vec3::new(0.0, 1.0, 0.0), Some(name));
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: item, off: 0, ..Default::default() });
    let (f0, s0) = (FINDER_CALLS.load(Relaxed), FINDS.load(Relaxed));
    let mut per_tick = (Vec::new(), Vec::new());
    for t in 1..=300 {
        let (fa, sa) = (FINDER_CALLS.load(Relaxed), FINDS.load(Relaxed));
        // Per cow: the searches of this tick (the mob's by the counter before and after each cow's own step is not
        // available: the ecs's `asked` is by cow through its Nav, the mob's by the time of its last path).
        r.step(Some(&p));
        h.tick();
        per_tick.0.push(FINDER_CALLS.load(Relaxed) - fa);
        per_tick.1.push(FINDS.load(Relaxed) - sa);
        if t == 60 || t == 300 {
            let md: Vec<f64> = cows.iter().map(|&(i, _)| xz(r.mobs[i].position, Vec3::new(0.0, 1.0, 0.0))).collect();
            let ed: Vec<f64> = cows.iter().map(|&(_, e)| xz(h.world.get::<Pos>(e).unwrap().0, Vec3::new(0.0, 1.0, 0.0))).collect();
            let within = |v: &[f64]| v.iter().filter(|&&d| d < 2.5).count();
            eprintln!("t={t}: mean distance mob {:.2} ecs {:.2}; within 2.5: mob {} ecs {}", mean(&md), mean(&ed), within(&md), within(&ed));
        }
    }
    let sum = |v: &[u64], a: usize, b: usize| v[a..b].iter().sum::<u64>();
    eprintln!("searches: ticks 1-60 mob {} ecs {}; 61-180 mob {} ecs {}; 181-300 mob {} ecs {} (whole run: mob {}, ecs {})", sum(&per_tick.0, 0, 60), sum(&per_tick.1, 0, 60), sum(&per_tick.0, 60, 180), sum(&per_tick.1, 60, 180), sum(&per_tick.0, 180, 300), sum(&per_tick.1, 180, 300), FINDER_CALLS.load(Relaxed) - f0, FINDS.load(Relaxed) - s0);
}

/// The same crowd: the searches of each cow in ticks 61-300, sorted, with its final distance, both sides.
#[test]
#[ignore = "trace"]
fn trace_tempt_crowd_by_cow() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let (item, name) = tempt_item();
    let mut cows = Vec::new();
    for k in 0..40 {
        let a = f64::from(k) * 0.157 * 2.0;
        let rad = 5.0 + f64::from(k % 4) * 1.2;
        let (x, z) = (rad * a.cos(), rad * a.sin());
        let (i, e) = (r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 600 + u64::from(k as u32)));
        h.world.get_mut::<Phase>(e).unwrap().0 = u64::from(r.mobs[i].id);
        h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
        cows.push((i, e));
    }
    h.tick();
    let p = player_view(Vec3::new(0.0, 1.0, 0.0), Some(name));
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: item, off: 0, ..Default::default() });
    let mut at60 = (Vec::new(), Vec::new());
    for t in 1..=300 {
        r.step(Some(&p));
        h.tick();
        if t == 60 {
            at60 = (r.searches.clone(), cows.iter().map(|&(_, e)| h.world.get::<Nav>(e).unwrap().searches).collect());
        }
    }
    let mut rows: Vec<(u64, u32, f64, f64)> = cows
        .iter()
        .enumerate()
        .map(|(k, &(i, e))| (r.searches[i] - at60.0[i], h.world.get::<Nav>(e).unwrap().searches - at60.1[k], xz(r.mobs[i].position, Vec3::new(0.0, 1.0, 0.0)), xz(h.world.get::<Pos>(e).unwrap().0, Vec3::new(0.0, 1.0, 0.0))))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    let tempting_mob = cows.iter().filter(|&&(i, _)| r.names(i).contains(&"Tempt")).count();
    let tempting_ecs = cows.iter().filter(|&&(_, e)| h.world.get::<Tempting>(e).is_some()).count();
    eprintln!("cows tempting at the end: mob {tempting_mob}, ecs {tempting_ecs}");
    for &(i, e) in cows.iter().filter(|&&(i, _)| xz(r.mobs[i].position, Vec3::new(0.0, 1.0, 0.0)) > 2.7).take(6) {
        let m = &r.mobs[i];
        eprintln!("mob cow at {:.2}: goals {:?}, path {:?}, speed {}", xz(m.position, Vec3::new(0.0, 1.0, 0.0)), r.names(i), m.ai.nav.path.as_ref().map(|p| (p.nodes.len(), p.next, p.is_done())), m.ai.nav.speed_modifier);
        let _ = e;
    }
    for (m, e, md, ed) in rows.iter().take(12) {
        eprintln!("searches in ticks 61-300: mob {m:3} (distance {md:.2}) | ecs {e:3} (distance {ed:.2})");
    }
}

/// The ecs cow of the crowd that searches most: its ticks 100-170 one by one.
#[test]
#[ignore = "trace"]
fn trace_tempt_crowd_worst_cow() {
    if !have_data() {
        return;
    }
    let (_r, mut h) = flat_pair();
    let (item, _) = tempt_item();
    let mut cows = Vec::new();
    for k in 0..40 {
        let a = f64::from(k) * 0.157 * 2.0;
        let rad = 5.0 + f64::from(k % 4) * 1.2;
        let (x, z) = (rad * a.cos(), rad * a.sin());
        cows.push(ecs_cow(&mut h, x, 1.0, z, 600 + u64::from(k as u32)));
    }
    h.tick();
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: item, off: 0, ..Default::default() });
    let mut log: Vec<Vec<(u32, f64, bool, usize, bool, bool, bool)>> = vec![Vec::new(); cows.len()];
    for _ in 1..=300 {
        h.tick();
        for (k, &e) in cows.iter().enumerate() {
            let n = h.world.get::<Nav>(e).unwrap();
            log[k].push((n.searches, xz(h.world.get::<Pos>(e).unwrap().0, Vec3::new(0.0, 1.0, 0.0)), h.world.get::<Following>(e).is_some(), n.path.as_ref().map_or(0, |p| p.nodes.len() - p.next.min(p.nodes.len())), n.path.as_ref().is_none_or(|p| p.is_done()), h.world.get::<Tempting>(e).is_some(), h.world.get::<Moving>(e).is_some()));
        }
    }
    let worst = (0..cows.len()).max_by_key(|&k| log[k][299].0 - log[k][60].0).unwrap();
    eprintln!("worst cow {worst}: {} searches in ticks 61-300", log[worst][299].0 - log[worst][60].0);
    for t in 100..170 {
        let (a, b) = (log[worst][t - 1], log[worst][t]);
        eprintln!("t={} searches+{} dist {:.2} following {} nodes left {} path done {} tempting {} moving {}", t + 1, b.0 - a.0, b.1, b.2, b.3, b.4, b.5, b.6);
    }
}

/// The same cow (index 31) of the same crowd, on the mob side: ticks 100-170.
#[test]
#[ignore = "trace"]
fn trace_tempt_crowd_same_cow_mob() {
    if !have_data() {
        return;
    }
    let (mut r, _h) = flat_pair();
    let (_, name) = tempt_item();
    let mut mobs = Vec::new();
    for k in 0..40 {
        let a = f64::from(k) * 0.157 * 2.0;
        let rad = 5.0 + f64::from(k % 4) * 1.2;
        mobs.push(r.add(Vec3::new(rad * a.cos(), 1.0, rad * a.sin())));
    }
    let p = player_view(Vec3::new(0.0, 1.0, 0.0), Some(name));
    let k = 31;
    let mut last = 0;
    for t in 1..=300 {
        r.step(Some(&p));
        if (100..170).contains(&t) {
            let m = &r.mobs[mobs[k]];
            eprintln!("t={t} searches+{} dist {:.2} goals {:?} path {:?}", r.searches[mobs[k]] - last, xz(m.position, Vec3::new(0.0, 1.0, 0.0)), r.names(mobs[k]), m.ai.nav.path.as_ref().map(|p| (p.nodes.len() - p.next.min(p.nodes.len()), p.is_done())));
        }
        last = r.searches[mobs[k]];
    }
}
