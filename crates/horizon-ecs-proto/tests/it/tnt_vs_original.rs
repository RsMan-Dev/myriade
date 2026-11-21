//! The primed TNT against `PrimedTnt` (`tick_one_tnt`, `travel_tnt`): the fall and the bounce tick by tick, and the fuse that blows it on its
//! tick. No Horizon original is callable from here (`PrimedTnt` lives in `Game`), so the reference is the recurrence of the jar's tick,
//! transcribed. `HORIZON_TEST_RESOURCES` set.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::projectile::Blast;
use horizon_ecs_proto::tnt::Primed;
use horizon_ecs_proto::tnt::{spawn_primed, FUSE, POWER};

/// The first solid cell under `(x, z)` from `from` down, as its top.
fn floor_under(h: &Herd, x: i32, z: i32, from: i32) -> i32 {
    let mw = h.world.resource::<Level>().mw.0;
    (-64..=from).rev().find(|&y| !mw.facts.is_air(mw.world.block(x, y, z))).map_or(0, |y| y + 1)
}

/// `PrimedTnt`'s tick on the ground: gravity 0.04, the move stopped by the floor, where the vertical speed becomes `-speed * bounciness`
/// (`Entity.restituteMovementAfterCollisions`: the entity's bounciness is 0 and a dirt block's restitution too), the drag 0.98, and on the
/// ground y times -0.5. The height and the vertical speed after each tick.
fn reference(start_y: f64, floor: f64, ticks: usize) -> Vec<(f64, f64)> {
    let (mut y, mut vy, mut on_ground) = (start_y, 0.0_f64, false);
    let drag = f64::from(0.98_f32);
    (0..ticks)
        .map(|_| {
            vy -= 0.04;
            if y + vy <= floor {
                y = floor;
                vy = 0.0;
                on_ground = true;
            } else {
                y += vy;
                on_ground = false;
            }
            vy *= drag;
            if on_ground {
                vy *= -0.5;
            }
            (y, vy)
        })
        .collect()
}

/// A primed TNT dropped from above falls and bounces exactly as the jar's tick, every tick.
#[test]
fn a_fall_and_bounce_follow_the_jar_tick_by_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    let start = f64::from(top) + 10.0;
    let e = spawn_primed(&mut h.world, Vec3::new(0.5, start, 0.5), Vec3::ZERO, FUSE, POWER);
    for (t, &(y, vy)) in reference(start, f64::from(top), 60).iter().enumerate() {
        h.tick();
        assert_eq!(h.world.get::<Pos>(e).unwrap().0.y, y, "height after tick {}", t + 1);
        assert_eq!(h.world.get::<Vel>(e).unwrap().0.y, vy, "vertical speed after tick {}", t + 1);
    }
}

/// The fuse blows the TNT on its tick: it is alive on every tick before, and gone on the tick the fuse runs out.
#[test]
fn the_fuse_blows_it_on_its_tick_and_not_before() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    let e = spawn_primed(&mut h.world, Vec3::new(0.5, f64::from(top) + 1.0, 0.5), Vec3::ZERO, FUSE, POWER);
    let mut gone = None;
    for t in 1..=u64::try_from(FUSE + 5).unwrap() {
        h.tick();
        if gone.is_none() && h.world.get_entity(e).is_err() {
            gone = Some(t);
        }
    }
    assert_eq!(gone, Some(u64::try_from(FUSE).unwrap()), "the blast is on the fuse's tick");
}

/// A TNT that lands in water still burns its fuse on the same tick: the water does not hold its fuse back.
#[test]
fn a_tnt_in_water_blows_on_the_same_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    h.set_block(0, top, 0, "minecraft:water");
    h.tick();
    let e = spawn_primed(&mut h.world, Vec3::new(0.5, f64::from(top) + 1.0, 0.5), Vec3::ZERO, FUSE, POWER);
    let mut gone = None;
    for t in 1..=u64::try_from(FUSE + 5).unwrap() {
        h.tick();
        if gone.is_none() && h.world.get_entity(e).is_err() {
            gone = Some(t);
        }
    }
    assert_eq!(gone, Some(u64::try_from(FUSE).unwrap()), "the blast is on the fuse's tick, in water");
}

/// A TNT block an explosion destroys is primed at its centre, pushed by `PrimedTnt`'s constructor (0.02 along a random angle, 0.2 up) and
/// fused for `nextInt(80 / 4) + 80 / 8` ticks (10 to 29): the draws come after the blast's rays, which draw from the same random.
#[test]
fn a_tnt_block_a_blast_destroys_is_primed_with_a_push_and_a_short_fuse() {
    blast_primes(false);
}

/// The same, from the seed's very first draw. The jar's sequence (`RandomSource`) is not the ECS explosion random (`ExplosionRng`, a
/// xorshift): the draws cannot be the jar's, and the rays draw before the TNT (see `suivi/ecs-ecarts.md`).
#[test]
#[ignore = "ExplosionRng is a xorshift, not the Java sequence: the seeded push and fuse of a primed block cannot equal the jar's"]
fn a_tnt_block_a_blast_destroys_is_primed_with_its_seeded_push_and_short_fuse() {
    blast_primes(true);
}

fn blast_primes(exact: bool) {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let top = floor_under(&h, 0, 0, 40);
    h.set_block(3, top, 0, "minecraft:tnt");
    h.tick();
    h.world.insert_resource(horizon_ecs_proto::explosion::ExplosionRng(horizon_ecs_proto::cow::Rng(0x9e37_79b9)));
    horizon_ecs_proto::explosion::explode_tnt(&mut h.world, Blast { at: Vec3::new(3.5, f64::from(top), 0.5), radius: 4.0, fire: false, owner: None, source: None });
    let mut draws = horizon_ecs_proto::cow::Rng(0x9e37_79b9);
    let d = draws.next_f64() * f64::from(std::f32::consts::TAU);
    let want_vel = Vec3::new(-d.sin() * 0.02, f64::from(0.2_f32), -d.cos() * 0.02);
    let want_fuse = draws.next_int(20) + 10;
    let primed: Vec<(Entity, Vec3, Vec3)> = h.world.query::<(Entity, &Pos, &Vel, &Primed)>().iter(&h.world).map(|(e, p, v, _)| (e, p.0, v.0)).collect();
    assert_eq!(primed.len(), 1, "one TNT is primed by the blast");
    let (e, at, vel) = primed[0];
    assert_eq!(at, Vec3::new(3.5, f64::from(top), 0.5), "at the centre of the block");
    if exact {
        assert_eq!(vel, want_vel, "the constructor's push from the seed");
    } else {
        assert!(((vel.x * vel.x + vel.z * vel.z).sqrt() - 0.02).abs() < 1e-12 && vel.y == f64::from(0.2_f32), "the constructor's push: 0.02 along an angle, 0.2 up: {vel:?}");
    }
    let mut gone = None;
    for t in 1..=100u64 {
        h.tick();
        if gone.is_none() && h.world.get_entity(e).is_err() {
            gone = Some(t);
        }
    }
    if exact {
        assert_eq!(gone, Some(u64::try_from(want_fuse).unwrap()), "blown on the short fuse");
    } else {
        assert!(gone.is_some_and(|t| (10..=29).contains(&t)), "blown on a fuse of 10 to 29 ticks: {gone:?}");
    }
}
