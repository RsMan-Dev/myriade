//! The swim of a fish (`HORIZON_SPECIES=minecraft:cod`) against a real `Mob` cod: the path finder with the swim evaluator, the fish's move control
//! and `travelInWater`, the flop on the ground and the death by asphyxia.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::fish_swim, cow::*, goals::*};

fn cod() -> bool {
    species().name == "minecraft:cod"
}
/// A pool of water 33 x 33 blocks and 4 deep on the floor of the slab, in both worlds.
fn pools() -> (MobRef, Herd) {
    let (r, mut h) = flat_pair();
    for x in -16..=16 {
        for z in -16..=16 {
            for y in 1..=4 {
                r.set_block(x, y, z, "minecraft:water");
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    h.tick();
    (r, h)
}

/// A swim: the `Mob` draws when to swim and where; the ecs fish is started at the same tick with the same spot (`Forced`), and the two are
/// followed for 60 ticks.
#[test]
fn a_fish_swims_as_a_mob_does_when_the_spot_is_imposed() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !cod() {
        return;
    }
    let spots: Vec<(f64, f64, f64)> = (0..4).flat_map(|i| (0..3).map(move |j| (-9.5 + 6.0 * f64::from(i), 2.3 + 0.5 * f64::from(j), -6.5 + 6.0 * f64::from(j)))).collect();
    // First pass: the mobs alone, to read when and where each begins to swim.
    let (mut r, _h) = pools();
    let ids: Vec<usize> = spots.iter().map(|&(x, y, z)| r.add(Vec3::new(x, y, z))).collect();
    for &i in &ids {
        r.mobs[i].ai.no_action_time = 0;
    }
    let mut start: Vec<Option<(usize, [i32; 3])>> = vec![None; ids.len()];
    for t in 1..=300_usize {
        r.step(None);
        for (k, &i) in ids.iter().enumerate() {
            r.mobs[i].ai.no_action_time = 0;
            if start[k].is_none() && t < 250 && r.names(i).contains(&"FishSwimGoal") && let Some(p) = &r.mobs[i].ai.nav.path {
                start[k] = Some((t, p.target));
            }
        }
    }
    let begun: Vec<usize> = (0..ids.len()).filter(|&k| start[k].is_some()).collect();
    assert!(begun.len() >= 5, "only {} fish swam", begun.len());
    // Second pass: the mobs and the ecs fish together.
    let (mut r, mut h) = pools();
    let fish: Vec<(usize, Entity)> = spots.iter().enumerate().map(|(k, &(x, y, z))| (r.add(Vec3::new(x, y, z)), ecs_cow(&mut h, x, y, z, 900 + k as u64))).collect();
    for &(i, e) in &fish {
        r.mobs[i].ai.no_action_time = 0;
        h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<Stroll>(e).unwrap().scheduled = true;
    }
    let mut apart: Vec<Vec<f64>> = vec![Vec::new(); 61];
    for t in 1..=300_usize {
        for &(i, _) in &fish {
            r.mobs[i].ai.no_action_time = 0;
        }
        for &k in &begun {
            let (s, target) = start[k].unwrap();
            if t == s {
                let e = fish[k].1;
                h.world.get_mut::<Stroll>(e).unwrap().ty = target[1];
                force_spot(&mut h.world, e, &fish_swim::MODULE, (f64::from(target[0]) + 0.5, f64::from(target[2]) + 0.5));
                fire(&mut h.world, e, &fish_swim::MODULE);
            }
        }
        r.step(None);
        h.tick();
        for &k in &begun {
            let (s, _) = start[k].unwrap();
            if std::env::var_os("TRACE").is_some() && t >= s && t <= s + 4 && k < 3 {
                let (m, e) = (&r.mobs[fish[k].0], fish[k].1);
                let (c, d) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Drive>(e).unwrap());
                eprintln!("k{k} t={t} MOB ({:.3},{:.3},{:.3}) yaw {:.1} spd {:.3} | ECS ({:.3},{:.3},{:.3}) yaw {:.1} spd {:.3} swim {}", m.position.x, m.position.y, m.position.z, m.yaw, m.ai.speed, c.x, c.y, c.z, h.world.get::<Yaw>(e).unwrap().0, d.speed, h.world.get::<horizon_ecs_proto::locomotion::SwimMode>(e).is_some());
            }
            if t >= s && t < s + 60 {
                apart[t - s + 1].push(xz(r.mobs[fish[k].0].position, h.world.get::<Pos>(fish[k].1).unwrap().0));
            }
        }
    }
    let close = |n: usize, d: f64| apart[n].iter().filter(|&&x| x < d).count();
    let line: Vec<String> = [2, 10, 30, 60].iter().map(|&n| format!("+{n}: {}/{} within 0.1, {} within 0.5", close(n, 0.1), apart[n].len(), close(n, 0.5))).collect();
    eprintln!("swim with the mob's spot: {}", line.join(" | "));
    assert!(close(2, 0.1) * 10 >= apart[2].len() * 9, "{line:?}");
    assert!(close(60, 0.5) * 10 >= apart[60].len() * 9, "{line:?}");
}

/// A fish on dry ground: the air runs down, it flops, and it dies of asphyxia at the same tick as the `Mob`.
#[test]
fn a_fish_out_of_the_water_flops_and_dies_of_asphyxia_when_a_mob_does() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    if !cod() {
        return;
    }
    let (i, e) = (r.add(Vec3::new(0.5, 1.0, 0.5)), ecs_cow(&mut h, 0.5, 1.0, 0.5, 4));
    let (mut mob_dead, mut ecs_dead, mut flops_mob, mut flops_ecs) = (None, None, 0_i32, 0_i32);
    let (mut was_mob, mut was_ecs) = (false, false);
    for t in 1..=900_i64 {
        r.step(None);
        h.tick();
        if mob_dead.is_none() && mob_health(&r.mobs[i]) <= 0.0 {
            mob_dead = Some(t);
        }
        if ecs_dead.is_none() && h.world.get::<Health>(e).is_none() {
            ecs_dead = Some(t);
        }
        if std::env::var_os("TRACE").is_some() && t <= 12 {
            eprintln!("k t={t} MOB y {:.3} vy {:.3} ground {} | ECS y {:.3} vy {:.3} ground {} dry {} moving {}", r.mobs[i].position.y, r.mobs[i].velocity.y, r.mobs[i].on_ground, h.world.get::<Pos>(e).unwrap().0.y, h.world.get::<Vel>(e).unwrap().0.y, h.world.get::<Contact>(e).unwrap().on_ground, h.world.get::<horizon_ecs_proto::locomotion::swim::Dry>(e).is_some(), h.world.get::<Moving>(e).is_some());
        }
        // A flop is a jump off the ground (the vertical speed becomes 0.4 from rest).
        let (my, ey) = (r.mobs[i].velocity.y, h.world.get::<Vel>(e).map_or(0.0, |v| v.0.y));
        if my > 0.2 && !was_mob {
            flops_mob += 1;
        }
        if ey > 0.2 && !was_ecs {
            flops_ecs += 1;
        }
        (was_mob, was_ecs) = (my > 0.2, ey > 0.2);
    }
    eprintln!("dead at: mob {mob_dead:?}, ecs {ecs_dead:?}; flops (first 900 ticks): mob {flops_mob}, ecs {flops_ecs}");
    let (m, c) = (mob_dead.expect("the mob died"), ecs_dead.expect("the ecs fish died"));
    assert!((m - c).abs() <= 1, "mob {m}, ecs {c}");
    // The ecs fish flops before its travel (`AbstractFish.aiStep` calls `super.aiStep` after the flop), the `Mob` after it: one tick apart.
    assert!(flops_ecs > 3 && (flops_ecs - flops_mob).abs() <= 2, "flops: mob {flops_mob}, ecs {flops_ecs}");
}

/// The mode follows the water, by the events of entering and leaving it: in the pool the fish swims (`SwimMode`, its air full, not dry); out
/// of it, it walks (`WalkMode`, dry); back in, the air is full again.
#[test]
fn the_mode_follows_the_water_by_events() {
    use horizon_ecs_proto::locomotion::{SwimMode, WalkMode};
    use horizon_ecs_proto::locomotion::swim::Dry;
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !cod() {
        return;
    }
    let (_r, mut h) = pools();
    let e = ecs_cow(&mut h, 0.5, 2.5, 0.5, 8);
    let state = |h: &Herd| (h.world.get::<SwimMode>(e).is_some(), h.world.get::<WalkMode>(e).is_some(), h.world.get::<Dry>(e).is_some());
    for _ in 0..3 {
        h.tick();
    }
    assert_eq!(state(&h), (true, false, false), "in the pool: swims");
    h.teleport(e, Vec3::new(30.5, 1.0, 30.5));
    for _ in 0..40 {
        h.tick();
    }
    assert_eq!(state(&h), (false, true, true), "on the ground: walks, dry");
    assert!(h.world.get::<Air>(e).unwrap().0 < 300, "its air runs down on the ground");
    h.teleport(e, Vec3::new(0.5, 2.5, 0.5));
    for _ in 0..3 {
        h.tick();
    }
    assert_eq!(state(&h), (true, false, false), "back in the pool: swims");
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 300, "the air is full again");
}
