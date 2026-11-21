//! Crowds against a real `Mob` herd in the same scene: cramming (`maxEntityCramming` 24) and the way a tight group
//! spreads out. The species is the one of the process (`HORIZON_SPECIES`).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;

fn pit(put: &mut dyn FnMut(i32, i32, i32)) {
    for x in 9..=11 {
        for z in 9..=11 {
            if (x, z) != (10, 10) {
                for y in 1..=3 {
                    put(x, y, z);
                }
            }
        }
    }
}
/// `n` animals in a one-block pit: how many are alive at the ticks 20, 40, 80 and 160, on both sides.
fn pit_run(n: usize) -> (Vec<usize>, Vec<usize>) {
    let marks = [20, 40, 80, 160];
    let mut r = MobRef::new(20, false);
    pit(&mut |x, y, z| r.set_block(x, y, z, "minecraft:stone"));
    let jitter = |i: usize| (0.01 * (i % 5) as f64, 0.01 * (i / 5 % 5) as f64);
    for i in 0..n {
        let (jx, jz) = jitter(i);
        r.add(Vec3::new(10.5 + jx - 0.02, 1.0, 10.5 + jz - 0.02));
    }
    let mut h = Herd::with_slab_awake(20, true, false);
    if std::env::var_os("ORDERED_PUSH").is_some() {
        h.set_ordered_push(true);
    }
    pit(&mut |x, y, z| h.set_block(x, y, z, "minecraft:stone"));
    h.tick();
    let ids: Vec<Entity> = (0..n).map(|i| {
        let (jx, jz) = jitter(i);
        let e = h.world.spawn(cow_bundle(10.5 + jx - 0.02, 10.5 + jz - 0.02, 7 + i as u64 * 0x9E37_79B9)).id();
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        e
    }).collect();
    let (mut mob, mut ecs) = (vec![], vec![]);
    for t in 1..=160 {
        r.step(None);
        h.tick();
        if std::env::var_os("CDEBUG").is_some() && n == 25 && t <= 60 && t % 3 == 0 {
            let ex: Vec<f64> = ids.iter().filter_map(|&e| h.world.get::<Pos>(e).map(|p| p.0.x)).collect();
            let mx: Vec<f64> = r.mobs.iter().filter(|m| mob_health(m) > 0.0).map(|m| m.position.x).collect();
            let rng = |v: &Vec<f64>| (v.iter().cloned().fold(f64::MAX, f64::min), v.iter().cloned().fold(f64::MIN, f64::max));
            eprintln!("t={t} alive mob {} ecs {} x-range mob {:.2?} ecs {:.2?}", mx.len(), ex.len(), rng(&mx), rng(&ex));
        }
        if marks.contains(&t) {
            mob.push(r.mobs.iter().filter(|m| mob_health(m) > 0.0).count());
            ecs.push(ids.iter().filter(|&&e| h.world.get::<Health>(e).is_some()).count());
        }
    }
    (mob, ecs)
}

/// More than 23 animals in a block are crushed, one tick in four, 6 a hit, with the 10-tick hurt window; 10 are not.
#[test]
fn a_crowd_in_a_pit_is_crushed_as_a_mob_s_is() {
    if horizon_ecs_proto::cow::skip_unless_farm_goals("a_crowd_in_a_pit_is_crushed_as_a_mob_s_is") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_small_body("a_crowd_in_a_pit_is_crushed_as_a_mob_s_is") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("a_crowd_in_a_pit_is_crushed_as_a_mob_s_is") {
        return;
    }
    if !have_data() {
        return;
    }
    for n in [10, 25, 40] {
        let (m, e) = pit_run(n);
        eprintln!("{n} in the pit, alive at ticks 20 / 40 / 80 / 160: mob {m:?} ecs {e:?}");
        if n == 10 {
            assert_eq!((m.last(), e.last()), (Some(&10), Some(&10)));
        } else {
            assert!(m[0] <= n && e[0] <= n);
            // The number alive falls on both sides to the point where fewer than 24 overlap the others.
            assert!(m[3] < n && e[3] < n, "crushed: mob {m:?}, ecs {e:?}");
            assert!((m[3] as i64 - e[3] as i64).abs() <= 3, "mob {m:?}, ecs {e:?}");
        }
    }
}

/// Two animals overlapping in the open: their positions tick by tick on both sides (the push law and its timing).
#[test]
fn two_overlapping_animals_move_apart_as_a_mob_s_do() {
    if horizon_ecs_proto::cow::skip_unless_small_body("two_overlapping_animals_move_apart_as_a_mob_s_do") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("two_overlapping_animals_move_apart_as_a_mob_s_do") {
        return;
    }
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(20, false);
    let mut h = Herd::with_slab_awake(20, true, false);
    h.tick();
    let w = f64::from(species().width);
    let three = std::env::var_os("THREE").is_some();
    let starts: Vec<(f64, f64)> = [(0.5, 0.5), (0.5 + 0.6 * w, 0.5), (0.5 + 0.3 * w, 0.5 + 0.5 * w)].into_iter().take(if three { 3 } else { 2 }).collect();
    let mut ids = vec![];
    for (i, s) in starts.iter().enumerate() {
        r.add(Vec3::new(s.0, 1.0, s.1));
        let e = h.world.spawn(cow_bundle(s.0, s.1, 3 + i as u64)).id();
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
        ids.push(e);
    }
    // The animals are in the world: the first pass of the ordered mode looks at all of them.
    if std::env::var_os("ORDERED_PUSH").is_some() {
        h.set_ordered_push(true);
    }
    let mut worst: f64 = 0.0;
    for t in 1..=60 {
        r.step(None);
        h.tick();
        let m: Vec<(f64, f64)> = r.mobs.iter().map(|m| (m.position.x, m.position.z)).collect();
        let e: Vec<(f64, f64)> = ids.iter().map(|&e| { let p = h.world.get::<Pos>(e).unwrap().0; (p.x, p.z) }).collect();
        if t <= 6 || t % 10 == 0 {
            eprintln!("t={t:2} mob {:.3?} | ecs {:.3?}", m.iter().map(|p| (p.0 * 1000.0).round() / 1000.0).collect::<Vec<_>>(), e.iter().map(|p| (p.0 * 1000.0).round() / 1000.0).collect::<Vec<_>>());
        }
        for (a, b) in m.iter().zip(&e) {
            worst = worst.max((a.0 - b.0).abs()).max((a.1 - b.1).abs());
        }
    }
    eprintln!("largest position difference over 60 ticks: {worst:.4}");
    // The `Mob` pass applies a push to an animal ticked later in the same tick (one impulse more before the pair is apart):
    // the final separation is 5 % wider on the `Mob` side (order of execution, category B). Without the two impulses per
    // tick of `Entity.push` called from both sides, the ecs would stop at half the speed.
    assert!(worst < if three { 0.2 } else { 0.1 }, "the push law differs: {worst}");
}

fn overlapping_pairs(p: &[(f64, f64)], w: f64) -> usize {
    let mut n = 0;
    for i in 0..p.len() {
        for j in i + 1..p.len() {
            if (p[i].0 - p[j].0).abs() < w && (p[i].1 - p[j].1).abs() < w {
                n += 1;
            }
        }
    }
    n
}
/// A tight herd (400 animals, 0.8 apart, a little noise) spreads out like a `Mob` herd: the pairs that still overlap after
/// 60 and 120 ticks, in the batch mode and in the ordered mode (which follows the `Mob`'s pass order).
#[test]
fn a_tight_herd_spreads_out_like_a_mob_herd() {
    if horizon_ecs_proto::cow::skip_unless_small_body("a_tight_herd_spreads_out_like_a_mob_herd") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("a_tight_herd_spreads_out_like_a_mob_herd") {
        return;
    }
    if !have_data() {
        return;
    }
    let n = 400;
    let side = 20;
    let mut r = MobRef::new(20, false);
    // A pitch of 0.89 body widths (0.8 for a cow): every animal overlaps its neighbours, whatever the species.
    let pitch = 0.89 * f64::from(species().width);
    let at = |i: usize| {
        let jit = |k: u64| (((i as u64 * 2654435761 + k * 40503) % 1000) as f64 / 1000.0 - 0.5) * 0.2;
        (-8.0 + (i % side) as f64 * pitch + 0.5 + jit(1) * pitch / 0.8, -8.0 + (i / side) as f64 * pitch + 0.5 + jit(2) * pitch / 0.8)
    };
    for i in 0..n {
        let p = at(i);
        r.add(Vec3::new(p.0, 1.0, p.1));
    }
    let w = f64::from(species().width);
    let mut mob = vec![];
    for t in 1..=120 {
        r.step(None);
        if t == 60 || t == 120 {
            mob.push(overlapping_pairs(&r.mobs.iter().map(|m| (m.position.x, m.position.z)).collect::<Vec<_>>(), w));
        }
    }
    let mut results = vec![];
    for ordered in [false, true] {
        let mut h = Herd::with_slab_awake(20, true, false);
        let ids: Vec<Entity> = (0..n).map(|i| {
            let p = at(i);
            let e = h.world.spawn(cow_bundle(p.0, p.1, 5 + i as u64)).id();
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
            e
        }).collect();
        if ordered {
            h.set_ordered_push(true);
        }
        let mut pairs = vec![];
        for t in 1..=120 {
            h.tick();
            if t == 60 || t == 120 {
                pairs.push(overlapping_pairs(&ids.iter().map(|&e| { let p = h.world.get::<Pos>(e).unwrap().0; (p.x, p.z) }).collect::<Vec<_>>(), w));
            }
        }
        results.push(pairs);
    }
    eprintln!("overlapping pairs at ticks 60 / 120: mob {mob:?}, ecs batch {:?}, ecs ordered {:?}", results[0], results[1]);
    let off = |e: &[usize]| (e[0] as f64 - mob[0] as f64).abs() / mob[0] as f64;
    // The ordered mode is closer to the `Mob` than the batch one (a small body, a hen, spreads out in a few ticks and
    // both modes then differ more in absolute terms).
    let dist = |e: &[usize]| (e[0] as f64 - mob[0] as f64).abs() + (e[1] as f64 - mob[1] as f64).abs();
    assert!(dist(&results[1]) < dist(&results[0]), "ordered {:?}, batch {:?}, mob {mob:?}", results[1], results[0]);
    let _ = off;
}

/// The two overlapping animals tick by tick: position and velocity (x) on both sides, and the pushes the `Mob` sent.
#[test]
#[ignore = "trace"]
fn trace_two_animals() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(20, false);
    let mut h = Herd::with_slab_awake(20, true, false);
    h.tick();
    let w = f64::from(species().width);
    let starts = [(0.5, 0.5), (0.5 + 0.6 * w, 0.5)];
    let mut ids = vec![];
    for (i, s) in starts.iter().enumerate() {
        r.add(Vec3::new(s.0, 1.0, s.1));
        let e = h.world.spawn(cow_bundle(s.0, s.1, 3 + i as u64)).id();
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
        ids.push(e);
    }
    if std::env::var_os("ORDERED_PUSH").is_some() {
        h.set_ordered_push(true);
    }
    for t in 1..=9 {
        let from = r.events.len();
        r.step(None);
        h.tick();
        let pushes: Vec<String> = r.events[from..].iter().filter_map(|e| match e {
            horizon_client_lib::mobs::MobEvent::Push { target, velocity } => Some(format!("{target:?} {:+.4}", velocity[0])),
            _ => None,
        }).collect();
        let m: Vec<String> = r.mobs.iter().map(|m| format!("x {:.4} vx {:+.4}", m.position.x, m.velocity.x)).collect();
        let e: Vec<String> = ids.iter().map(|&e| format!("x {:.4} vx {:+.4}", h.world.get::<Pos>(e).unwrap().0.x, h.world.get::<Vel>(e).unwrap().0.x)).collect();
        eprintln!("t={t}: MOB {m:?} pushes {pushes:?}\n     ECS {e:?}");
    }
}

/// Small groups (2, 3, 4, 6 animals that overlap at the start) in several layouts: the positions after 10 and 40 ticks and
/// the spread of the group, against the `Mob`. Prints one line per group size (`GROUP n=...`): the mean position error
/// at ticks 10 and 40 and the mean largest distance between two animals (Mob, ECS).
#[test]
#[ignore = "report"]
fn small_groups_report() {
    if !have_data() {
        return;
    }
    let _first = MobRef::new(20, false);
    let w = f64::from(species().width);
    let layouts = |n: usize| -> Vec<Vec<(f64, f64)>> {
        let mut out = vec![];
        for (ph, r) in [(0.0, 0.3), (0.7, 0.45), (2.1, 0.2)] {
            out.push((0..n).map(|i| {
                let a = ph + std::f64::consts::TAU * i as f64 / n as f64;
                (0.5 + r * w * a.cos(), 0.5 + r * w * a.sin())
            }).collect());
        }
        // A line, and a line with a bump.
        out.push((0..n).map(|i| (0.5 + 0.25 * w * i as f64, 0.5)).collect());
        out.push((0..n).map(|i| (0.5 + 0.3 * w * i as f64, 0.5 + 0.15 * w * (i % 2) as f64)).collect());
        out
    };
    for n in [2usize, 3, 4, 6] {
        let (mut e10, mut e40, mut sm, mut se, mut cnt) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for lay in layouts(n) {
            let mut r = MobRef::new(20, false);
            let mut h = Herd::with_slab_awake(20, true, false);
            let mut ids = vec![];
            for (i, s) in lay.iter().enumerate() {
                r.add(Vec3::new(s.0, 1.0, s.1));
                let e = h.world.spawn(cow_bundle(s.0, s.1, 3 + i as u64)).id();
                h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
                horizon_ecs_proto::cow::enter_index(&mut h.world, e);
                ids.push(e);
            }
            let pos_m = |r: &MobRef| r.mobs.iter().map(|m| (m.position.x, m.position.z)).collect::<Vec<_>>();
            let pos_e = |h: &Herd| ids.iter().map(|&e| { let p = h.world.get::<Pos>(e).unwrap().0; (p.x, p.z) }).collect::<Vec<_>>();
            let err = |a: &[(f64, f64)], b: &[(f64, f64)]| a.iter().zip(b).map(|(p, q)| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt()).sum::<f64>() / a.len() as f64;
            let spread = |a: &[(f64, f64)]| a.iter().flat_map(|p| a.iter().map(move |q| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt())).fold(0.0, f64::max);
            for t in 1..=40 {
                r.step(None);
                h.tick();
                if t == 10 {
                    e10 += err(&pos_m(&r), &pos_e(&h));
                }
            }
            e40 += err(&pos_m(&r), &pos_e(&h));
            sm += spread(&pos_m(&r));
            se += spread(&pos_e(&h));
            cnt += 1.0;
        }
        println!("GROUP n={n} err10={:.4} err40={:.4} spread_mob={:.3} spread_ecs={:.3}", e10 / cnt, e40 / cnt, sm / cnt, se / cnt);
    }
}
