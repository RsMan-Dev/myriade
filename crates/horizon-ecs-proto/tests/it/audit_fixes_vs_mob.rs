//! Audit fixes: the cost of the state chains (baby that waits for an adult, turtle with an egg) measured in instructions per
//! creature and tick (`--ignored --nocapture`), and the behaviour kept (the start ticks of the baby and the turtle's search).
use crate::common;
use common::*;
use horizon_ecs_proto::{behaviours::*, cow::*, goals::*};

fn counter() -> Option<horizon_ecs_proto::instr::Counter> {
    horizon_ecs_proto::instr::Counter::new()
}
/// 500 baby armadillos with no adult within reach, spaced 6 blocks: instructions per baby and tick.
#[test]
#[ignore]
fn cost_of_a_baby_that_waits_for_an_adult() {
    if !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:armadillo") {
        return;
    }
    let mut h = Herd::new(500, 6.0, false, false, 3);
    let es: Vec<_> = h.world.query::<bevy_ecs::entity::Entity>().iter(&h.world).collect();
    let babies: Vec<_> = es.into_iter().filter(|e| h.world.get::<baby_follow_adult::Follow>(*e).is_some()).collect();
    for e in &babies {
        follow_parent::make_baby(&mut h.world, *e, 1_000_000);
        h.world.get_mut::<ActiveUntil>(*e).unwrap().0 = u64::MAX;
        wake(&mut h.world, *e, &baby_follow_adult::MODULE);
    }
    for _ in 0..200 {
        h.tick();
    }
    let Some(c) = counter() else { return };
    let a = c.read();
    for _ in 0..400 {
        h.tick();
    }
    eprintln!("MEASURE baby waiting: {} babies, {:.0} instr per tick, {:.1} per baby-tick", babies.len(), (c.read() - a) as f64 / 400.0, (c.read() - a) as f64 / 400.0 / babies.len() as f64);
}
/// 500 turtles with an egg near home and no sand within reach: instructions per turtle and tick.
#[test]
#[ignore]
fn cost_of_a_turtle_with_an_egg_and_no_sand() {
    if !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:turtle") {
        return;
    }
    let mut h = Herd::new(500, 6.0, false, false, 3);
    let all: Vec<_> = h.world.query::<bevy_ecs::entity::Entity>().iter(&h.world).collect();
    let es: Vec<_> = all.into_iter().filter(|e| h.world.get::<ActiveUntil>(*e).is_some()).collect();
    for e in &es {
        turtle_state::update(&mut h.world, *e, |t| t.has_egg = true);
        h.world.get_mut::<ActiveUntil>(*e).unwrap().0 = u64::MAX;
        wake(&mut h.world, *e, &turtle_lay_egg::MODULE);
    }
    for _ in 0..200 {
        h.tick();
    }
    let Some(c) = counter() else { return };
    let a = c.read();
    for _ in 0..800 {
        h.tick();
    }
    eprintln!("MEASURE turtle egg: {} turtles, {:.0} instr per tick, {:.1} per turtle-tick", es.len(), (c.read() - a) as f64 / 800.0, (c.read() - a) as f64 / 800.0 / es.len() as f64);
}

/// A baby with no adult near asks slowly (every 20 ticks, no more every 2): an adult that arrives at 8 blocks is followed within 25 ticks.
#[test]
fn a_baby_follows_an_adult_that_arrives_within_twenty_five_ticks() {
    if !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:armadillo") {
        return;
    }
    let mut h = Herd::new(2, 40.0, false, false, 5);
    let es: Vec<_> = h.world.query::<bevy_ecs::entity::Entity>().iter(&h.world).filter(|e| h.world.get::<baby_follow_adult::Follow>(*e).is_some()).collect();
    let (baby, adult) = (es[0], es[1]);
    for e in [baby, adult] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    }
    follow_parent::make_baby(&mut h.world, baby, 1_000_000);
    wake(&mut h.world, baby, &baby_follow_adult::MODULE);
    for _ in 0..100 {
        h.tick();
    }
    assert!(h.world.get::<baby_follow_adult::FollowsAdult>(baby).is_none(), "witness: no adult near, no following");
    let p = h.world.get::<Pos>(baby).unwrap().0;
    h.teleport(adult, horizon_core::geometry::Vec3::new(p.x + 8.0, p.y, p.z));
    let mut started = None;
    for t in 0..40 {
        h.tick();
        if started.is_none() && h.world.get::<baby_follow_adult::FollowsAdult>(baby).is_some() {
            started = Some(t);
        }
    }
    assert!(started.is_some_and(|t| t <= 25), "the baby follows the adult within 25 ticks of its arrival: {started:?}");
}

/// A wild wolf that a player hit stays angry after it lost the target (`neutral_anger`, 400 to 780 ticks): it takes the player
/// again by its angry-player target; without the module it never did (the predicate `neutral:angry` was never true).
#[test]
fn a_wolf_hit_by_a_player_takes_him_again_while_it_is_angry() {
    if !have_data() || !matches!(std::env::var("HORIZON_SPECIES").as_deref(), Ok("minecraft:wolf" | "minecraft:zombified_piglin")) {
        return;
    }
    let mut h = Herd::new(1, 40.0, false, false, 9);
    let wolf = h.world.query::<bevy_ecs::entity::Entity>().iter(&h.world).find(|e| h.world.get::<ActiveUntil>(*e).is_some()).unwrap();
    h.world.get_mut::<ActiveUntil>(wolf).unwrap().0 = u64::MAX;
    h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
    let p = h.world.get::<Pos>(wolf).unwrap().0;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x + 6.0, y: p.y, z: p.z, main: 0, off: 0 });
    for _ in 0..5 {
        h.tick();
    }
    assert!(h.world.get::<horizon_ecs_proto::targeting::Target>(wolf).is_none(), "witness: a calm wild wolf takes no target");
    h.hit_by(wolf, 1.0, 1);
    for _ in 0..10 {
        h.tick();
    }
    assert!(h.world.get::<horizon_ecs_proto::targeting::Target>(wolf).is_some(), "witness: the hit gives the wolf its target");
    // The player runs away: the target ends by its range (a real loss: the goal that held it ends too).
    let q = h.world.get::<Pos>(wolf).unwrap().0;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: q.x + 100.0, y: q.y, z: q.z, main: 0, off: 0 });
    for _ in 0..40 {
        h.tick();
    }
    assert!(h.world.get::<horizon_ecs_proto::targeting::Target>(wolf).is_none(), "witness: the target is lost out of range");
    let mut again = false;
    for _ in 0..150 {
        h.tick();
        again |= h.world.get::<horizon_ecs_proto::targeting::Target>(wolf).is_some();
        let q = h.world.get::<Pos>(wolf).unwrap().0;
        h.world.resource_mut::<Online>().set(Player { id: 1, x: q.x + 6.0, y: q.y, z: q.z, main: 0, off: 0 });
    }
    assert!(again, "the angry wolf takes the player again");
}
