//! The goat's jump boost and screaming goat, against the `Mob` (`HORIZON_SPECIES=minecraft:goat`, `HORIZON_TEST_RESOURCES` set). Written
//! without a run: the leap is the goal's own, so its length is measured on the Mob and on the ECS alike, with and without `jump_boost`.
//! The screaming draw is 2 percent of the goats born, and no other species draws it.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, effects::{self, Kind}, species::goat::Screaming};

/// The horizontal distance from the start to the farthest point of a leap, in blocks.
fn leap_reach(boost: Option<i32>) -> (f64, f64) {
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 6100);
    if let Some(amp) = boost {
        r.mobs[i].add_effect("minecraft:jump_boost", 600, amp);
        assert!(effects::add(&mut h.world, e, Kind::JumpBoost, amp, 600));
    }
    let (mut mob_reach, mut ecs_reach) = (0.0_f64, 0.0_f64);
    for _ in 0..400 {
        r.step(None);
        h.tick();
        let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
        mob_reach = mob_reach.max(((a.x - 0.5).powi(2) + (a.z - 0.5).powi(2)).sqrt());
        ecs_reach = ecs_reach.max(((b.x - 0.5).powi(2) + (b.z - 0.5).powi(2)).sqrt());
    }
    (mob_reach, ecs_reach)
}

#[test]
fn a_jump_boost_lengthens_the_goats_leap_as_the_mob_does() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let (mob0, ecs0) = leap_reach(None);
    let (mob1, ecs1) = leap_reach(Some(1));
    eprintln!("goat leap: plain mob {mob0:.3} ecs {ecs0:.3}; boost 2 mob {mob1:.3} ecs {ecs1:.3}");
    // The ECS leap matches the Mob's with and without the boost, and the boost makes the leap longer on both.
    assert!((mob0 - ecs0).abs() < 0.05, "{mob0} {ecs0}");
    assert!((mob1 - ecs1).abs() < 0.05, "{mob1} {ecs1}");
    assert!(mob1 > mob0 && ecs1 > ecs0, "{mob0} {mob1} {ecs0} {ecs1}");
}

#[test]
fn about_two_percent_of_the_goats_born_scream() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let (_r, mut h) = flat_pair();
    let sp = *species();
    const BORN: u64 = 5000;
    let screaming = (0..BORN).filter(|&seed| {
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9000 + seed);
        h.world.get::<Screaming>(e).is_some()
    });
    let n = screaming.count() as f64;
    let share = n / BORN as f64;
    eprintln!("screaming goats: {n} of {BORN} ({share:.4})");
    // The Mob's chance is 0.02: 5000 draws give 100 on average, a binomial deviation of about 10.
    assert!((share - 0.02).abs() < 0.008, "{share}");
}

#[test]
fn a_species_without_the_chance_never_screams() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    let (_r, mut h) = flat_pair();
    let sp = *species();
    for seed in 0..200 {
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 300 + seed);
        assert!(h.world.get::<Screaming>(e).is_none());
    }
}
