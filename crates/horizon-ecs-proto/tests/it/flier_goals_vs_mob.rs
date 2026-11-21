//! The goals of the vex and of the phantom against a real `Mob`, with no target (the laws, not the streams: the draws differ): a phantom circles
//! round its anchor (5 blocks above where it appeared), a vex drifts to air blocks near where it is. 48 of each, followed for 1200 ticks:
//! the mean distance from the anchor / the start and the mean height gap, taken every 10 ticks.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*};

fn laws(name: &str) {
    let n = 48;
    let (mut r, mut h) = flat_pair();
    let start: Vec<Vec3> = (0..n).map(|k| Vec3::new(-3.0 + 0.125 * k as f64, 8.0 + (k % 3) as f64, -3.0 + 0.11 * k as f64)).collect();
    let ids: Vec<(usize, bevy_ecs::prelude::Entity)> = start.iter().enumerate().map(|(k, s)| (r.add(*s), ecs_cow(&mut h, s.x, s.y, s.z, 800 + k as u64))).collect();
    for &(_, e) in &ids {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        // The anchor is made at the spawn (5 above the block); the test moves the animal after it is made.
        let p = h.world.get::<Pos>(e).unwrap().0;
        if name == "phantom" {
            h.world.entity_mut(e).insert(Anchor([p.x.floor() as i32, p.y.floor() as i32 + 5, p.z.floor() as i32]));
        } else {
            h.world.entity_mut(e).remove::<Anchor>();
        }
    }
    arm_all(&mut h.world);
    let (mut md, mut ed, mut my, mut ey, mut samples) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut mspeed, mut espeed) = (0.0, 0.0);
    for t in 1..=1200_usize {
        let before: Vec<(Vec3, Vec3)> = ids.iter().map(|&(i, e)| (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0)).collect();
        r.step(None);
        h.tick();
        if std::env::var_os("TRACE").is_some() && (t % 100 == 0 || t == 20) {
            let (i, e) = ids[0];
            let em = h.world.entity(e);
            eprintln!("t={t} MOB {:?} v {:?} | ECS {:?} v {:?} moving {} fly {} circling {} mt {:?} anchor {:?}", r.mobs[i].position, r.mobs[i].velocity, em.get::<Pos>().unwrap().0, em.get::<Vel>().unwrap().0, em.contains::<Moving>(), em.contains::<horizon_ecs_proto::locomotion::FlyMode>(), em.contains::<horizon_ecs_proto::behaviours::flier_goals::Circling>(), em.get::<MoveTo>().map(|m| (m.active, m.x, m.y, m.z)), em.get::<Anchor>().map(|a| a.0));
        }
        if t > 100 && t % 10 == 0 {
            for (k, &(i, e)) in ids.iter().enumerate() {
                let (a, b, s) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0, start[k]);
                // The anchor of the phantom: 5 above the start; the vex: the start (no anchor: where it is when it draws, near enough).
                let up = if name == "phantom" { 5.0 } else { 0.0 };
                md += ((a.x - s.x).powi(2) + (a.z - s.z).powi(2)).sqrt();
                ed += ((b.x - s.x).powi(2) + (b.z - s.z).powi(2)).sqrt();
                my += (a.y - s.y - up).abs();
                ey += (b.y - s.y - up).abs();
                mspeed += ((a.x - before[k].0.x).powi(2) + (a.y - before[k].0.y).powi(2) + (a.z - before[k].0.z).powi(2)).sqrt();
                espeed += ((b.x - before[k].1.x).powi(2) + (b.y - before[k].1.y).powi(2) + (b.z - before[k].1.z).powi(2)).sqrt();
                samples += 1.0;
            }
        }
    }
    if std::env::var_os("TRACE5").is_some() {
        let moved = ids.iter().zip(&start).filter(|((_, e), s)| { let p = h.world.get::<Pos>(*e).unwrap().0; (p.x - s.x).abs() + (p.z - s.z).abs() > 1.0 }).count();
        let mob_moved = ids.iter().zip(&start).filter(|((i, _), s)| { let p = r.mobs[*i].position; (p.x - s.x).abs() + (p.z - s.z).abs() > 1.0 }).count();
        eprintln!("{name}: moved ecs {moved}/{n}, mob {mob_moved}/{n}");
    }
    let f = |v: f64| v / samples;
    eprintln!("{name}: mob dist {:.2} dy {:.2} step {:.3} | ecs dist {:.2} dy {:.2} step {:.3}", f(md), f(my), f(mspeed), f(ed), f(ey), f(espeed));
    let near = |a: f64, b: f64| (a - b).abs() <= 0.25 * a.max(b).max(1.0);
    assert!(near(f(md), f(ed)) && near(f(my), f(ey)) && near(f(mspeed), f(espeed)), "{name} differs");
}

#[test]
fn a_phantom_circles_its_anchor_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if process_species_name() == "minecraft:phantom" {
        laws("phantom");
    }
}

#[test]
fn a_vex_drifts_near_its_origin_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if process_species_name() == "minecraft:vex" {
        laws("vex");
    }
}

/// With a player in the box: the phantoms take it for a target, circle above it and dive. The first dive (tick) and the number of dives in the
/// first 1000 ticks (the `Mob`'s flag, the ecs `Diving`), 24 phantoms on each side, the means. The player is not hurt on the `Mob` side (the
/// harness), so the ecs player is immortal.
#[test]
fn the_phantoms_take_the_player_and_dive_at_the_cadence_of_the_mob() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if process_species_name() != "minecraft:phantom" {
        return;
    }
    let n = 24;
    let (mut r, mut h) = flat_pair();
    let ids: Vec<(usize, bevy_ecs::prelude::Entity)> = (0..n)
        .map(|k| {
            let s = Vec3::new(-4.0 + 0.3 * k as f64, 9.0 + (k % 3) as f64, -4.0 + 0.3 * k as f64);
            let e = ecs_cow(&mut h, s.x, s.y, s.z, 900 + k as u64);
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
            h.world.entity_mut(e).insert(Anchor([s.x.floor() as i32, s.y.floor() as i32 + 5, s.z.floor() as i32]));
            (r.add(s), e)
        })
        .collect();
    arm_all(&mut h.world);
    let at = Vec3::new(0.5, 1.0, 0.5);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
    h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
    let view = player_view(at, None);
    let (mut mfirst, mut efirst, mut mdives, mut edives) = (vec![None; n], vec![None; n], vec![0; n], vec![0; n]);
    let (mut mflag, mut eflag) = (vec![false; n], vec![false; n]);
    for t in 1..=std::env::var("TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000_i64) {
        r.step(Some(&view));
        h.tick();
        if std::env::var_os("TRACE6").is_some() {
            let (i, e) = ids[0];
            let mt = (r.mobs[i].ai.target.is_some(), r.mobs[i].extra.flag);
            let et = (h.world.get::<horizon_ecs_proto::targeting::Target>(e).is_some(), h.world.get::<horizon_ecs_proto::behaviours::flier_goals::Diving>(e).is_some());
            thread_local!(static LAST: std::cell::Cell<((bool, bool), (bool, bool))> = const { std::cell::Cell::new(((false, false), (false, false))) });
            if std::env::var_os("TRACE7").is_some() && (245..=330).contains(&t) && t % 5 == 0 {
                eprintln!("t={t} ECS target {} dive {} strategy {} sweeping {} circling {}", et.0, et.1, h.world.get::<horizon_ecs_proto::behaviours::flier_goals::Strategy>(e).is_some(), h.world.get::<horizon_ecs_proto::behaviours::flier_goals::Sweeping>(e).is_some(), h.world.get::<horizon_ecs_proto::behaviours::flier_goals::Circling>(e).is_some());
            }
            if LAST.with(|l| l.get()) != (mt, et) {
                LAST.with(|l| l.set((mt, et)));
                eprintln!("t={t} MOB target {} dive {} y {:.1} | ECS target {} dive {} y {:.1}", mt.0, mt.1, r.mobs[i].position.y, et.0, et.1, h.world.get::<Pos>(e).unwrap().0.y);
            }
        }
        for (k, &(i, e)) in ids.iter().enumerate() {
            let (m, c) = (r.mobs[i].extra.flag, h.world.get::<horizon_ecs_proto::behaviours::flier_goals::Diving>(e).is_some());
            if m && !mflag[k] {
                mdives[k] += 1;
                mfirst[k].get_or_insert(t);
            }
            if c && !eflag[k] {
                edives[k] += 1;
                efirst[k].get_or_insert(t);
            }
            (mflag[k], eflag[k]) = (m, c);
        }
    }
    let mean = |v: &[Option<i64>]| v.iter().flatten().sum::<i64>() as f64 / v.iter().flatten().count().max(1) as f64;
    let (mn, en) = (mfirst.iter().flatten().count(), efirst.iter().flatten().count());
    eprintln!("phantom dives: mob {mn}/{n} first at {:.0}, {} dives | ecs {en}/{n} first at {:.0}, {} dives", mean(&mfirst), mdives.iter().sum::<i32>(), mean(&efirst), edives.iter().sum::<i32>());
    assert!(mn > 12 && en > 12, "most phantoms dive");
    assert!((mean(&mfirst) - mean(&efirst)).abs() < 0.3 * mean(&mfirst).max(mean(&efirst)), "the first dive");
    let (a, b) = (f64::from(mdives.iter().sum::<i32>()), f64::from(edives.iter().sum::<i32>()));
    assert!((a - b).abs() < 0.1 * a, "the dives in the run: mob {a}, ecs {b}");
}
