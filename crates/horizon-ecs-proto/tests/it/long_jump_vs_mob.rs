//! The flight of a long jump (`HORIZON_SPECIES=minecraft:frog`): the velocity of the leap is given to both, the friction is discarded
//! (`LivingEntity.shouldDiscardFriction`: `Drive.discard_friction` against the `Mob`'s `extra.frog.discard_friction`) until the landing, and
//! the two are followed. The choice of the spot and the preparation (40 ticks of looking) are the goal's, not measured here.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::GoalSet;

#[test]
fn a_leap_with_discarded_friction_flies_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if species().name != "minecraft:frog" {
        return;
    }
    let leaps = [(0.5, 1.0, 0.5, 0.25, 0.7, 0.1), (-3.5, 1.0, 2.5, -0.2, 0.55, 0.3), (2.5, 1.0, -4.5, 0.05, 0.62, -0.28), (-6.5, 1.0, -6.5, 0.3, 0.8, 0.3)];
    let mut worst = 0.0_f64;
    for (k, &(x, y, z, vx, vy, vz)) in leaps.iter().enumerate() {
        let (mut r, mut h) = flat_pair();
        let (i, e) = (r.add(Vec3::new(x, y, z)), ecs_cow(&mut h, x, y, z, 3500 + k as u64));
        r.mobs[i].ai.goals = Default::default();
        r.mobs[i].ai.targets = Default::default();
        r.mobs[i].extra.animal_brain = None;
        h.world.entity_mut(e).remove::<GoalSet>();
        for _ in 0..6 {
            r.mobs[i].ai.no_action_time = 0;
            r.step(None);
            h.tick();
        }
        r.mobs[i].velocity = Vec3::new(vx, vy, vz);
        r.mobs[i].extra.frog.discard_friction = true;
        h.world.get_mut::<Vel>(e).unwrap().0 = Vec3::new(vx, vy, vz);
        h.world.get_mut::<Drive>(e).unwrap().discard_friction = true;
        h.world.entity_mut(e).insert(Moving);
        let mut landed = None;
        for t in 1..=60_usize {
            r.mobs[i].ai.no_action_time = 0;
            r.step(None);
            h.tick();
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if std::env::var_os("TRACE2").is_some() && d > 0.01 && t % 5 == 0 {
                eprintln!("leap {k} t={t} gap {d:.4} MOB ({:.3},{:.3},{:.3}) | ECS ({:.3},{:.3},{:.3})", a.x, a.y, a.z, b.x, b.y, b.z);
            }
            worst = worst.max(d);
            if landed.is_none() && t > 3 && r.mobs[i].on_ground {
                landed = Some(t);
                // The goal puts the friction back when the frog lands (`LongJumpMidJump`).
                r.mobs[i].extra.frog.discard_friction = false;
                h.world.get_mut::<Drive>(e).unwrap().discard_friction = false;
            }
        }
        eprintln!("leap {k}: landed at {landed:?}");
    }
    eprintln!("long jump: worst gap {worst:.4}");
    assert!(worst < 0.05, "{worst}");
}
