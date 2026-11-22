//! The steering of the mounts through the bridge: the harnessed happy ghast flies along its rider's look, a llama is boarded but never
//! steered, the nautilus gives its rider the breath on a deadline and plays its "dash ready" sound. Written, not run.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_client_lib::mobs::MobWorld;
use horizon_ecs_proto::behaviours::harness_worn::Harness;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cow::Pos;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::host_out_misc::MiscEvent;
use horizon_ecs_proto::mount::{self, Mount};
use horizon_ecs_proto::targeting::Who;

fn with_mw(kind: &str) -> (&'static MobWorld<'static>, EcsWorld, Entity) {
    have_data();
    let setup = HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 };
    let (mw, mut h) = awake_host(&setup);
    let e = h.spawn(mw, kind, [0.5, 64.0, 0.5], None).expect("a known species");
    (mw, h, e)
}

fn ride(h: &mut EcsWorld, e: Entity) {
    assert!(mount::ride(h.world_mut(), e, Who::Player(1)), "the first seat is free");
}

/// The ghast takes a rider's forward input only while it wears its harness.
#[test]
fn a_harnessed_happy_ghast_flies_along_its_riders_look() {
    let (mw, mut h, ghast) = with_mw("minecraft:happy_ghast");
    ride(&mut h, ghast);
    assert!(!h.player_steers(ghast), "no harness, no control");
    h.world_mut().entity_mut(ghast).insert(Harness { item: "minecraft:white_harness" });
    assert!(h.player_steers(ghast), "a harnessed adult with no still timeout is steered");
    let start = h.world().get::<Pos>(ghast).unwrap().0;
    for _ in 0..30 {
        h.ride_input(ghast, [0.0, 1.0], 0.0, false, false, false, 0.0, 0.0);
        h.step(mw);
    }
    // The ghast's own float goal still runs under a player (`updateControlFlags` leaves its goals on): the heading is not asserted, only
    // the rider's forward input (`getRiddenInput`) in the drive and the flight it adds.
    assert!(h.world().get::<horizon_ecs_proto::cow::Drive>(ghast).unwrap().zza > 0.1, "the rider's forward input reaches the drive");
    let end = h.world().get::<Pos>(ghast).unwrap().0;
    assert!((end.x - start.x).hypot(end.z - start.z) > 1.0, "30 ticks of forward input flew it: {start:?} -> {end:?}");
}

/// A llama carries a player but is neither steered nor jumpable, saddle or not.
#[test]
fn a_llama_is_boarded_but_never_steered() {
    let (_, mut h, llama) = with_mw("minecraft:llama");
    ride(&mut h, llama);
    assert!(h.world().get::<Mount>(llama).is_some_and(|m| m.rider == Some(Who::Player(1))));
    assert!(!h.player_steers(llama), "a llama has no saddle: nobody controls it");
    assert!(!h.jumpable(llama), "no jump bar on a llama");
}

/// The nautilus breath goes to the rider at once and again every 40 ticks (60 ticks long, ambient), and the dash's cooldown ends with
/// the "ready" sound.
#[test]
fn a_nautilus_gives_its_rider_the_breath_and_the_dash_ready_sound() {
    let (mw, mut h, nautilus) = with_mw("minecraft:nautilus");
    h.world_mut().get_mut::<Mount>(nautilus).unwrap().saddled = true;
    ride(&mut h, nautilus);
    assert!(h.jumpable(nautilus), "a saddled nautilus may dash: its jump key does something");
    let level = mw.clone();
    let mut breaths: Vec<i32> = Vec::new();
    let mut ready = 0;
    for tick in 0..90 {
        if tick == 5 {
            h.ride_input(nautilus, [0.0, 0.0], 1.0, false, false, false, 0.0, 0.0);
        }
        // The host moves the game time on at each tick; the test world's stands still.
        let now: &'static MobWorld<'static> = Box::leak(Box::new(MobWorld { game_time: level.game_time + i64::from(tick) + 1, ..level.clone() }));
        for ev in h.step(now) {
            match ev {
                HostEvent::Misc(MiscEvent::PlayerEffect { player: 1, effect: "minecraft:breath_of_the_nautilus", ticks: 60, amplifier: 0, ambient: true }) => breaths.push(tick),
                HostEvent::Sound { name, .. } if name.starts_with("entity.nautilus.dash_ready") => ready += 1,
                _ => {}
            }
        }
    }
    // The breath is given at the boarding, then on each game time multiple of 40 (`gameTime % 40 == 0`).
    assert_eq!(breaths.first(), Some(&0), "a new rider breathes at once: {breaths:?}");
    assert!(breaths[1] <= 40, "the next one comes at the next multiple of 40: {breaths:?}");
    assert!(breaths.windows(2).skip(1).all(|p| p[1] - p[0] == 40), "then every 40 ticks: {breaths:?}");
    assert_eq!(ready, 1, "one dash, one ready sound, 40 ticks after it");
}

/// `HappyGhast.getRiddenRotation`: the ghast's pitch is half the rider's; the key held (`isJumping`) lifts it.
#[test]
fn a_piloted_ghast_tilts_by_half_the_riders_pitch_and_rises_on_the_held_jump() {
    let (mw, mut h, ghast) = with_mw("minecraft:happy_ghast");
    h.world_mut().entity_mut(ghast).insert(Harness { item: "minecraft:white_harness" });
    ride(&mut h, ghast);
    let y0 = h.world().get::<Pos>(ghast).unwrap().0.y;
    for _ in 0..20 {
        h.ride_input(ghast, [0.0, 0.0], 0.0, true, false, false, 0.0, 40.0);
        h.step(mw);
    }
    let pitch = h.world().get::<horizon_ecs_proto::goals::Head>(ghast).unwrap().pitch;
    assert!((pitch - 20.0).abs() < 1e-4, "half of 40 degrees: {pitch}");
    assert!(h.world().get::<Pos>(ghast).unwrap().0.y > y0 + 0.5, "the held jump key lifts the ghast");
}

/// A player standing above (`Above`, the still timeout) takes the steering away; the rider alone never does.
#[test]
fn a_player_above_the_ghast_takes_the_steering_away() {
    use horizon_ecs_proto::behaviours::still_under_riders::Above;
    let (_, mut h, ghast) = with_mw("minecraft:happy_ghast");
    h.world_mut().entity_mut(ghast).insert(Harness { item: "minecraft:white_harness" });
    ride(&mut h, ghast);
    assert!(h.player_steers(ghast), "the rider does not count as above");
    h.world_mut().entity_mut(ghast).insert(Above);
    assert!(!h.player_steers(ghast), "a player on top holds it still");
}
