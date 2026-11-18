//! The farm behaviours of the ecs (sheep: grazing and wool; hen: egg and wings) against a real `Mob` of the same
//! species in the same scenario. The species is the one of the process (`HORIZON_SPECIES`).
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_client_lib::mobs::MobEvent;
use horizon_ecs_proto::{cow::*, farm::*, goals::*};

fn is(name: &str) -> bool {
    species().name == name
}
/// Grass under both worlds for `n` blocks each way of the origin, and the same herd flat.
fn grass_pair(n: i32) -> (MobRef, Herd) {
    let (r, mut h) = flat_pair();
    for x in -n..=n {
        for z in -n..=n {
            r.set_block(x, 0, z, "minecraft:grass_block");
            h.set_block(x, 0, z, "minecraft:grass_block");
        }
    }
    h.tick();
    (r, h)
}

/// A sheep on grass: the `Mob` draws when it grazes; the ecs sheep is started at the same tick; the block goes at the
/// same moment (34 ticks in), the head comes up at the same moment (40), the grass block became dirt on both sides.
#[test]
fn a_sheep_grazes_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = grass_pair(8);
    if !is("minecraft:sheep") {
        return;
    }
    let spots = [(0.5, 0.5), (3.5, 0.5), (-3.5, 0.5), (0.5, 4.5), (0.5, -4.5), (4.5, 4.5)];
    let mobs: Vec<usize> = spots.iter().map(|&(x, z)| r.add(Vec3::new(x, 1.0, z))).collect();
    // First pass: the mobs alone, to read when each starts to graze and when its block goes.
    let mut log: Vec<(Option<i64>, Option<i64>, Option<i64>)> = vec![(None, None, None); mobs.len()];
    let mut was = vec![false; mobs.len()];
    let mut seen = 0;
    for t in 1..=3000_i64 {
        r.step(None);
        for (k, &i) in mobs.iter().enumerate() {
            let on = r.names(i).contains(&"EatBlock");
            if on && !was[k] && log[k].0.is_none() {
                log[k].0 = Some(t);
            }
            if !on && was[k] && log[k].2.is_none() {
                log[k].2 = Some(t);
            }
            was[k] = on;
        }
        for e in &r.events[seen..] {
            if let MobEvent::SetBlockName { mob: Some(id), name: "minecraft:dirt", .. } = e && let Some(k) = mobs.iter().position(|&i| r.mobs[i].id == *id) && log[k].1.is_none() {
                log[k].1 = Some(t);
            }
        }
        seen = r.events.len();
    }
    let began: Vec<usize> = (0..mobs.len()).filter(|&k| log[k].0.is_some() && log[k].1.is_some() && log[k].2.is_some()).collect();
    eprintln!("mob grazes (start, block, end): {:?}", log);
    assert!(!began.is_empty(), "no sheep grazed in 3000 ticks");
    // Second pass: the same world; the ecs sheep graze at the mob's start ticks.
    let (mut r, mut h) = grass_pair(8);
    let mobs: Vec<usize> = spots.iter().map(|&(x, z)| r.add(Vec3::new(x, 1.0, z))).collect();
    let sheep: Vec<Entity> = spots.iter().enumerate().map(|(k, &(x, z))| ecs_cow(&mut h, x, 1.0, z, 40 + k as u64)).collect();
    for &e in &sheep {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<GoalTimers>(e).unwrap().graze = true;
    }
    let mut rows = Vec::new();
    let mut ecs_block: Vec<Option<i64>> = vec![None; mobs.len()];
    let mut ecs_end: Vec<Option<i64>> = vec![None; mobs.len()];
    let mut ecs_start: Vec<Option<i64>> = vec![None; mobs.len()];
    for t in 1..=3000_i64 {
        for &k in &began {
            if Some(t) == log[k].0 {
                h.world.resource_mut::<Fired>().0.push((sheep[k], Kind::EatBlock));
            }
        }
        r.step(None);
        h.tick();
        for &k in &began {
            let g = h.world.get::<Grazing>(sheep[k]).copied();
            if let Some(g) = g {
                if ecs_start[k].is_none() {
                    ecs_start[k] = Some(t);
                }
                if g.eaten && ecs_block[k].is_none() {
                    ecs_block[k] = Some(t);
                }
            } else if ecs_start[k].is_some() && ecs_end[k].is_none() {
                ecs_end[k] = Some(t);
            }
        }
    }
    for &k in &began {
        let (s, b, e) = (log[k].0.unwrap(), log[k].1.unwrap(), log[k].2.unwrap());
        let (es, eb, ee) = (ecs_start[k].unwrap(), ecs_block[k].unwrap(), ecs_end[k].unwrap());
        rows.push((b - s, eb - es, e - s, ee - es));
        let (x, z) = (spots[k].0.floor() as i32, spots[k].1.floor() as i32);
        assert_eq!(r.w.block_name(x, 0, z), h.world.resource::<Level>().mw.0.block_name(x, 0, z), "the block under sheep {k}");
    }
    eprintln!("(block after start, head up after start) mob vs ecs: {rows:?}");
    for (mb, eb, me, ee) in &rows {
        assert!((mb - eb).abs() <= 1 && (me - ee).abs() <= 1, "{rows:?}");
    }
}

/// Short grass at the feet is broken (not the block under it), the wool grows back, and a baby ages up a minute.
#[test]
fn eating_breaks_short_grass_regrows_wool_and_ages_a_baby() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = grass_pair(8);
    if !is("minecraft:sheep") {
        return;
    }
    // A sheep standing in short grass; a baby standing on a grass block.
    r.set_block(-7, 1, -7, "minecraft:short_grass");
    h.set_block(-7, 1, -7, "minecraft:short_grass");
    let (a, b) = (r.add(Vec3::new(-6.5, 1.0, -6.5)), r.add(Vec3::new(5.5, 1.0, 5.5)));
    r.mobs[b].extra.age = -24000;
    r.mobs[a].extra.sheared = true;
    let (ea, eb) = (ecs_cow(&mut h, -6.5, 1.0, -6.5, 71), ecs_cow(&mut h, 5.5, 1.0, 5.5, 72));
    make_baby(&mut h.world, eb, 24000);
    h.world.get_mut::<Wool>(ea).unwrap().sheared = true;
    for &e in &[ea, eb] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<GoalTimers>(e).unwrap().graze = true;
    }
    h.tick();
    assert!(!shear(&mut h.world, eb), "a baby is not shorn");
    // Both graze at tick 5: the mobs by their own draw is not imposed here, the goal is started by hand.
    for t in 1..=60 {
        if t == 5 {
            h.world.resource_mut::<Fired>().0.push((ea, Kind::EatBlock));
            h.world.resource_mut::<Fired>().0.push((eb, Kind::EatBlock));
            for &i in &[a, b] {
                r.mobs[i].ai.goals.set_start_delay("EatBlock", 0);
            }
        }
        r.step(None);
        h.tick();
    }
    // The Mob cannot be forced to start (its draw is one in 500 per ask): its rule is read from `ate`; the ecs
    // outcome is what is checked, against the values of `Mob.ate`.
    let (sheared, now) = (h.world.get::<Wool>(ea).unwrap().sheared, h.world.resource::<Clock>().0);
    let level = *h.world.resource::<Level>();
    eprintln!("ecs: wool sheared {sheared}; block at the feet {}; below {}; baby GrowAt {:?} (tick {now})", level.mw.0.block_name(-7, 1, -7), level.mw.0.block_name(-7, 0, -7), h.world.get::<GrowAt>(eb).map(|g| g.0));
    let pb = h.world.get::<Pos>(eb).unwrap().0;
    eprintln!("baby at {pb:?}, below it {}", level.mw.0.block_name(pb.x.floor() as i32, 0, pb.z.floor() as i32));
    // The same effects on a real `Mob` (`Mob.ate`): the wool grows back, a baby ages up 1200 ticks.
    let (age_before, mut events) = (r.mobs[b].extra.age, Vec::new());
    horizon_client_lib::mobs::ai::ate(&mut r.mobs[a], &mut events);
    horizon_client_lib::mobs::ai::ate(&mut r.mobs[b], &mut events);
    assert!(!r.mobs[a].extra.sheared && r.mobs[b].extra.age == age_before + 1200, "mob: sheared {}, age {}", r.mobs[a].extra.sheared, r.mobs[b].extra.age);
    assert!(!sheared, "the wool grew back");
    assert_eq!(level.mw.0.block_name(-7, 1, -7), "minecraft:air", "short grass is broken");
    assert_eq!(level.mw.0.block_name(-7, 0, -7), "minecraft:grass_block", "the grass block under it stays");
    assert_eq!(level.mw.0.block_name(5, 0, 5), "minecraft:dirt", "a grass block under a baby turns to dirt");
    assert_eq!(h.world.get::<GrowAt>(eb).unwrap().0, 24000 + 1 - 1200, "a baby ages up 60 seconds");
}

/// How often sheep start to graze: 40 sheep on grass over 2000 ticks, `Mob` against ecs (each its own draws).
#[test]
fn sheep_graze_as_often_as_a_mob_s_do() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = grass_pair(10);
    if !is("minecraft:sheep") {
        return;
    }
    let mut cows = Vec::new();
    for i in 0..8 {
        for j in 0..5 {
            let (x, z) = (-8.5 + 2.4 * f64::from(i), -5.0 + 2.4 * f64::from(j));
            cows.push((r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 300 + cows.len() as u64)));
        }
    }
    for &(_, e) in &cows {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.get_mut::<GoalTimers>(e).unwrap().graze = false;
    }
    arm_all(&mut h.world);
    let (mut mob_starts, mut ecs_starts) = (0, 0);
    let (mut was_m, mut was_e) = (vec![false; cows.len()], vec![false; cows.len()]);
    for _ in 0..2000 {
        r.step(None);
        h.tick();
        for (k, &(i, e)) in cows.iter().enumerate() {
            let (m, c) = (r.names(i).contains(&"EatBlock"), h.world.get::<Grazing>(e).is_some());
            mob_starts += usize::from(m && !was_m[k]);
            ecs_starts += usize::from(c && !was_e[k]);
            (was_m[k], was_e[k]) = (m, c);
        }
    }
    eprintln!("graze starts in 2000 ticks, 40 sheep: mob {mob_starts}, ecs {ecs_starts} (a sheep eats where there is grass: the blocks run out)");
    assert!(mob_starts > 10 && ecs_starts as f64 > mob_starts as f64 * 0.7 && (ecs_starts as f64) < mob_starts as f64 * 1.4, "mob {mob_starts}, ecs {ecs_starts}");
}
