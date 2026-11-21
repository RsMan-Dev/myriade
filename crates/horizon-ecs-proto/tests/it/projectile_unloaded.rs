//! A fireball, a wither skull or a wind charge (`AbstractHurtingProjectile`) has no age limit: it is discarded on the tick it enters a
//! chunk that is not loaded (`tick`: `hasChunkAt` of its block position), and lives on while it stays in loaded chunks.
use bevy_ecs::{entity_disabling::Disabled, prelude::*};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::{Herd, Pos},
    projectile::{launch, Flight, Projectile},
    targeting::Who,
};

const AT: Vec3 = Vec3 { x: 0.5, y: 60.0, z: 0.5 };

fn loaded(h: &Herd, p: Vec3) -> bool {
    h.chunk_simulated((p.x / 16.0).floor() as i32, (p.z / 16.0).floor() as i32)
}

#[test]
fn a_skull_is_gone_on_the_tick_it_enters_an_unloaded_chunk() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(0.5, 0.5)]);
    let e = launch(&mut h.world, Flight::WITHER_SKULL, AT, Vec3::new(2.0, 0.0, 0.0), None);
    let mut gone = false;
    for _ in 0..2000 {
        let before = h.world.get::<Pos>(e).unwrap().0;
        h.tick();
        match h.world.get::<Pos>(e).map(|p| p.0) {
            Some(now) => assert!(loaded(&h, now), "it stayed alive in an unloaded chunk at {now:?}"),
            None => {
                gone = true;
                assert!(loaded(&h, before), "it was gone before it left the loaded chunks, at {before:?}");
                break;
            }
        }
    }
    assert!(gone, "it never left the loaded chunks");
}

#[test]
fn a_skull_in_loaded_chunks_has_no_age_limit() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(0.5, 0.5)]);
    let e = launch(&mut h.world, Flight::WITHER_SKULL, AT, Vec3::ZERO, None);
    for _ in 0..1300 {
        h.tick();
    }
    assert!(h.world.get_entity(e).is_ok(), "it expired at 1 200 ticks while its chunks were loaded");
    assert_eq!(h.world.get::<Projectile>(e).unwrap().age, 1300);
}

#[test]
fn a_wither_salvo_does_not_pile_up_skulls() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(0.5, 0.5)]);
    for _ in 0..1200 {
        launch(&mut h.world, Flight::WITHER_SKULL, AT, Vec3::new(1.0, 0.0, 0.0), None);
        h.tick();
    }
    let alive = h.world.query_filtered::<Entity, (With<Projectile>, Allow<Disabled>)>().iter(&h.world).count();
    assert!(alive < 200, "{alive} skulls alive after 1 200 ticks of salvos: they pile up");
}

#[test]
fn a_skull_launched_outside_the_loaded_chunks_is_discarded() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(400.5, 400.5)]);
    let e = launch(&mut h.world, Flight::WITHER_SKULL, AT, Vec3::new(0.0, 0.0, 0.0), None);
    assert!(h.world.get_entity(e).is_err(), "it waits where no player is near");
}

#[test]
fn a_skull_in_flight_is_discarded_when_its_chunks_stop_ticking() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(0.5, 0.5)]);
    let e = launch(&mut h.world, Flight::WITHER_SKULL, AT, Vec3::new(0.0, 0.0, 0.0), None);
    h.tick();
    h.set_players(vec![]);
    h.tick();
    assert!(h.world.get_entity(e).is_err(), "it was frozen instead of discarded");
}

#[test]
fn the_skulls_in_flight_go_when_the_wither_that_shot_them_dies() {
    if !super::common::have_data() {
        return;
    }
    let mut h = Herd::with_slab(40, true, false);
    h.set_players(vec![(0.5, 0.5)]);
    let wither = h.world.spawn_empty().id();
    let shot = launch(&mut h.world, Flight::WITHER_SKULL, AT, Vec3::ZERO, Some(Who::Entity(wither)));
    let other = launch(&mut h.world, Flight::WITHER_SKULL, AT, Vec3::ZERO, None);
    h.tick();
    h.world.despawn(wither);
    h.tick();
    assert!(h.world.get_entity(shot).is_err(), "a skull of a dead wither still flies");
    assert!(h.world.get_entity(other).is_ok(), "a skull with no shooter went with another wither");
}
