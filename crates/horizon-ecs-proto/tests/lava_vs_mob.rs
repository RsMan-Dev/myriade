//! The lava medium (`HORIZON_SPECIES=minecraft:strider`) against a real `Mob` strider: the path map with lava as ground, the travel on lava, the
//! float, the change of mode by the event of entering and leaving the lava. A strider is sent across a pool of lava; both are followed.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::go_to_at, locomotion::LavaMode};

fn strider() -> bool {
    species().name == "minecraft:strider"
}
/// A pool of lava 11 x 11 on the floor of the slab, in both worlds.
fn pools() -> (MobRef, Herd) {
    let (r, mut h) = flat_pair();
    for x in -5..=5 {
        for z in -5..=5 {
            r.set_block(x, 0, z, "minecraft:lava");
            h.set_block(x, 0, z, "minecraft:lava");
        }
    }
    h.tick();
    (r, h)
}

#[test]
fn a_strider_crosses_a_lava_pool_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !strider() {
        return;
    }
    // Inside the pool (the lava is `warm`: a strider on the ground is cold and slower, a species rule not tested here).
    let starts = [(-4.5, -2.5), (-4.5, 0.5), (-4.5, 3.5), (4.5, -3.5), (4.5, 1.5), (0.5, -4.5)];
    let mut worst = [0.0_f64; 3];
    let mut in_lava_ticks = 0;
    for (k, &(x0, z0)) in starts.iter().enumerate() {
        let (mut r, mut h) = pools();
        let (i, e) = (r.add(Vec3::new(x0, 1.0, z0)), ecs_cow(&mut h, x0, 1.0, z0, 400 + k as u64));
        // The `Mob` of this scene has no goals: the strider's own goals (stroll, the lava) would walk it elsewhere.
        h.world.entity_mut(e).insert(horizon_ecs_proto::goals::GoalSet::new(&[]));
        let goal = if x0.abs() < 1.0 { [x0, 0.0, 4.5] } else { [-x0, 0.0, z0] };
        // The ecs animal has no fire immunity yet (a species rule): it must live through the lava.
        h.world.get_mut::<Health>(e).unwrap().0 = 1.0e6;
        for _ in 0..6 {
            r.step(None);
            h.tick();
        }
        let speed = 1.0;
        let mw = r.w;
        let ok = r.mobs[i].nav_move_to(mw, goal, speed);
        if std::env::var_os("TRACE").is_some() {
            eprintln!("strider {k}: mob path {ok} from ({x0},{z0}) to {goal:?} at y {:.3}", r.mobs[i].position.y);
        }
        go_to_at(&mut h.world, e, goal, speed, 1);
        for t in 1..=200_usize {
            r.step(None);
            h.tick();
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if std::env::var_os("TRACE2").is_some() && d > 0.01 && t % 10 == 0 {
                eprintln!("strider {k} t={t} gap {d:.3} MOB ({:.3},{:.3},{:.3}) lava {} | ECS ({:.3},{:.3},{:.3}) lava mode {}", a.x, a.y, a.z, r.mobs[i].fluids.in_lava, b.x, b.y, b.z, h.world.get::<LavaMode>(e).is_some());
            }
            if r.mobs[i].fluids.in_lava {
                in_lava_ticks += 1;
            }
            for (n, lim) in [20, 100, 200].into_iter().enumerate() {
                if t <= lim && d > worst[n] {
                    worst[n] = d;
                }
            }
        }
    }
    eprintln!("lava: worst gap to +20 {:.4}, +100 {:.4}, +200 {:.4}; {in_lava_ticks} mob ticks in lava", worst[0], worst[1], worst[2]);
    assert!(in_lava_ticks > 100, "the striders hardly entered the lava");
    assert!(worst[0] < 0.1 && worst[2] < 0.5, "{worst:?}");
}
