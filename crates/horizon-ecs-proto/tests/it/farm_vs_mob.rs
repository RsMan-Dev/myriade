//! The farm behaviours of the ecs (sheep: grazing and wool; hen: egg and wings) against a real `Mob` of the same
//! species in the same scenario. The species is the one of the process (`HORIZON_SPECIES`).
use crate::common;
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
        h.world.get_mut::<Graze>(e).unwrap().armed = true;
    }
    let mut rows = Vec::new();
    let mut ecs_block: Vec<Option<i64>> = vec![None; mobs.len()];
    let mut ecs_end: Vec<Option<i64>> = vec![None; mobs.len()];
    let mut ecs_start: Vec<Option<i64>> = vec![None; mobs.len()];
    for t in 1..=3000_i64 {
        for &k in &began {
            if Some(t) == log[k].0 {
                fire(&mut h.world, sheep[k], &horizon_ecs_proto::behaviours::eat_block::MODULE);
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
        h.world.get_mut::<Graze>(e).unwrap().armed = true;
    }
    h.tick();
    assert!(!shear(&mut h.world, eb), "a baby is not shorn");
    // Both graze at tick 5: the mobs by their own draw is not imposed here, the goal is started by hand.
    for t in 1..=60 {
        if t == 5 {
            fire(&mut h.world, ea, &horizon_ecs_proto::behaviours::eat_block::MODULE);
            fire(&mut h.world, eb, &horizon_ecs_proto::behaviours::eat_block::MODULE);
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
        h.world.get_mut::<Graze>(e).unwrap().armed = false;
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

fn hen() -> bool {
    is("minecraft:chicken")
}

/// The egg timer: an adult hen lays when its timer ends and draws the next in 6000..12000; a baby does not count,
/// and starts counting when it grows. The first laying is imposed from the `Mob`'s own draw.
#[test]
fn a_hen_lays_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    if !hen() {
        return;
    }
    let mut hens = Vec::new();
    for k in 0..30 {
        let (x, z) = (-12.0 + 2.5 * f64::from(k % 10), -6.0 + 3.0 * f64::from(k / 10));
        let (i, e) = (r.add(Vec3::new(x, 1.0, z)), ecs_cow(&mut h, x, 1.0, z, 500 + k as u64));
        // The mob draws its own timer at birth; the ecs hen takes the same (shortened to the first minute).
        let left = 20 + (k % 7) as i32 * 5;
        r.mobs[i].extra.egg_time = left;
        set_egg(&mut h.world, e, left as u64);
        hens.push((i, e));
    }
    // A baby: its timer is frozen; it grows at tick 100 (age -100), then lays 30 ticks later.
    let (bi, be) = (r.add(Vec3::new(8.5, 1.0, 5.5)), ecs_cow(&mut h, 8.5, 1.0, 5.5, 900));
    r.mobs[bi].extra.egg_time = 30;
    r.mobs[bi].extra.age = -100;
    set_egg(&mut h.world, be, 30);
    make_baby(&mut h.world, be, 100);
    let (mut mob_first, mut ecs_first) = (vec![None; hens.len() + 1], vec![None; hens.len() + 1]);
    let mut seen = 0;
    for t in 1..=160_i64 {
        r.step(None);
        h.tick();
        for ev in &r.events[seen..] {
            if let MobEvent::LayEgg { mob, .. } = ev && let Some(k) = (0..=hens.len()).find(|&k| r.mobs[if k < hens.len() { hens[k].0 } else { bi }].id == *mob) && mob_first[k].is_none() {
                mob_first[k] = Some(t);
            }
        }
        seen = r.events.len();
        for ev in h.world.resource::<LaidEggs>().0.iter() {
            if let Some(k) = (0..=hens.len()).find(|&k| (if k < hens.len() { hens[k].1 } else { be }) == ev.0) && ecs_first[k].is_none() {
                ecs_first[k] = Some(t);
            }
        }
    }
    eprintln!("first egg, mob: {:?}\nfirst egg, ecs: {:?}", mob_first, ecs_first);
    for k in 0..=hens.len() {
        let (m, c) = (mob_first[k].expect("mob laid"), ecs_first[k].expect("ecs laid"));
        assert!((m - c).abs() <= 1, "hen {k}: mob {m}, ecs {c}");
    }
    // The next timer: the draw is 6000 + nextInt(6000) on both sides.
    let mob_next: Vec<f64> = hens.iter().map(|&(i, _)| f64::from(r.mobs[i].extra.egg_time)).collect();
    let ecs_next: Vec<f64> = hens.iter().map(|&(_, e)| h.world.get::<Egg>(e).unwrap().left as f64).collect();
    let (mm, em) = (mean(&mob_next), mean(&ecs_next));
    eprintln!("next timer: mob mean {mm:.0} (min {:.0}, max {:.0}); ecs mean {em:.0} (min {:.0}, max {:.0})", mob_next.iter().cloned().fold(1e9, f64::min), mob_next.iter().cloned().fold(0.0, f64::max), ecs_next.iter().cloned().fold(1e9, f64::min), ecs_next.iter().cloned().fold(0.0, f64::max));
    assert!(mob_next.iter().chain(&ecs_next).all(|&v| (5800.0..12000.0).contains(&v)) && (mm - em).abs() < 1500.0);
}

/// A hen dropped from 30 blocks: the glide (`vy * 0.6` in the air), the height each tick, the landing, no damage, and the
/// wings' state during and after the fall.
#[test]
fn a_hen_falls_slowly_unhurt_and_flaps_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    if !hen() {
        return;
    }
    let i = r.add(Vec3::new(0.5, 31.0, 0.5));
    let e = ecs_cow(&mut h, 0.5, 31.0, 0.5, 5);
    h.world.get_mut::<Vel>(e).unwrap().0.y = 0.0;
    let (mut worst_y, mut worst_flap, mut worst_speed, mut worst_flapping) = (0.0_f64, 0.0_f32, 0.0_f32, 0.0_f32);
    let (mut mob_land, mut ecs_land) = (None, None);
    for t in 1..=400_i64 {
        r.step(None);
        h.tick();
        let (m, c) = (&r.mobs[i], h.world.get::<Pos>(e).unwrap().0);
        worst_y = worst_y.max((m.position.y - c.y).abs());
        if mob_land.is_none() && m.on_ground {
            mob_land = Some(t);
        }
        if ecs_land.is_none() && h.world.get::<Contact>(e).unwrap().on_ground {
            ecs_land = Some(t);
        }
        let f = h.world.get::<Flap>(e).unwrap();
        if [60, 266, 267, 268, 270, 300, 400].contains(&t) {
            eprintln!("t={t} flap mob {:.3} ecs {:.3}; flapping mob {:.4} ecs {:.4}; moving {}", m.anim.flap, f.flap, m.anim.flapping, f.flapping, h.world.get::<Moving>(e).is_some());
        }
        if t <= 60 || t == 400 {
            worst_flap = worst_flap.max((m.anim.flap - f.flap).abs());
            worst_speed = worst_speed.max((m.anim.flap_speed - f.speed).abs());
            worst_flapping = worst_flapping.max((m.anim.flapping - f.flapping).abs());
        }
    }
    eprintln!("hen: landed at tick mob {mob_land:?}, ecs {ecs_land:?}; worst height difference {worst_y:.4}; wings: flap {worst_flap:.3}, speed {worst_speed:.3}, flapping {worst_flapping:.4} apart; health mob {} ecs {}", r.mobs[i].health, health(&h, e));
    assert!(worst_y < 0.01 && (mob_land.unwrap() - ecs_land.unwrap()).abs() <= 1);
    assert!(r.mobs[i].health >= 4.0 && health(&h, e) >= 4.0, "no fall damage");
    // The wings agree to the digit in the air (ticks 1..60); after the landing the `Mob` takes one step of decay more
    // than the ecs (its `flapping` 0.81 against 0.90 on the landing tick), which leaves 0.1 x 18 = 1.8 degrees of `flap`.
    assert!(worst_speed < 0.01 && worst_flapping < 0.01 && worst_flap < 2.0, "wings");
}
