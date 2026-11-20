//! The squid (`HORIZON_SPECIES=minecraft:squid`) against a real `Mob` squid: the tentacle cycle that pushes it along the vector of its goals,
//! `Squid.travel` and the fall out of the water. The cycle and the vector are those of the `Mob` each tick (its draws are not the ecs ones);
//! 8 squids, one per level (the push between neighbours is not what is measured), 600 ticks in a pool, 60 ticks out of the water.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*, locomotion::swim::SquidState};

fn run(name: &str, wet: bool) -> f64 {
    let n = if wet { 600 } else { 60 };
    let mut worst = 0.0_f64;
    let mut compared = 0;
    for k in 0..8 {
        let (mut r, mut h) = flat_pair();
        if wet {
            for x in -16..=16 {
                for z in -16..=16 {
                    for y in 1..=5 {
                        r.set_block(x, y, z, "minecraft:water");
                        h.set_block(x, y, z, "minecraft:water");
                    }
                }
            }
            h.tick();
        }
        let s = Vec3::new(-6.0 + 1.5 * k as f64, if wet { 3.0 } else { 6.0 }, -5.0 + 1.1 * k as f64);
        let e = ecs_cow(&mut h, s.x, s.y, s.z, 1300 + k as u64);
        h.world.entity_mut(e).remove::<GoalSet>();
        let i = r.add(s);
        if wet {
            // The ecs animal learns it is in water at its first tick: it starts again from its place at rest.
            h.tick();
            h.tick();
            h.teleport(e, s);
            h.world.get_mut::<Vel>(e).unwrap().0 = Vec3::ZERO;
        }
        for t in 1..=n {
            // The `Mob` empties its vector once `noActionTime` passes 100 (no player near): the test keeps it at 0.
            r.mobs[i].ai.no_action_time = 0;
            let pre = (r.mobs[i].anim.v[0], r.mobs[i].anim.v[4]);
            r.step(None);
            let mut st = h.world.get_mut::<SquidState>(e).unwrap();
            (st.movement, st.speed, st.vector) = (pre.0, pre.1, r.mobs[i].extra.vector);
            h.tick();
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if std::env::var_os("TRACE2").is_some() && d > 0.01 && t % 20 == 0 {
                eprintln!("{name} {k} t={t} gap {d:.3} MOB ({:.3},{:.3},{:.3}) | ECS ({:.3},{:.3},{:.3})", a.x, a.y, a.z, b.x, b.y, b.z);
            }
            if std::env::var_os("TRACE9").is_some() && k == 3 && wet && (156..=163).contains(&t) {
                let em = h.world.entity(e);
                eprintln!("t={t} MOB y {:.4} vy {:.4} water {} mv {:.3} | ECS y {:.4} vy {:.4} water {} mv {:.3}", a.y, r.mobs[i].velocity.y, r.mobs[i].fluids.in_water, pre.0, b.y, em.get::<Vel>().unwrap().0.y, em.get::<FluidState>().unwrap().in_water, em.get::<SquidState>().unwrap().movement);
            }
            if std::env::var_os("TRACE3").is_some() && d > 1.0e-6 && k == 3 && wet {
                thread_local!(static ONCE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) });
                if ONCE.with(|o| { let n = o.get(); o.set(n + 1); n < 4 }) {
                    let em = h.world.entity(e);
                    eprintln!("t={t} gap {d:.5} MOB y {:.4} vel ({:.4},{:.4},{:.4}) water {} mv {:.3} | ECS y {:.4} vel ({:.4},{:.4},{:.4}) water {} inf {}", a.y, r.mobs[i].velocity.x, r.mobs[i].velocity.y, r.mobs[i].velocity.z, r.mobs[i].fluids.in_water, pre.0, b.y, em.get::<Vel>().unwrap().0.x, em.get::<Vel>().unwrap().0.y, em.get::<Vel>().unwrap().0.z, em.get::<FluidState>().unwrap().in_water, em.contains::<InFluid>());
                }
            }
            worst = worst.max(d);
            compared += 1;
        }
    }
    eprintln!("{name}: worst gap {worst:.4} over {compared} squid-ticks (8 squids, up to {n} ticks each)");
    worst
}

#[test]
fn a_squid_is_pushed_by_its_cycle_as_a_mob_is() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !matches!(process_species_name().as_str(), "minecraft:squid" | "minecraft:glow_squid") {
        return;
    }
    let wet = run("squid in water", true);
    let dry = run("squid out of water", false);
    assert!(wet == 0.0 && dry == 0.0, "wet {wet}, dry {dry}");
}

/// The goals with their own draws (the laws): 24 squids on each side in deep water, 400 ticks: the mean distance from the start and the mean
/// depth change. The `Mob`'s `noActionTime` is kept at 0 (no player near would empty the vector).
#[test]
fn the_squids_wander_as_the_mobs_do() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !matches!(process_species_name().as_str(), "minecraft:squid" | "minecraft:glow_squid") {
        return;
    }
    let (mut r, mut h) = flat_pair();
    for x in -18..=18 {
        for z in -18..=18 {
            for y in 1..=40 {
                r.set_block(x, y, z, "minecraft:water");
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    h.tick();
    let n = 24;
    let start: Vec<Vec3> = (0..n).map(|k| Vec3::new(-12.0 + k as f64, 20.0, -6.0 + 0.5 * k as f64)).collect();
    let ids: Vec<(usize, bevy_ecs::prelude::Entity)> = start.iter().enumerate().map(|(k, s)| (r.add(*s), ecs_cow(&mut h, s.x, s.y, s.z, 1500 + k as u64))).collect();
    h.tick();
    h.tick();
    for (k, &(_, e)) in ids.iter().enumerate() {
        h.teleport(e, start[k]);
        h.world.get_mut::<Vel>(e).unwrap().0 = Vec3::ZERO;
    }
    arm_all(&mut h.world);
    let (mut md, mut ed, mut mdy, mut edy, mut mstep, mut estep) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for t in 1..=400 {
        for &(i, _) in &ids {
            r.mobs[i].ai.no_action_time = 0;
        }
        let before: Vec<(Vec3, Vec3)> = ids.iter().map(|&(i, e)| (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0)).collect();
        r.step(None);
        h.tick();
        if t == 400 {
            for (k, &(i, e)) in ids.iter().enumerate() {
                let (a, b, s) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0, start[k]);
                md += ((a.x - s.x).powi(2) + (a.z - s.z).powi(2)).sqrt();
                ed += ((b.x - s.x).powi(2) + (b.z - s.z).powi(2)).sqrt();
                mdy += (a.y - s.y).abs();
                edy += (b.y - s.y).abs();
                mstep += ((a.x - before[k].0.x).powi(2) + (a.y - before[k].0.y).powi(2) + (a.z - before[k].0.z).powi(2)).sqrt();
                estep += ((b.x - before[k].1.x).powi(2) + (b.y - before[k].1.y).powi(2) + (b.z - before[k].1.z).powi(2)).sqrt();
            }
        }
    }
    let f = |v: f64| v / n as f64;
    eprintln!("squids wander: mob dist {:.2} dy {:.2} | ecs dist {:.2} dy {:.2}", f(md), f(mdy), f(ed), f(edy));
    let near = |a: f64, b: f64| (a - b).abs() <= 0.3 * a.max(b).max(1.0);
    assert!(near(f(md), f(ed)) && near(f(mdy), f(edy)), "the squids wander differently");
    let _ = (mstep, estep);
}

/// `SquidFlee`: a player hits the squid (the `Mob`'s `last_hurt_by`, the ecs `LastHurtBy` and the event of the blow) at 2.5 blocks: the squid
/// swims away. Both with their own goals (the draws of the random movement differ): the mean distance gained from the player after 60
/// ticks, 16 squids on each side.
#[test]
fn a_hurt_squid_flees_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if !matches!(process_species_name().as_str(), "minecraft:squid" | "minecraft:glow_squid") {
        return;
    }
    use horizon_client_lib::mobs::Who as MWho;
    let (mut r, mut h) = flat_pair();
    for x in -18..=18 {
        for z in -18..=18 {
            for y in 1..=40 {
                r.set_block(x, y, z, "minecraft:water");
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    h.tick();
    let n = 16;
    let me = Vec3::new(0.5, 20.0, 0.5);
    let start: Vec<Vec3> = (0..n).map(|k| Vec3::new(3.0 + 0.2 * k as f64, 20.0 + 0.1 * k as f64, 0.5 + 0.1 * k as f64)).collect();
    let ids: Vec<(usize, bevy_ecs::prelude::Entity)> = start.iter().enumerate().map(|(k, s)| (r.add(*s), ecs_cow(&mut h, s.x, s.y, s.z, 1700 + k as u64))).collect();
    h.tick();
    h.tick();
    for (k, &(_, e)) in ids.iter().enumerate() {
        h.teleport(e, start[k]);
        h.world.get_mut::<Vel>(e).unwrap().0 = Vec3::ZERO;
    }
    arm_all(&mut h.world);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: me.x, y: me.y, z: me.z, main: 0, off: 0, ..Default::default() });
    let view = player_view(me, None);
    let (mut md, mut ed) = (0.0, 0.0);
    for t in 1..=60 {
        for &(i, _) in &ids {
            r.mobs[i].ai.no_action_time = 0;
        }
        if t == 3 {
            for &(i, e) in &ids {
                r.mobs[i].ai.last_hurt_by = Some(MWho::Player);
                let now = h.world.resource::<Clock>().0;
                h.world.entity_mut(e).insert(LastHurtBy { who: horizon_ecs_proto::targeting::Who::Player(1), at: now });
                h.world.resource_mut::<horizon_ecs_proto::goals::Events>().hurt_by_entity.push(e);
            }
        }
        r.step(Some(&view));
        h.tick();
    }
    for (k, &(i, e)) in ids.iter().enumerate() {
        let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
        md += ((a.x - me.x).powi(2) + (a.y - me.y).powi(2) + (a.z - me.z).powi(2)).sqrt() - start[k].distance(me);
        ed += ((b.x - me.x).powi(2) + (b.y - me.y).powi(2) + (b.z - me.z).powi(2)).sqrt() - start[k].distance(me);
    }
    eprintln!("squids flee: mob gains {:.2} blocks, ecs {:.2}", md / n as f64, ed / n as f64);
    assert!(md / n as f64 > 2.0 && ed / n as f64 > 2.0, "they flee");
    assert!((md - ed).abs() / n as f64 <= 0.3 * (md / n as f64), "the flight differs");
}
