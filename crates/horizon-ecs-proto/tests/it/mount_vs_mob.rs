//! The horse of the frame, by configuration, against the `Mob` horse in the same scene (`HORIZON_SPECIES=minecraft:horse`,
//! `HORIZON_TEST_RESOURCES` set): the taming by riding (the law of the temper), the rider's steering with the inputs imposed, and the
//! dismount.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::equine::Rider;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::mount::*;
use horizon_ecs_proto::pilot::{self, PilotInput};
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::Who;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:horse"])
}
fn input(forward: f32, strafe: f32, yaw: f32, jump: f32) -> Rider {
    Rider { strafe, forward, yaw, pitch: 0.0, jump, steer: false, sprint: false, jumping: false, sneak: false, client: false }
}

/// A wild horse ridden bolts, and each tick it draws one in 50: with a temper of 50 it is tamed half the time and throws its rider off the
/// other half (and rears); both sides follow the law.
#[test]
fn a_ridden_wild_horse_is_tamed_by_the_law_of_its_temper_or_throws_the_rider() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let (mut tamed, mut thrown) = (0, 0);
    for k in 0..200 {
        let e = spawn_species(&mut h.world, species(), 0.5 + (k % 40) as f64, 0.5 + (k / 40) as f64 * 3.0, 31 + k as u64);
        h.world.resource_mut::<Entered>().0.push(e);
        for _ in 0..5 {
            h.tick();
        }
        h.world.get_mut::<Mount>(e).unwrap().temper = 50;
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        assert!(interact(&mut h.world, e, &Interaction { player: 1, item: 0 }), "the player did not mount");
        for _ in 0..800 {
            h.tick();
            if h.world.get::<Owner>(e).unwrap().0.is_some() {
                tamed += 1;
                break;
            }
            if h.world.get::<Mount>(e).unwrap().rider.is_none() {
                thrown += 1;
                break;
            }
        }
        let _ = h.world.get_mut::<Mount>(e).map(|mut m| m.rider = None);
        h.world.entity_mut(e).despawn();
    }
    eprintln!("200 rides at temper 50: tamed {tamed}, thrown {thrown}");
    assert!(tamed + thrown >= 190, "{tamed} {thrown}");
    assert!((70..=130).contains(&tamed), "a half: {tamed}");
}

/// The rider's steering: with forward and a turn imposed, the horse walks the same path, and a jump lifts it.
#[test]
fn the_rider_steers_the_horse_as_it_steers_the_original() {
    if skip() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let i = r.add_id(Vec3::new(0.5, 1.0, 0.5), 31_337);
    horizon_client_lib::mobs::tame::set_tame(&mut r.mobs[i], true);
    r.mobs[i].extra.vector[0] = 0.7;
    r.mobs[i].extra.set_saddled(true);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    horizon_ecs_proto::tame::set_owner(&mut h.world, e, Who::Player(1));
    h.world.get_mut::<Mount>(e).unwrap().saddled = true;
    assert!(ride(&mut h.world, e, Who::Player(1)));
    eprintln!("speed: mob {} ecs {}", r.mobs[i].movement_speed(), h.world.get::<Body>(e).unwrap().speed);
    let view = player_view(Vec3::new(0.5, 1.0, 0.5), None);
    let mut worst = 0.0_f64;
    for t in 1..=120 {
        // Forward for 60 ticks, then a quarter turn and a jump at 80.
        let yaw = if t < 60 { 0.0 } else { 90.0 };
        let jump = if t == 80 { 1.0 } else { 0.0 };
        r.mobs[i].rider = Some(input(1.0, 0.0, yaw, jump));
        pilot::set(&mut h.world, e, PilotInput { who: Who::Player(1), forward: 1.0, strafe: 0.0, yaw: yaw, pitch: 0.0, jump: jump, sprint: false, sneak: false, steer: false, keys: [false, false, (1.0) > 0.0, false] });
        r.step(Some(&view));
        h.tick();
        let (m, p) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
        let d = ((m.x - p.x).powi(2) + (m.y - p.y).powi(2) + (m.z - p.z).powi(2)).sqrt();
        worst = worst.max(d);
        if t % 30 == 0 {
            eprintln!("t{t}: mob ({:.2},{:.2},{:.2}) ecs ({:.2},{:.2},{:.2})", m.x, m.y, m.z, p.x, p.y, p.z);
        }
    }
    eprintln!("worst gap {worst:.3}");
    assert!(worst < 0.5, "the horse does not go where the original goes: {worst}");
}

/// Dismounting frees the mount (no more occupation, no rider), sets the player down beside it and raises the event.
#[test]
fn a_dismount_frees_the_mount_and_the_player() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    assert!(ride(&mut h.world, e, Who::Player(1)));
    assert!(is_occupied(&h.world, e, OCC_MOUNTED));
    assert!(!ride(&mut h.world, e, Who::Player(2)), "two riders on one horse");
    h.tick();
    assert_eq!(dismount(&mut h.world, e), Some(Who::Player(1)));
    assert!(!is_occupied(&h.world, e, OCC_MOUNTED));
    assert!(h.world.get::<Mount>(e).unwrap().rider.is_none());
    h.tick();
    assert!(h.world.resource::<Events>().dismounted.iter().any(|m| m == e));
}

/// The same bolting on the `Mob` side, in the same law: 100 ridden wild horses with a temper of 50, tamed or throwing the rider about half the time.
#[test]
fn the_original_horse_follows_the_same_law() {
    if skip() {
        return;
    }
    let mut r = MobRef::new(60, false);
    for k in 0..100 {
        let i = r.add_id(Vec3::new(-50.0 + 4.0 * (k % 25) as f64 + 0.5, 1.0, -40.0 + 8.0 * (k / 25) as f64 + 0.5), 1000 + 977 * k as u32);
        r.mobs[i].extra.equine_temper = 50;
        r.mobs[i].rider = Some(input(0.0, 0.0, 0.0, 0.0));
    }
    let view = player_view(Vec3::new(0.0, 1.0, 20.0), None);
    // The first outcome of each horse (the host would take the rider off after a refusal; here it stays and the horse draws again).
    let mut first: Vec<Option<bool>> = vec![None; r.mobs.len()];
    let mut seen = 0;
    for _ in 0..800 {
        r.step(Some(&view));
        for k in 0..r.mobs.len() {
            if first[k].is_none() && r.mobs[k].extra.tame {
                first[k] = Some(true);
            }
        }
        for e in &r.events[seen.min(r.events.len())..] {
            if let horizon_client_lib::mobs::MobEvent::Buck { mob } = e
                && let Some(k) = r.mobs.iter().position(|m| m.id == *mob)
                && first[k].is_none()
            {
                first[k] = Some(false);
            }
        }
        seen = r.events.len();
    }
    let tamed = first.iter().filter(|f| **f == Some(true)).count();
    let bucked = first.iter().filter(|f| **f == Some(false)).count();
    eprintln!("100 ridden Mob horses at temper 50, first outcome: tamed {tamed}, bucked {bucked}");
    assert!(tamed + bucked >= 80 && (30..=70).contains(&tamed), "{tamed} {bucked}");
}

/// The law of the temper, whole: a wild horse that keeps its rider is refused after each failed draw and its temper goes up by 5; it is tamed with
/// the chance `temper / 100` at each draw, and the rider is put back on after each throw (the `Mob` keeps its rider; the ECS horse is mounted
/// again at once). The number of refusals before the taming has the exact law below (p(r) = (5r/100) * product over i < r of (1 - 5i/100)); the
/// two sides, 300 horses each, are measured against it and against each other, and the temper is 5 per refusal on both.
#[test]
fn the_whole_law_of_the_temper_is_the_same_on_both_sides() {
    if skip() {
        return;
    }
    // The exact law and its mean.
    let mut p = vec![0.0_f64; 22];
    let mut alive = 1.0;
    for r in 1..21 {
        let t = 5.0 * f64::from(r);
        p[r as usize] = alive * (t.min(100.0) / 100.0);
        alive *= 1.0 - t.min(100.0) / 100.0;
    }
    let mean: f64 = p.iter().enumerate().map(|(r, q)| r as f64 * q).sum();
    let var: f64 = p.iter().enumerate().map(|(r, q)| q * (r as f64 - mean).powi(2)).sum();
    // The ECS side.
    let mut h = Herd::with_slab_awake(60, true, false);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    let mut ecs = Vec::new();
    for k in 0..300 {
        let e = spawn_species(&mut h.world, species(), -39.5 + (k % 20) as f64 * 4.0, -29.5 + (k / 20) as f64 * 4.0, 77 + k as u64);
        h.world.resource_mut::<Entered>().0.push(e);
        for _ in 0..5 {
            h.tick();
        }
        assert!(ride(&mut h.world, e, Who::Player(1)));
        let mut refusals = 0;
        for _ in 0..20_000 {
            h.tick();
            if h.world.get::<Owner>(e).unwrap().0.is_some() {
                break;
            }
            if h.world.get::<Mount>(e).unwrap().rider.is_none() {
                refusals += 1;
                assert_eq!(h.world.get::<Mount>(e).unwrap().temper, 5 * refusals, "the temper after each refusal");
                assert!(ride(&mut h.world, e, Who::Player(1)));
            }
        }
        ecs.push(refusals);
        h.world.entity_mut(e).despawn();
    }
    // The `Mob` side.
    let mut r = MobRef::new(60, false);
    for k in 0..300 {
        let i = r.add_id(Vec3::new(-50.0 + 4.0 * (k % 25) as f64 + 0.5, 1.0, -50.0 + 8.0 * (k / 25) as f64 + 0.5), 1000 + 977 * k as u32);
        r.mobs[i].rider = Some(input(0.0, 0.0, 0.0, 0.0));
    }
    let view = player_view(Vec3::new(0.0, 1.0, 20.0), None);
    let mut bucks = vec![0i32; 300];
    let mut seen = 0;
    for _ in 0..3000 {
        r.step(Some(&view));
        for e in &r.events[seen.min(r.events.len())..] {
            if let horizon_client_lib::mobs::MobEvent::Buck { mob } = e
                && let Some(k) = r.mobs.iter().position(|m| m.id == *mob)
                && !r.mobs[k].extra.tame
            {
                bucks[k] += 1;
                assert_eq!(r.mobs[k].extra.equine_temper, 5 * bucks[k], "the temper after each refusal (Mob)");
            }
        }
        seen = r.events.len();
    }
    let mob: Vec<i32> = (0..300).map(|k| if r.mobs[k].extra.tame { bucks[k] } else { -1 }).collect();
    let stat = |v: &[i32]| {
        let ok: Vec<f64> = v.iter().filter(|&&x| x >= 0).map(|&x| f64::from(x)).collect();
        (ok.iter().sum::<f64>() / ok.len() as f64, ok.len())
    };
    let (em, en) = stat(&ecs);
    let (mm, mn) = stat(&mob);
    let se = (var / 300.0).sqrt();
    eprintln!("refusals before the taming: exact mean {mean:.2} (sd of a mean of 300: {se:.2}); ecs {em:.2} ({en} tamed), mob {mm:.2} ({mn} tamed)");
    let hist = |v: &[i32]| (0..=14).map(|r| v.iter().filter(|&&x| x == r).count()).collect::<Vec<_>>();
    eprintln!("histogram 0..14: ecs {:?}\n                  mob {:?}", hist(&ecs), hist(&mob));
    assert!(en == 300 && mn >= 290, "{en} {mn}");
    assert!((em - mean).abs() < 3.5 * se && (mm - mean).abs() < 3.5 * se, "ecs {em}, mob {mm}, exact {mean}");
    assert!((em - mm).abs() < 4.5 * se, "ecs {em}, mob {mm}");
}
