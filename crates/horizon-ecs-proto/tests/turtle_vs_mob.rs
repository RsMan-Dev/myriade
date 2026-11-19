//! The amphibious mode (`HORIZON_SPECIES=minecraft:turtle`) against a real `Mob` turtle: the path finder with the amphibious evaluator on land and
//! in water, `TurtleMoveControl` in both media, `travelInWater` at 0.1, the node reached within 1 vertically. The turtle is sent from the shore
//! into a pool and back; both are followed.
mod common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::{go_to_at, GoalSet}};

fn turtle() -> bool {
    species().name == "minecraft:turtle"
}
/// A pool of water 12 x 25 blocks, 3 deep, on the floor of the slab (x -12..=-1), in both worlds.
fn pools() -> (MobRef, Herd) {
    let (r, mut h) = flat_pair();
    for x in -12..=-1 {
        for z in -12..=12 {
            for y in 1..=3 {
                r.set_block(x, y, z, "minecraft:water");
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    h.tick();
    (r, h)
}

#[test]
fn a_turtle_goes_from_the_shore_into_the_water_and_back_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !turtle() {
        return;
    }
    // (start, goal): land to water, water to land, water to water, land to land.
    let legs: [([f64; 3], [f64; 3]); 6] = [
        ([6.5, 1.0, -6.5], [-7.5, 2.0, -6.5]),
        ([4.5, 1.0, 0.5], [-9.5, 1.0, 3.5]),
        ([-8.5, 2.0, 5.5], [6.5, 1.0, 5.5]),
        ([-10.5, 2.0, -3.5], [-3.5, 2.0, 4.5]),
        ([2.5, 1.0, 8.5], [10.5, 1.0, 2.5]),
        ([-2.5, 1.0, 10.5], [-10.5, 1.0, -10.5]),
    ];
    // Every leg is compared: from land into the water, from the water to land, and inside each.
    let asserted = |_: usize| true;
    let mut worst = [0.0_f64; 3];
    let mut moved = 0;
    let mut water_start_worst = 0.0_f64;
    for (k, &(s, g)) in legs.iter().enumerate() {
        let (mut r, mut h) = pools();
        let (i, e) = (r.add(Vec3::new(s[0], s[1], s[2])), ecs_cow(&mut h, s[0], s[1], s[2], 500 + k as u64));
        // The `Mob`'s own goals (home, travel, stroll, panic) would send it elsewhere: the leg is the one imposed.
        r.mobs[i].ai.goals = Default::default();
        // The ecs turtle's own goals (stroll, travel, go to water) would move it too.
        h.world.entity_mut(e).remove::<GoalSet>();
        // The ecs animal learns it is in water at its first tick (the event of entering a fluid): it starts again from its place at rest.
        h.tick();
        h.tick();
        h.teleport(e, Vec3::new(s[0], s[1], s[2]));
        h.world.get_mut::<Vel>(e).unwrap().0 = Vec3::ZERO;
        for q in 0..8 {
            r.step(None);
            h.tick();
            if std::env::var_os("TRACE3").is_some() && k == 2 {
                eprintln!("moving {} following {}", h.world.entity(e).contains::<Moving>(), h.world.entity(e).contains::<Following>());
                eprintln!("pre {q} MOB ({:.3},{:.3},{:.3}) vy {:.4} water {} | ECS ({:.3},{:.3},{:.3}) vy {:.4}", r.mobs[i].position.x, r.mobs[i].position.y, r.mobs[i].position.z, r.mobs[i].velocity.y, r.mobs[i].fluids.in_water, h.world.get::<Pos>(e).unwrap().0.x, h.world.get::<Pos>(e).unwrap().0.y, h.world.get::<Pos>(e).unwrap().0.z, h.world.get::<Vel>(e).unwrap().0.y);
            }
        }
        let mw = r.w;
        let ok = r.mobs[i].nav_move_to(mw, g, 1.0);
        go_to_at(&mut h.world, e, g, 1.0, 1);
        if std::env::var_os("TRACE").is_some() {
            eprintln!("turtle {k}: mob path {ok}");
        }
        if std::env::var_os("TRACE5").is_some() && !asserted(k) {
            let nodes = |p: &horizon_client_lib::mobs::path::Path| p.nodes.iter().take(6).map(|n| (n.x, n.y, n.z)).collect::<Vec<_>>();
            let mp = r.mobs[i].ai.nav.path.as_ref().map(|p| (p.nodes.len(), nodes(p)));
            r.step(None);
            h.tick();
            let ep = h.world.get::<Nav>(e).and_then(|n| n.path.as_ref().map(|p| (p.nodes.len(), nodes(p))));
            eprintln!("turtle {k}: start {:?} goal {:?}\n  MOB path {:?} mob in_water {} width {}\n  ECS path {:?} (after one tick) InFluid {} fs.in_water {} body.width {}", s, g, mp, r.mobs[i].fluids.in_water, r.mobs[i].width(), ep, h.world.entity(e).contains::<InFluid>(), h.world.get::<FluidState>(e).unwrap().in_water, h.world.get::<Body>(e).unwrap().width);
        }
        for t in 1..=300_usize {
            r.step(None);
            h.tick();
            if std::env::var_os("TRACE4").is_some() && k == 2 && t <= 6 {
                eprintln!("t={t} MOB y {:.4} vy {:.4} speed {:.3} | ECS y {:.4} vy {:.4} speed {:.3} moving {}", r.mobs[i].position.y, r.mobs[i].velocity.y, r.mobs[i].ai.speed, h.world.get::<Pos>(e).unwrap().0.y, h.world.get::<Vel>(e).unwrap().0.y, h.world.get::<Drive>(e).unwrap().speed, h.world.entity(e).contains::<Moving>());
            }
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if std::env::var_os("TRACE2").is_some() && d > 0.05 && t % 15 == 0 {
                eprintln!("turtle {k} t={t} gap {d:.3} MOB ({:.3},{:.3},{:.3}) | ECS ({:.3},{:.3},{:.3})", a.x, a.y, a.z, b.x, b.y, b.z);
            }
            if !asserted(k) {
                water_start_worst = water_start_worst.max(d);
                continue;
            }
            for (n, lim) in [20, 100, 300].into_iter().enumerate() {
                if t <= lim && d > worst[n] {
                    worst[n] = d;
                }
            }
            if t == 300 && asserted(k) && ((a.x - s[0]).abs() + (a.z - s[2]).abs() > 3.0) {
                moved += 1;
            }
        }
    }
    eprintln!("turtle: worst gap to +20 {:.4}, +100 {:.4}, +300 {:.4}; {moved}/6 moved; water-start legs included ({water_start_worst:.2} not asserted separately)", worst[0], worst[1], worst[2]);
    assert!(moved >= 5, "the turtles hardly moved");
    assert!(worst[0] < 0.1 && worst[2] < 0.5, "{worst:?}");
}
