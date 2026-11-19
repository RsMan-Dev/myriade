//! The flight of a parrot (`HORIZON_SPECIES=minecraft:parrot`) against a real `Mob` parrot: the path finder with the fly evaluator,
//! `FlyingMoveControl` and `travelFlying`, from the spot the `Mob` chose.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::fly_stroll, cow::*, goals::*};

fn parrot() -> bool {
    species().name == "minecraft:parrot"
}

/// A flight: the `Mob` draws when to fly and where; the ecs parrot is started at the same tick with the same spot (`Forced`), and the two are
/// followed for 40 ticks (the `Mob` draws its next flight later, which the ecs parrot is not told).
#[test]
fn a_parrot_flies_as_a_mob_does_when_the_spot_is_imposed() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !parrot() {
        return;
    }
    let spots: Vec<(f64, f64, f64)> = (0..4).flat_map(|i| (0..3).map(move |j| (-12.5 + 8.0 * f64::from(i), 5.5 + 1.0 * f64::from(j), -8.5 + 8.0 * f64::from(j)))).collect();
    let (mut r, _h) = flat_pair();
    let ids: Vec<usize> = spots.iter().map(|&(x, y, z)| r.add(Vec3::new(x, y, z))).collect();
    for &i in &ids {
        r.mobs[i].ai.no_action_time = 0;
    }
    let mut start: Vec<Option<(usize, [i32; 3])>> = vec![None; ids.len()];
    for t in 1..=400_usize {
        r.step(None);
        for (k, &i) in ids.iter().enumerate() {
            r.mobs[i].ai.no_action_time = 0;
            if start[k].is_none() && t < 350 && let Some(p) = &r.mobs[i].ai.nav.path {
                start[k] = Some((t, p.target));
                if std::env::var_os("TRACE").is_some() {
                    eprintln!("mob {k}: path at t={t} target {:?} goals {:?}", p.target, r.names(i));
                }
            }
        }
    }
    let begun: Vec<usize> = (0..ids.len()).filter(|&k| start[k].is_some()).collect();
    assert!(begun.len() >= 5, "only {} parrots flew", begun.len());
    let (mut r, mut h) = flat_pair();
    let birds: Vec<(usize, Entity)> = spots.iter().enumerate().map(|(k, &(x, y, z))| (r.add(Vec3::new(x, y, z)), ecs_cow(&mut h, x, y, z, 1100 + k as u64))).collect();
    for &(i, e) in &birds {
        r.mobs[i].ai.no_action_time = 0;
        h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<Stroll>(e).unwrap().scheduled = true;
    }
    let mut apart: Vec<Vec<f64>> = vec![Vec::new(); 41];
    for t in 1..=400_usize {
        for &(i, _) in &birds {
            r.mobs[i].ai.no_action_time = 0;
        }
        for &k in &begun {
            let (s, target) = start[k].unwrap();
            if t == s {
                let e = birds[k].1;
                h.world.get_mut::<Stroll>(e).unwrap().ty = target[1];
                force_spot(&mut h.world, e, &fly_stroll::MODULE, (f64::from(target[0]) + 0.5, f64::from(target[2]) + 0.5));
                fire(&mut h.world, e, &fly_stroll::MODULE);
            }
        }
        r.step(None);
        h.tick();
        if std::env::var_os("TRACE0").is_some() && t <= 8 {
            let (m, e) = (&r.mobs[birds[1].0], birds[1].1);
            eprintln!("pre t={t} MOB y {:.4} vy {:.4} ground {} nograv {} | ECS y {:.4} vy {:.4}", m.position.y, m.velocity.y, m.on_ground, m.no_gravity, h.world.get::<Pos>(e).unwrap().0.y, h.world.get::<Vel>(e).unwrap().0.y);
        }
        for &k in &begun {
            let (s, _) = start[k].unwrap();
            if std::env::var_os("TRACE").is_some() && t >= s && t <= s + 3 && k < 3 {
                let (m, e) = (&r.mobs[birds[k].0], birds[k].1);
                let (c, d) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Drive>(e).unwrap());
                eprintln!("k{k} t={t} MOB ({:.3},{:.3},{:.3}) yaw {:.1} | ECS ({:.3},{:.3},{:.3}) yaw {:.1} spd {:.3} yya {:.3}", m.position.x, m.position.y, m.position.z, m.yaw, c.x, c.y, c.z, h.world.get::<Yaw>(e).unwrap().0, d.speed, d.yya);
            }
            if t >= s && t < s + 40 {
                {
                let (a, b) = (r.mobs[birds[k].0].position, h.world.get::<Pos>(birds[k].1).unwrap().0);
                apart[t - s + 1].push(((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt());
            }
            }
        }
    }
    if std::env::var_os("TRACE1").is_some() {
        for &k in &begun {
            let (m, e) = (&r.mobs[birds[k].0], birds[k].1);
            let c = h.world.get::<Pos>(e).unwrap().0;
            eprintln!("end k{k} start {:?}: MOB ({:.2},{:.2},{:.2}) path {} | ECS ({:.2},{:.2},{:.2}) path {}", start[k].unwrap(), m.position.x, m.position.y, m.position.z, m.ai.nav.path.is_some(), c.x, c.y, c.z, h.world.get::<Nav>(e).unwrap().path.is_some());
        }
    }
    let close = |n: usize, d: f64| apart[n].iter().filter(|&&x| x < d).count();
    let line: Vec<String> = [2, 10, 30, 40].iter().map(|&n| format!("+{n}: {}/{} within 0.1, {} within 0.5", close(n, 0.1), apart[n].len(), close(n, 0.5))).collect();
    eprintln!("flight with the mob's spot: {}", line.join(" | "));
    assert!(close(2, 0.1) * 10 >= apart[2].len() * 9, "{line:?}");
    assert!(close(40, 0.5) * 10 >= apart[40].len() * 8, "{line:?}");
}
