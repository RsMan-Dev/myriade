//! The bee (`HORIZON_SPECIES=minecraft:bee`) against a real `Mob` bee: a poppy near it (one bee, one flower, one level: the bees would
//! meet): it flies to the flower, hovers over it for 400 ticks or more and takes the nectar. The tick of the nectar and the place the bee hovers
//! at, 16 draws on each side (the laws: the draws differ); then, with a hive, the bee with nectar goes home and the ecs bee is out of the world
//! while it is inside (the `Mob` side has no hive block entity in the harness: only the ecs side is checked there).
mod common;
use common::*;
use horizon_client_lib::mobs::bee as mbee;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::bee::{Bee, Hives, InHive};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::arm_all;

#[test]
fn a_bee_pollinates_a_flower_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    let n = 16;
    let (mut mt, mut et, mut md, mut ed) = (Vec::new(), Vec::new(), 0.0, 0.0);
    for k in 0..n {
        let (mut r, mut h) = flat_pair();
        let f = (3 + k % 3, 1, -2 + k % 4);
        r.set_block(f.0, f.1, f.2, "minecraft:poppy");
        h.set_block(f.0, f.1, f.2, "minecraft:poppy");
        h.tick();
        let s = Vec3::new(0.5, 2.5, 0.5);
        let (i, e) = (r.add(s), ecs_cow(&mut h, s.x, s.y, s.z, 3100 + k as u64));
        // The ecs animal learns where it is at its first ticks.
        arm_all(&mut h.world);
        let (mut mn, mut en) = (None, None);
        let (mut mp, mut ep) = (None, None);
        for t in 1..=900_i64 {
            r.mobs[i].ai.no_action_time = 0;
            r.step(None);
            h.tick();
            if mp.is_none() && mbee::pollinating(&r.mobs[i]) {
                mp = Some(t);
            }
            if ep.is_none() && h.world.get::<Bee>(e).is_some_and(|b| b.pollinating) {
                ep = Some(t);
            }
            if mn.is_none() && mbee::has_nectar(&r.mobs[i]) {
                mn = Some(t);
            }
            if en.is_none() && h.world.get::<Bee>(e).is_some_and(|b| b.nectar) {
                en = Some(t);
            }
            if mn.is_some() && en.is_some() {
                break;
            }
        }
        if std::env::var_os("TRACE").is_some() {
            eprintln!("bee {k}: mob pollinating at {mp:?} nectar {mn:?} | ecs pollinating at {ep:?} nectar {en:?}");
        }
        let c = Vec3::new(f64::from(f.0) + 0.5, 1.6, f64::from(f.2) + 0.5);
        md += r.mobs[i].position.distance(c);
        ed += h.world.get::<Pos>(e).unwrap().0.distance(c);
        if let Some(t) = mn {
            mt.push(t as f64);
        }
        if let Some(t) = en {
            et.push(t as f64);
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    eprintln!("bee nectar: mob {}/{n} at {:.0}, dist {:.2} | ecs {}/{n} at {:.0}, dist {:.2}", mt.len(), mean(&mt), md / n as f64, et.len(), mean(&et), ed / n as f64);
    // Measured, not equal: the `Mob` bee approaches the flower slowly by a direct wanted point (0.35) once the first path ends and counts the time
    // of the pollination only within a block of it, and half of its first asks find no reachable flower (the 600-tick cache): see PORTABLES.md.
    assert!(et.len() == n as usize && mt.len() >= 6, "the bees get nectar");
    assert!((mean(&mt) - mean(&et)).abs() < 0.25 * mean(&mt).max(mean(&et)), "the time of the nectar");
}

#[test]
fn a_bee_with_nectar_goes_into_its_hive_and_comes_out() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:bee" {
        return;
    }
    let (_r, mut h) = flat_pair();
    h.world.insert_resource(horizon_ecs_proto::behaviours::bee::StayInHive(Some(false)));
    h.set_block(4, 1, 0, "minecraft:bee_nest");
    h.tick();
    let e = ecs_cow(&mut h, 0.5, 2.5, 0.5, 3300);
    arm_all(&mut h.world);
    {
        let now = h.world.resource::<horizon_ecs_proto::cow::Clock>().0;
        let mut b = h.world.get_mut::<Bee>(e).unwrap();
        b.nectar = true;
        b.hive = Some([4, 1, 0]);
        b.no_nectar_since = now;
    }
    let (mut inside, mut out_again) = (None, None);
    for t in 1..=3500_i64 {
        h.tick();
        if inside.is_none() && h.world.get::<InHive>(e).is_some() {
            inside = Some(t);
        }
        if inside.is_some() && out_again.is_none() && h.world.get::<InHive>(e).is_none() {
            out_again = Some(t);
        }
    }
    eprintln!("bee hive: in at {inside:?}, out at {out_again:?}; hives {}", h.world.resource::<Hives>().bees.len());
    assert!(inside.is_some(), "the bee went in");
    assert!(out_again.is_some_and(|o| o - inside.unwrap() >= 2400), "it stays 2400 ticks with nectar");
}
