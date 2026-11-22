//! The mounts of the bridge by `Entity` (greffe lot 70): the jump charge and its bar, the saddle, the jump's refusal, the cooldown, the
//! shears, the riders' get-off and the seat. Written, not run.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_client_lib::mobs::MobWorld;
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup};
use horizon_ecs_proto::cow::{Body, Pos};
use horizon_ecs_proto::goals::Slot;
use horizon_ecs_proto::items::item_id;
use horizon_ecs_proto::mount::{self, Mount};
use horizon_ecs_proto::mount_jump::JumpCharge;
use horizon_ecs_proto::targeting::Who;

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

/// A host world with one `kind` at the origin, and the world it reads blocks from.
fn with_mw(kind: &str) -> (&'static MobWorld<'static>, EcsWorld, Entity) {
    have_data();
    let (mw, mut h) = awake_host(&setup());
    let e = h.spawn(mw, kind, [0.5, 64.0, 0.5], None).expect("a known species");
    (mw, h, e)
}

fn with(kind: &str) -> (EcsWorld, Entity) {
    let (_, h, e) = with_mw(kind);
    (h, e)
}

/// A player of id 1 takes the first seat of `horse`.
fn ride(h: &mut EcsWorld, horse: Entity) {
    assert!(mount::ride(h.world_mut(), horse, Who::Player(1)), "the first seat is free");
}

fn saddle(h: &mut EcsWorld, e: Entity) {
    h.equip(e, Slot::Saddle, item_id("minecraft:saddle"));
}

#[test]
fn charge_jump_released_after_ten_ticks_gives_the_bar_and_ends_the_charge() {
    let (mut h, horse) = with("minecraft:horse");
    saddle(&mut h, horse);
    ride(&mut h, horse);
    for _ in 0..10 {
        assert_eq!(h.charge_jump(horse, true), 0.0, "a held jump gives nothing yet");
    }
    assert!(h.world().get::<JumpCharge>(horse).is_some_and(|c| c.ticks == 10));
    let bar = h.charge_jump(horse, false);
    assert!((bar - 1.0).abs() < 1e-6, "ten ticks of the jump bar scale to 1.0, got {bar}");
    assert!(h.world().get::<JumpCharge>(horse).is_none(), "the charge is gone");
}

#[test]
fn start_jump_refused_without_saddle_accepted_with_it() {
    let (mut h, horse) = with("minecraft:horse");
    ride(&mut h, horse);
    assert!(h.start_jump(horse, 50).is_none(), "no saddle, no jump");
    saddle(&mut h, horse);
    assert_eq!(h.start_jump(horse, 50), Some(("entity.horse.jump", 0.4)));
}

#[test]
fn a_pig_is_not_jumpable_even_saddled() {
    let (mut h, pig) = with("minecraft:pig");
    saddle(&mut h, pig);
    assert!(!h.jumpable(pig), "a pig steers with the stick, it does not jump with the bar");
}

#[test]
fn a_camel_dash_starts_a_cooldown_that_ends_after_55_ticks() {
    have_data();
    let (mw, mut h) = awake_host_on_stone(&setup());
    let camel = h.spawn(mw, "minecraft:camel", [0.5, 1.0, 0.5], None).expect("a known species");
    saddle(&mut h, camel);
    ride(&mut h, camel);
    for _ in 0..20 {
        h.step(mw);
    }
    for _ in 0..10 {
        h.charge_jump(camel, true);
    }
    let bar = h.charge_jump(camel, false);
    h.ride_input(camel, [0.0, 1.0], bar, false, false, false, 0.0, 0.0);
    h.step(mw);
    assert!(h.has_jump_cooldown(camel), "the jump dashed, so the camel waits");
    for _ in 0..55 {
        h.step(mw);
    }
    assert!(!h.has_jump_cooldown(camel), "after 55 ticks it may dash again");
}

#[test]
fn shears_take_the_saddle_off() {
    let (mut h, horse) = with("minecraft:horse");
    assert_eq!(h.shear_saddle(horse, "minecraft:shears"), None, "no saddle to shear");
    saddle(&mut h, horse);
    assert_eq!(h.shear_saddle(horse, "minecraft:shears"), Some(true));
    assert!(!h.world().get::<Mount>(horse).expect("a mount").saddled);
}

#[test]
fn refuse_rider_empties_the_seats() {
    let (mut h, horse) = with("minecraft:horse");
    ride(&mut h, horse);
    h.refuse_rider(horse);
    assert!(h.world().get::<Mount>(horse).expect("a mount").rider.is_none());
}

#[test]
fn dismounts_in_water_only_with_the_tag() {
    let (h, camel) = with("minecraft:camel");
    let (g, cow) = with("minecraft:cow");
    assert!(h.dismounts_in_water(camel, true), "the camel carries the tag");
    assert!(!h.dismounts_in_water(camel, false), "its eyes must be in the water");
    assert!(!g.dismounts_in_water(cow, true), "the cow does not carry the tag");
}

#[test]
fn rider_seat_is_the_first_seat_point() {
    let (mut h, horse) = with("minecraft:horse");
    ride(&mut h, horse);
    let at = h.world().get::<Pos>(horse).expect("a position").0;
    let height = h.world().get::<Body>(horse).expect("a body").height;
    let seat_height = h.world().get::<Mount>(horse).expect("a mount").seat_height;
    assert_eq!(h.rider_seat(horse), Some([at.x, at.y + f64::from(height * seat_height), at.z]));
}
