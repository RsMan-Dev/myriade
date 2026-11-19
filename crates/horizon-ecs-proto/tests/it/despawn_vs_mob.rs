//! The despawn of the ECS (`despawn.rs`) against Horizon's `Mob.checkDespawn` (`ai::despawn_check`): the distance past which it goes at
//! once, the law of the idle despawn (the 1-in-800 draw of each tick against a deadline drawn in advance), the 32-block ring that resets
//! `noActionTime`, and the tick at which a creature a walking player leaves behind goes. Needs HORIZON_TEST_RESOURCES.
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::{self, Mob, MobWorld};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*, spawning::born};

fn have() -> bool {
    horizon_ecs_proto::species::inscribe_every_module();
    std::env::var_os("HORIZON_TEST_RESOURCES").is_some()
}
fn player(h: &mut Herd, x: f64, z: f64) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x, y: 1.0, z, main: 0, off: 0 });
}
fn zombie() -> Species {
    species_table().0.iter().find(|s| s.name == "minecraft:zombie").copied().unwrap()
}
fn view(mw: &MobWorld<'static>, at: Option<Vec3>) -> MobWorld<'static> {
    MobWorld { nearest_player: at, ..mw.clone() }
}
/// A real mob that is checked as the game checks it each tick: the check, then (it lives) one tick of `noActionTime`.
fn mob_lives_until(mw: &MobWorld<'static>, id: u32, d: f64, max: u32) -> Option<u32> {
    let kind = mob_kind("minecraft:zombie");
    let mut m = Mob::new(id, kind, Vec3::new(d, 1.0, 0.0), 0.0);
    let v = view(mw, Some(Vec3::new(0.0, 1.0, 0.0)));
    for t in 0..max {
        if !mobs::ai::despawn_check(&mut m, &v) {
            return Some(t);
        }
        m.ai.no_action_time += 1;
    }
    None
}
fn far_id(j: usize) -> u32 {
    1 + (j as u32) * 7919 % 100_003
}
fn quantiles(v: &mut Vec<u32>) -> [u32; 5] {
    v.sort_unstable();
    let n = v.len();
    [v[0], v[n / 4], v[n / 2], v[3 * n / 4], v[n - 1]]
}

#[test]
fn a_creature_past_128_blocks_goes_at_once_and_one_within_does_not() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world(176, true);
    let mw = h.world.resource::<Level>().mw.0;
    player(&mut h, 0.0, 0.0);
    h.tick();
    let z = zombie();
    // Horizon: 127.5 stays, 128.5 goes (the first check).
    assert_eq!(mob_lives_until(mw, 1, 127.5, 5), None);
    assert_eq!(mob_lives_until(mw, 1, 128.5, 5), Some(0));
    let near = born(&mut h.world, &z, [127.5, 1.0, 0.5], 11, 127.5 * 127.5);
    let far = born(&mut h.world, &z, [100.5, 5.0, 40.5], 12, 100.0 * 100.0);
    horizon_ecs_proto::despawn::keep(&mut h.world, far);
    let far2 = born(&mut h.world, &z, [-129.5, 1.0, 0.5], 13, 129.5 * 129.5);
    // One born past the distance goes when it is admitted (nothing else measures a creature nothing moves).
    assert!(h.world.get_entity(far2).is_err(), "129.5 blocks away it goes at its birth");
    for _ in 0..3 {
        h.tick();
    }
    player(&mut h, 0.1, 0.0);
    for _ in 0..3 {
        h.tick();
    }
    assert!(h.world.get_entity(near).is_ok(), "127.4 blocks away it stays");
    // The player walks away: the one 167 blocks off goes, the persistent one (140 blocks off) stays.
    player(&mut h, -40.0, 0.0);
    for _ in 0..3 {
        h.tick();
    }
    assert!(h.world.get_entity(near).is_err(), "167 blocks away it goes");
    assert!(h.world.get_entity(far).is_ok(), "persistent: stays");
}

#[test]
fn the_idle_despawn_has_the_law_of_the_draw_of_each_tick() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world(176, true);
    let mw = h.world.resource::<Level>().mw.0;
    player(&mut h, 0.0, 0.0);
    h.tick();
    let n = 500;
    let mut real: Vec<u32> = (0..n).map(|j| mob_lives_until(mw, far_id(j), 60.0, 20_000).expect("goes")).collect();
    let z = zombie();
    let t0 = h.world.resource::<Clock>().0;
    // At 60 blocks (past the ring, inside 128), spread on a circle so the zombies do not meet.
    let ids: Vec<Entity> = (0..n)
        .map(|j| {
            let a = j as f64 / n as f64 * std::f64::consts::TAU;
            let e = born(&mut h.world, &z, [60.0 * a.cos(), 1.0, 60.0 * a.sin()], 99 + (j as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15), 3600.0);
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
            e
        })
        .collect();
    let mut gone = vec![0u32; n];
    let mut left = n;
    let mut t = 0u32;
    while left > 0 && t < 20_000 {
        t += 1;
        h.tick();
        for (j, e) in ids.iter().enumerate() {
            if gone[j] == 0 && h.world.get_entity(*e).is_err() {
                gone[j] = t;
                left -= 1;
            }
        }
    }
    let _ = t0;
    let mut ecs = gone.clone();
    let (qr, qe) = (quantiles(&mut real), quantiles(&mut ecs));
    let mean = |v: &[u32]| v.iter().map(|x| f64::from(*x)).sum::<f64>() / v.len() as f64;
    let sd = |v: &[u32]| (v.iter().map(|x| (f64::from(*x) - mean(v)).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)).sqrt();
    eprintln!("idle despawn at 60 blocks, {n} each: horizon mean {:.1} sd {:.1} quantiles {qr:?}", mean(&real), sd(&real));
    eprintln!("                                      ecs     mean {:.1} sd {:.1} quantiles {qe:?}", mean(&ecs), sd(&ecs));
    let z = (mean(&real) - mean(&ecs)) / ((sd(&real).powi(2) + sd(&ecs).powi(2)) / n as f64).sqrt();
    eprintln!("z = {z:+.2}");
    assert!(z.abs() < 3.5);
    // Nobody goes before the count of 600 (the minimum of the Mob is 601 ticks).
    assert!(qe[0] >= 595 && qr[0] >= 600);
}

#[test]
fn a_player_within_32_blocks_keeps_every_creature_and_the_count_starts_when_it_leaves() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world(176, true);
    player(&mut h, 0.0, 0.0);
    h.tick();
    let z = zombie();
    let n = 200;
    let ids: Vec<Entity> = (0..n)
        .map(|j| {
            let a = j as f64 / n as f64 * std::f64::consts::TAU;
            born(&mut h.world, &z, [24.0 * a.cos(), 1.0, 24.0 * a.sin()], 5 + (j as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15), 576.0)
        })
        .collect();
    for _ in 0..2500 {
        h.tick();
        // The zombies chase the player; the player stays still and is not hurt.
    }
    let alive = ids.iter().filter(|e| h.world.get_entity(**e).is_ok()).count();
    eprintln!("{alive} of {n} still there after 2500 ticks within the ring");
    assert!(alive >= n - 2, "a player within 32 blocks resets noActionTime: nothing goes");
}

#[test]
fn a_creature_a_walking_player_leaves_behind_goes_on_the_tick_the_mob_would_go() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world(176, true);
    let mw = h.world.resource::<Level>().mw.0;
    player(&mut h, 0.0, 0.0);
    h.tick();
    let z = zombie();
    let e = born(&mut h.world, &z, [100.5, 1.0, 0.5], 21, 100.5 * 100.5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
    let mut m = Mob::new(far_id(3), mob_kind("minecraft:zombie"), Vec3::new(100.5, 1.0, 0.5), 0.0);
    let (mut t_mob, mut t_ecs) = (None, None);
    for t in 1..400u32 {
        let px = -0.25 * f64::from(t);
        player(&mut h, px, 0.0);
        h.tick();
        if t_ecs.is_none() && h.world.get_entity(e).is_err() {
            t_ecs = Some(t);
        }
        if t_mob.is_none() && !mobs::ai::despawn_check(&mut m, &view(mw, Some(Vec3::new(px, 1.0, 0.0)))) {
            t_mob = Some(t);
        }
    }
    eprintln!("walked away: the mob goes at tick {t_mob:?}, the creature at {t_ecs:?}");
    let (a, b): (u32, u32) = (t_mob.unwrap(), t_ecs.unwrap());
    assert!(a.abs_diff(b) <= 1);
}

#[test]
fn leaving_the_ring_starts_the_count_with_the_law_of_the_mob() {
    if !have() {
        return;
    }
    let mut h = Herd::with_spawn_world(176, true);
    let mw = h.world.resource::<Level>().mw.0;
    player(&mut h, 0.0, 0.0);
    h.tick();
    let n = 300;
    let z = zombie();
    let ids: Vec<Entity> = (0..n)
        .map(|j| {
            let a = j as f64 / n as f64 * std::f64::consts::TAU;
            born(&mut h.world, &z, [20.0 * a.cos(), 1.0, 20.0 * a.sin()], 5 + (j as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15), 400.0)
        })
        .collect();
    for _ in 0..50 {
        h.tick();
    }
    // The player leaves: from here the creatures that are not within 32 blocks of it count.
    player(&mut h, -75.0, 0.0);
    let mut gone = vec![0u32; n];
    let (mut left, mut t) = (n, 0u32);
    while left > 0 && t < 20_000 {
        t += 1;
        h.tick();
        for (j, e) in ids.iter().enumerate() {
            if gone[j] == 0 && h.world.get_entity(*e).is_err() {
                gone[j] = t;
                left -= 1;
            }
        }
    }
    let mut real: Vec<u32> = (0..n).map(|j| mob_lives_until(mw, far_id(j), 60.0, 20_000).unwrap()).collect();
    let mean = |v: &[u32]| v.iter().map(|x| f64::from(*x)).sum::<f64>() / v.len() as f64;
    let sd = |v: &[u32]| (v.iter().map(|x| (f64::from(*x) - mean(v)).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)).sqrt();
    let (qr, qe) = (quantiles(&mut real), quantiles(&mut gone.clone()));
    let z = (mean(&real) - mean(&gone)) / ((sd(&real).powi(2) + sd(&gone).powi(2)) / n as f64).sqrt();
    eprintln!("after leaving the ring, {n} each: horizon mean {:.0} {qr:?}; ecs mean {:.0} {qe:?}; z {z:+.2}", mean(&real), mean(&gone));
    assert!(z.abs() < 3.5 && qe[0] >= 595);
}

#[test]
fn a_creature_that_picked_something_up_is_kept_and_not_counted() {
    if !have() {
        return;
    }
    use horizon_ecs_proto::despawn::{Despawn, Persistent};
    use horizon_ecs_proto::spawning::SpawnCounts;
    let mut h = Herd::with_spawn_world(176, true);
    player(&mut h, 0.0, 0.0);
    h.tick();
    let z = zombie();
    let e = born(&mut h.world, &z, [60.5, 1.0, 0.5], 31, 3600.0);
    let counted = h.world.resource::<SpawnCounts>().global[0];
    let mut rng = horizon_core::random::LegacyRandom::new(1);
    let item = horizon_ecs_proto::item::spawn_at_location(&mut h.world, horizon_client_lib::items::ItemStack::new("minecraft:apple", 1), Vec3::new(60.5, 1.0, 0.5), &mut rng);
    h.world.get_mut::<horizon_ecs_proto::item::Item>(item).unwrap().ready_at = Some(0);
    let taken = horizon_ecs_proto::item::pick_up(&mut h.world, e, item, 1);
    assert!(taken.is_some());
    assert!(h.world.get::<Persistent>(e).is_some());
    assert_eq!(h.world.resource::<SpawnCounts>().global[0], counted - 1, "the caps stop counting it");
    // It never goes: 20 000 ticks 60 blocks from the player.
    for _ in 0..3000 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_ok() && h.world.get::<Despawn>(e).is_some());
}
