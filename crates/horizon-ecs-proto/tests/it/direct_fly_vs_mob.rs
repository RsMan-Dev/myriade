//! The direct flight (`fly::Control::Direct`, a vex through the blocks) and the banking flight (`Control::Bank`, a phantom) against a real `Mob`
//! of the species (`HORIZON_SPECIES=minecraft:vex` or `minecraft:phantom`): the points to fly to are imposed on both (the goals draw them);
//! the goal-less `Mob` and the ecs animal are followed for 400 ticks.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*, locomotion::fly::steer_to};

fn run(name: &str, mob_wants: impl Fn(&mut horizon_client_lib::mobs::Mob, [f64; 3], f64), start_ground: f64) {
    let spots = [(-12.5, 6.5, -8.5), (3.5, 8.5, 7.5), (-4.5, 7.0, -3.5), (8.5, 9.5, 5.5), (0.5, 6.5, -7.5), (-10.5, 8.5, 4.5)];
    let goals = [[6.5, 12.5, 6.5], [-8.5, 5.5, 3.5], [2.5, 14.5, -9.5], [-3.5, 8.5, 9.5]];
    let mut worst = [0.0_f64; 3];
    for (k, &(x0, y0, z0)) in spots.iter().enumerate() {
        let (mut r, mut h) = flat_pair();
        let (i, e) = (r.add(Vec3::new(x0, y0 + start_ground, z0)), ecs_cow(&mut h, x0, y0 + start_ground, z0, 600 + k as u64));
        h.world.entity_mut(e).remove::<GoalSet>();
        h.world.get_mut::<Yaw>(e).unwrap().0 = r.mobs[i].yaw;
        r.mobs[i].ai.goals = Default::default();
        r.mobs[i].ai.targets = Default::default();
        if std::env::var_os("TRACE").is_some() && k == 0 {
            eprintln!("{name}: mob attrs.speed {} | ecs Drive.speed {} Body.speed {}", r.mobs[i].attrs.speed, h.world.get::<Drive>(e).unwrap().speed, h.world.get::<Body>(e).unwrap().speed);
        }
        for t in 1..=400_usize {
            if (t - 1) % 70 == 0 {
                let g = goals[((t - 1) / 70 + k) % goals.len()];
                mob_wants(&mut r.mobs[i], g, 1.0);
                steer_to(&mut h.world, e, g, 1.0);
            }
            r.step(None);
            h.tick();
            if std::env::var_os("TRACE").is_some() && k == 0 && t <= 4 {
                eprintln!("t={t} MOB v {:?} speed {} yaw {} | ECS v {:?} speed {} yaw {}", r.mobs[i].velocity, r.mobs[i].attrs.speed, r.mobs[i].yaw, h.world.get::<Vel>(e).unwrap().0, h.world.get::<Drive>(e).unwrap().speed, h.world.get::<Yaw>(e).unwrap().0);
            }
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if d > 0.05 && std::env::var_os("TRACE2").is_some() && t % 10 == 0 {
                eprintln!("{name} {k} t={t} gap {d:.3} MOB ({:.3},{:.3},{:.3}) | ECS ({:.3},{:.3},{:.3})", a.x, a.y, a.z, b.x, b.y, b.z);
            }
            for (n, lim) in [20, 100, 400].into_iter().enumerate() {
                if t <= lim && d > worst[n] {
                    worst[n] = d;
                }
            }
        }
    }
    eprintln!("{name}: worst gap to +20 {:.4}, +100 {:.4}, +400 {:.4}", worst[0], worst[1], worst[2]);
    assert!(worst[0] < 0.1 && worst[2] < 0.5, "{worst:?}");
}

#[test]
fn a_vex_flies_straight_at_the_wanted_point_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:vex" {
        return;
    }
    run("vex", |m, g, s| m.ai.mov.set_wanted(g[0], g[1], g[2], s), 0.0);
}

#[test]
fn a_phantom_banks_toward_the_wanted_point_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:phantom" {
        return;
    }
    run("phantom", |m, g, _| m.extra.vector = g, 6.0);
}
