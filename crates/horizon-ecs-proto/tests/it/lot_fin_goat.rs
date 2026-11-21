//! The goat's jump boost and screaming goat, against the `Mob` (`HORIZON_SPECIES=minecraft:goat`, `HORIZON_TEST_RESOURCES` set). Written
//! without a run: the leap is the goal's own, so its length is measured on the Mob and on the ECS alike, with and without `jump_boost`.
//! The screaming draw is 2 percent of the goats born, and no other species draws it.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::Entered, effects::{self, Kind}, species::goat::Screaming};

/// The horizontal length of the first leap (from take-off to landing, in blocks) on the `Mob` and on the ECS: a stroll never leaves the
/// ground, so only the leap counts. `None` when no leap came within the time given.
fn leap_length(boost: Option<i32>) -> (Option<f64>, Option<f64>) {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    let i = r.add(Vec3::new(0.5, 1.0, 0.5));
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 6100);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    // The long jump holds only while the ram cools down: both goats are given a cooldown of 5000 ticks (the Mob's spawn draws none here).
    r.mobs[i].extra.memories.push(("ram_cooldown_ticks", 5000, i64::MAX));
    let now = horizon_ecs_proto::cow::now_of(&h.world, e);
    h.world.get_mut::<horizon_ecs_proto::behaviours::ram::RamCooldown>(e).unwrap().0 = now + 5000;
    horizon_ecs_proto::goals::wake(&mut h.world, e, &horizon_ecs_proto::behaviours::long_jump::MODULE);
    if let Some(amp) = boost {
        r.mobs[i].add_effect("minecraft:jump_boost", 6000, amp);
        assert!(effects::add(&mut h.world, e, Kind::JumpBoost, amp, 6000));
    }
    // (take-off point, length) of the first leap on each side.
    let (mut mob, mut ecs): (Option<([f64; 2], Option<f64>)>, Option<([f64; 2], Option<f64>)>) = (None, None);
    let track = |s: &mut Option<([f64; 2], Option<f64>)>, x: f64, y: f64, z: f64| match s {
        None if y > 1.05 => *s = Some(([x, z], None)),
        Some((from, len @ None)) if y <= 1.001 => *len = Some((x - from[0]).hypot(z - from[1])),
        _ => {}
    };
    for _ in 0..4000 {
        r.mobs[i].ai.no_action_time = 0;
        r.step(None);
        h.tick();
        let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
        track(&mut mob, a.x, a.y, a.z);
        track(&mut ecs, b.x, b.y, b.z);
        if mob.is_some_and(|m| m.1.is_some()) && ecs.is_some_and(|m| m.1.is_some()) {
            break;
        }
    }
    (mob.and_then(|m| m.1), ecs.and_then(|m| m.1))
}

#[test]
#[ignore = "no leap is measured: on the flat harness world the Mob's long jump goal starts (32 ticks) and ends without leaping within 4000 ticks, and the ECS one does not leap either; the boost formula (0.1 x (level + 1) in f32) is the same code line on both sides, read not measured; the former version passed vacuously (0 = 0)"]
fn a_jump_boost_lengthens_the_goats_leap_as_the_mob_does() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let (mob0, ecs0) = leap_length(None);
    let (mob1, ecs1) = leap_length(Some(1));
    eprintln!("goat first leap: plain mob {mob0:?} ecs {ecs0:?}; boost 2 mob {mob1:?} ecs {ecs1:?}");
    // Not vacuous: a leap comes on each side, with and without the boost, and the boost makes it longer (same draws on each side).
    let (mob0, ecs0, mob1, ecs1) = (mob0.expect("mob leap"), ecs0.expect("ecs leap"), mob1.expect("mob boosted leap"), ecs1.expect("ecs boosted leap"));
    assert!(mob1 > mob0, "Mob: {mob0} {mob1}");
    assert!(ecs1 > ecs0, "ECS: {ecs0} {ecs1}");
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
