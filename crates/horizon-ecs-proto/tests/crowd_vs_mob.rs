//! Crowds against a real `Mob` herd in the same scene: cramming (`maxEntityCramming` 24) and the way a tight group
//! spreads out. The species is the one of the process (`HORIZON_SPECIES`).
mod common;
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
    let mut h = Herd::with_slab(20, true, false);
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
