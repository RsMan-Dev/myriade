//! The goat's jump boost and screaming goat (`HORIZON_SPECIES=minecraft:goat`, `HORIZON_TEST_RESOURCES` set). The leap is the goal's own:
//! its length is measured with and without `jump_boost`.
//! The screaming draw is 2 percent of the goats born, and no other species draws it.
use crate::common;
use common::*;
use horizon_ecs_proto::{cow::*, goals::Entered, effects::{self, Kind}, species::goat::Screaming};

/// A goat on a 3 x 3 island in a ring of void, its ram cooling down (the long jump holds).
fn island() -> (Herd, bevy_ecs::entity::Entity) {
    let mut h = Herd::with_slab_awake(60, true, false);
    // The long jump leaps only to a spot it cannot reach on foot: on a flat ground every spot is walkable and the goat never leaps.
    // The goat stands on a 3 x 3 island in a ring of void two blocks wide.
    for x in -3..=3i32 {
        for z in -3..=3i32 {
            if x.abs().max(z.abs()) >= 2 {
                for y in -3..=0 {
                    h.set_block(x, y, z, "minecraft:air");
                }
            }
        }
    }
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 6100);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    // The long jump holds only while the ram cools down: the goat is given a cooldown of 5000 ticks.
    let now = horizon_ecs_proto::cow::now_of(&h.world, e);
    h.world.get_mut::<horizon_ecs_proto::behaviours::ram::RamCooldown>(e).unwrap().0 = now + 5000;
    horizon_ecs_proto::goals::wake(&mut h.world, e, &horizon_ecs_proto::behaviours::long_jump::MODULE);
    (h, e)
}

/// The horizontal length of the first leap (from take-off to landing, in blocks): a stroll never leaves the ground, so only the leap
/// counts. `None` when no leap came within the time given.
fn leap_length(boost: Option<i32>) -> Option<f64> {
    let (mut h, e) = island();
    if let Some(amp) = boost {
        assert!(effects::add(&mut h.world, e, Kind::JumpBoost, amp, 6000));
    }
    // (take-off point, length) of the first leap.
    let mut ecs: Option<([f64; 2], Option<f64>)> = None;
    for _ in 0..4000 {
        h.tick();
        let b = h.world.get::<Pos>(e).unwrap().0;
        match ecs {
            None if b.y > 1.05 => ecs = Some(([b.x, b.z], None)),
            Some((from, None)) if b.y <= 1.001 => ecs = Some((from, Some((b.x - from[0]).hypot(b.z - from[1])))),
            _ => {}
        }
        if ecs.is_some_and(|m| m.1.is_some()) {
            break;
        }
    }
    ecs.and_then(|m| m.1)
}

#[test]
fn a_jump_boost_lengthens_the_goats_leap() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let ecs0 = leap_length(None);
    let ecs1 = leap_length(Some(1));
    eprintln!("goat first leap: plain {ecs0:?}; boost 2 {ecs1:?}");
    // Not vacuous: a leap comes with and without the boost, and the boost makes it longer.
    let (ecs0, ecs1) = (ecs0.expect("ecs leap"), ecs1.expect("ecs boosted leap"));
    assert!(ecs1 > ecs0, "ECS: {ecs0} {ecs1}");
}

#[test]
fn about_two_percent_of_the_goats_born_scream() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let sp = *species();
    const BORN: u64 = 5000;
    let screaming = (0..BORN).filter(|&seed| {
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9000 + seed);
        h.world.get::<Screaming>(e).is_some()
    });
    let n = screaming.count() as f64;
    let share = n / BORN as f64;
    eprintln!("screaming goats: {n} of {BORN} ({share:.4})");
    // The chance is 0.02: 5000 draws give 100 on average, a binomial deviation of about 10.
    assert!((share - 0.02).abs() < 0.008, "{share}");
}

#[test]
fn a_species_without_the_chance_never_screams() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let sp = *species();
    for seed in 0..200 {
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 300 + seed);
        assert!(h.world.get::<Screaming>(e).is_none());
    }
}

/// A player who holds food keeps the goat from leaping (`GoatAi.longJumpActivity` wants no tempting player); when the item is put away the
/// activity comes back by itself (the ECS wakes it by the event of the player, no poll) and the goat leaps.
/// The tick at which the player puts the food away: past the longest cooldown of the long jump (1200).
const RELEASE: u32 = 1400;

#[test]
fn a_goat_leaps_again_once_the_player_puts_the_food_away() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let (mut h, e) = island();
    let (item, _) = tempt_item();
    let mut ecs_leap = None;
    for t in 0..1700u32 {
        let held = t < RELEASE;
        h.world.resource_mut::<horizon_ecs_proto::goals::Online>().set(horizon_ecs_proto::goals::Player { id: 1, x: 1.5, y: 1.0, z: 0.5, main: if held { item } else { 0 }, off: 0, ..Default::default() });
        h.tick();
        if ecs_leap.is_none() && h.world.get::<Pos>(e).unwrap().0.y > 1.05 {
            ecs_leap = Some(t);
        }
    }
    eprintln!("goat leaps after the food is put away: ecs {ecs_leap:?} (food held until tick {RELEASE})");
    let c = ecs_leap.expect("the ECS goat never leaps");
    // The ECS cooldown of the long jump ran out while the goat was tempted: the leap that follows within 150 ticks is the wake by the
    // player's event (a goal asked again by a poll would have missed or delayed it).
    assert!((RELEASE..RELEASE + 150).contains(&c), "ecs {c}, food put away at {RELEASE}");
}
