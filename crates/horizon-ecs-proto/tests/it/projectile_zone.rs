//! A projectile is ticked only in a chunk that ticks (`ServerLevel.tick`: `isPositionEntityTicking`): with no player near it stands where
//! it is, for no instruction, and it flies on from there when a player comes back.
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::{Herd, Pos},
    profile,
    projectile::{launch, Flight, Projectile},
};

const AT: Vec3 = Vec3 { x: 0.5, y: 60.0, z: 0.5 };
const V: Vec3 = Vec3 { x: 0.3, y: 0.2, z: 0.1 };

fn pos(h: &Herd, e: Entity) -> Vec3 {
    h.world.get::<Pos>(e).unwrap().0
}
fn age(h: &Herd, e: Entity) -> u32 {
    h.world.get::<Projectile>(e).unwrap().age
}

/// A herd with `n` projectiles flown for 10 ticks beside a player, then left alone: the instructions the flight system takes in 100 ticks.
fn waiting(n: usize) -> (Herd, Entity, u64) {
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(0.5, 0.5)]);
    let e = launch(&mut h.world, Flight::BREEZE_WIND_CHARGE, AT, V, None);
    for _ in 1..n {
        launch(&mut h.world, Flight::BREEZE_WIND_CHARGE, AT, V, None);
    }
    for _ in 0..10 {
        h.tick();
    }
    h.set_players(vec![]);
    h.tick();
    let shared = profile::instrument(&mut h);
    profile::reset(&shared);
    for _ in 0..100 {
        h.tick();
    }
    let cost = shared.lock().unwrap().rows.iter().filter(|r| r.name.contains("projectile::fly")).map(|r| r.run + r.apply).sum();
    (h, e, cost)
}

#[test]
fn a_projectile_waits_without_a_player_and_flies_on_from_where_it_stopped() {
    if !super::common::have_data() {
        return;
    }
    // The control: the same projectile, ticked 20 times with the whole world awake.
    let mut c = Herd::with_slab_awake(40, true, false);
    let ec = launch(&mut c.world, Flight::BREEZE_WIND_CHARGE, AT, V, None);
    for _ in 0..20 {
        c.tick();
    }
    let (mut h, e, one) = waiting(1);
    assert_eq!(age(&h, e), 10, "it aged with no player near");
    // 300 frozen projectiles cost what one does: the instruction counter itself has a fixed cost per system and tick, nothing per entity.
    let (_, _, many) = waiting(300);
    assert_eq!(many, one, "a frozen projectile costs the flight system instructions");
    let stopped = pos(&h, e);
    for _ in 0..50 {
        h.tick();
    }
    assert_eq!(pos(&h, e), stopped, "it moved with no player near");
    h.set_players(vec![(0.5, 0.5)]);
    for _ in 0..10 {
        h.tick();
    }
    assert_eq!(age(&h, e), 20);
    assert_eq!(pos(&h, e), pos(&c, ec), "it did not resume from where it stopped");
}

#[test]
fn a_projectile_launched_far_from_every_player_does_not_fly_until_one_comes() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(400.5, 400.5)]);
    let e = launch(&mut h.world, Flight::BREEZE_WIND_CHARGE, AT, V, None);
    for _ in 0..30 {
        h.tick();
    }
    assert_eq!((pos(&h, e), age(&h, e)), (AT, 0));
    h.set_players(vec![(0.5, 0.5)]);
    for _ in 0..3 {
        h.tick();
    }
    assert_eq!(age(&h, e), 3);
}

/// An arrow stuck in a block does not age while frozen: its life of 1 200 ticks resumes where it stopped.
#[test]
fn a_stuck_arrow_keeps_its_life_while_frozen() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(0.5, 0.5)]);
    let e = launch(&mut h.world, Flight::ARROW, Vec3::new(0.5, 3.0, 0.5), Vec3::new(0.0, -1.0, 0.0), None);
    for _ in 0..10 {
        h.tick();
    }
    let Some(_) = h.world.get::<horizon_ecs_proto::projectile::Stuck>(e) else { return };
    h.set_players(vec![]);
    for _ in 0..2000 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_ok(), "a frozen arrow expired while nobody simulated it");
    h.set_players(vec![(0.5, 0.5)]);
    for _ in 0..1100 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_ok(), "it expired before its 1 200 ticks of its own time");
    for _ in 0..200 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_err(), "it never expired");
}
