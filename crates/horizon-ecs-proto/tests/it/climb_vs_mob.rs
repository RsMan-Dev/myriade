//! The wall climber (`HORIZON_SPECIES=minecraft:spider`) against a real `Mob` spider: a wall of 5 blocks and a target on top of it. The path
//! cannot reach the top: the spider walks to the foot of the wall, presses against it and climbs (`Spider.climbing`, the velocity clamp, the
//! 0.2 of the climb), straight at the target (`WallClimberNavigation.pathToPosition`). Both are sent the same way; followed for 200 ticks.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::{go_to_at, GoalSet}};

#[test]
fn a_spider_climbs_a_wall_to_a_target_above_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if process_species_name() != "minecraft:spider" {
        return;
    }
    let starts = [(0.5, 1.0, 0.5), (-3.5, 1.0, 2.5), (1.5, 1.0, -3.5), (-1.5, 1.0, 4.5)];
    let goals = [[5.5, 6.0, 0.5], [5.5, 6.0, 2.5], [5.5, 6.0, -1.5], [4.5, 6.0, 4.5]];
    let mut worst = [0.0_f64; 3];
    let mut top = 0;
    for (k, (&(x, y, z), g)) in starts.iter().zip(&goals).enumerate() {
        let (mut r, mut h) = flat_pair();
        for bx in 4..=6 {
            for bz in -6..=6 {
                for by in 1..=5 {
                    r.set_block(bx, by, bz, "minecraft:stone");
                    h.set_block(bx, by, bz, "minecraft:stone");
                }
            }
        }
        h.tick();
        let (i, e) = (r.add(Vec3::new(x, y, z)), ecs_cow(&mut h, x, y, z, 1900 + k as u64));
        r.mobs[i].ai.goals = Default::default();
        r.mobs[i].ai.targets = Default::default();
        h.world.entity_mut(e).remove::<GoalSet>();
        for _ in 0..8 {
            r.step(None);
            h.tick();
        }
        let mw = r.w;
        r.mobs[i].nav_move_to(mw, *g, 1.0);
        go_to_at(&mut h.world, e, *g, 1.0, 1);
        for t in 1..=200_usize {
            r.step(None);
            h.tick();
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if std::env::var_os("TRACE2").is_some() && d > 0.05 && t % 10 == 0 {
                eprintln!("spider {k} t={t} gap {d:.3} MOB ({:.3},{:.3},{:.3}) | ECS ({:.3},{:.3},{:.3})", a.x, a.y, a.z, b.x, b.y, b.z);
            }
            for (n, lim) in [20, 100, 200].into_iter().enumerate() {
                if t <= lim && d > worst[n] {
                    worst[n] = d;
                }
            }
            if t == 200 && r.mobs[i].position.y >= g[1] - 0.5 {
                top += 1;
            }
        }
    }
    eprintln!("spider: worst gap to +20 {:.4}, +100 {:.4}, +200 {:.4}; {top}/4 at the height of the target (mob)", worst[0], worst[1], worst[2]);
    assert!(top >= 3, "the spiders hardly climbed");
    assert!(worst[0] < 0.15 && worst[2] < 0.5, "{worst:?}");
}
