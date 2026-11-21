//! The armor stand's state against `ArmorStand` (`hurtServer`, `hasPhysics`, the box of `BABY_DIMENSIONS`, the default pose, the equipment
//! slots): the values are the jar's, transcribed. Fire and explosion damage, gravity and the fall into an item are not checked here: no ECS
//! tick of a stand reads them yet. `HORIZON_TEST_RESOURCES` set for the equipment one.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::stand_pose::{self, Part};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::equipment;
use horizon_ecs_proto::goals::{equipped, Entered, Slot};
use horizon_ecs_proto::stand::{self, Blow, LastHit, Stand, Unsettled, DEFAULT_POSE, INVISIBLE, MARKER, NO_GRAVITY, SMALL};

/// A small stand is the baby box (0.5 by 0.9875), any other the full one (0.5 by 1.975).
#[test]
fn a_small_stand_is_the_baby_box() {
    assert_eq!(stand::stand_size(Stand { flags: SMALL }), (0.5, 0.9875));
    assert_eq!(stand::stand_size(Stand::default()), (0.5, 1.975));
}

/// A stand with a marker or without gravity never moves; an invisible one does, as the jar's `hasPhysics` says.
#[test]
fn a_marker_or_a_weightless_stand_has_no_physics() {
    assert!(!stand::has_physics(Stand { flags: MARKER }));
    assert!(!stand::has_physics(Stand { flags: NO_GRAVITY }));
    assert!(!stand::has_physics(Stand { flags: SMALL | MARKER }));
    assert!(stand::has_physics(Stand { flags: SMALL }));
    assert!(stand::has_physics(Stand { flags: INVISIBLE }));
}

/// A pose that is not the default one is kept and read back; setting the default one again leaves no pose behind.
#[test]
fn a_pose_is_set_read_and_reset_to_the_default() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    assert_eq!(stand::pose_of(&w, e), DEFAULT_POSE);
    stand_pose::set(&mut w, e, Part::Head, [15.0, 0.0, 0.0]);
    assert_eq!(stand_pose::get(&w, e, Part::Head), [15.0, 0.0, 0.0]);
    assert_eq!(stand_pose::get(&w, e, Part::LeftArm), [-10.0, 0.0, -10.0], "the other parts keep the default");
    stand_pose::set(&mut w, e, Part::Head, DEFAULT_POSE[0]);
    assert_eq!(stand::pose_of(&w, e), DEFAULT_POSE);
    assert!(w.get::<stand::Pose>(e).is_none(), "the default pose leaves no component");
}

/// A piece put in the main hand is worn there, and taking it back leaves the slot empty and returns the piece.
#[test]
fn a_piece_is_put_on_and_taken_off_a_stand() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let e = h.world.spawn_empty().id();
    assert_eq!(equipment::set_slot(&mut h.world, e, Slot::Main, 7), 0);
    assert_eq!(equipped(&h.world, e, Slot::Main), 7);
    assert_eq!(equipment::take_slot(&mut h.world, e, Slot::Main), 7);
    assert_eq!(equipped(&h.world, e, Slot::Main), 0);
}

/// The first blow wobbles the stand; a second within five ticks breaks it; a blow after that window wobbles again.
#[test]
fn two_close_blows_break_the_stand_and_a_late_one_only_wobbles() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    assert_eq!(stand::blow(&mut w, e, 10, false, false), Blow::Wobble);
    assert_eq!(w.get::<LastHit>(e).copied(), Some(LastHit(10)));
    assert_eq!(stand::blow(&mut w, e, 15, false, false), Blow::Break, "five ticks after the first is still close");
    let late = w.spawn_empty().id();
    stand::blow(&mut w, late, 10, false, false);
    assert_eq!(stand::blow(&mut w, late, 16, false, false), Blow::Wobble, "six ticks after is not close");
}

/// A creative player's blow, or one that always kills stands, breaks it at once and sets no wobble.
#[test]
fn a_creative_or_always_killing_blow_breaks_at_once() {
    let mut w = World::new();
    let a = w.spawn_empty().id();
    let b = w.spawn_empty().id();
    assert_eq!(stand::blow(&mut w, a, 3, true, false), Blow::Break);
    assert_eq!(stand::blow(&mut w, b, 3, false, true), Blow::Break);
    assert!(w.get::<LastHit>(a).is_none() && w.get::<LastHit>(b).is_none());
}

/// An invisible stand is spared by a blast (`ArmorStand.ignoreExplosion`); a visible one is not.
#[test]
fn an_invisible_stand_ignores_the_blast() {
    assert!(stand::ignores_explosion(Stand { flags: INVISIBLE }));
    assert!(!stand::ignores_explosion(Stand { flags: SMALL }));
}

/// A broken stand is gone and one armor stand item lies where it stood.
#[test]
fn a_broken_stand_drops_its_item_where_it_stood() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let e = h.world.spawn(Pos(Vec3::new(4.5, 2.0, 4.5))).id();
    let item = stand::break_drop(&mut h.world, e);
    assert!(h.world.get_entity(e).is_err(), "the stand is gone");
    let stack = &h.world.get::<horizon_ecs_proto::item::Item>(item).expect("an item").stack;
    assert_eq!(&*stack.item, "minecraft:armor_stand");
    assert_eq!(h.world.get::<Pos>(item).unwrap().0, Vec3::new(4.5, 2.0, 4.5));
}

/// The first solid cell under `(x, z)` from `from` down, as its top.
fn floor_at(h: &Herd, x: i32, z: i32, from: i32) -> i32 {
    let mw = h.world.resource::<Level>().mw.0;
    (-64..=from).rev().find(|&y| !mw.facts.is_air(mw.world.block(x, y, z))).map_or(0, |y| y + 1)
}

/// A stand made by `stand::spawn` from the armor stand species at `at`, its clock running.
fn spawn_stand(h: &mut Herd, at: Vec3, flags: u8) -> Entity {
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:armor_stand").unwrap();
    let e = stand::spawn(&mut h.world, &sp, at, 5, flags);
    if let Some(mut a) = h.world.get_mut::<ActiveUntil>(e) {
        a.0 = u64::MAX;
    }
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// The jar's fall of a living entity (`LivingEntity.travelInAir`): the move by the speed of the last tick (stopped by the floor, where the
/// vertical speed is zeroed), then the gravity 0.08 and the drag 0.98 on the speed. The height and the vertical speed after each tick.
fn fall_reference(start_y: f64, floor: f64, ticks: usize) -> Vec<(f64, f64)> {
    let (mut y, mut vy) = (start_y, 0.0_f64);
    let drag = f64::from(0.98_f32);
    (0..ticks)
        .map(|_| {
            if y + vy <= floor {
                y = floor;
                vy = 0.0;
            } else {
                y += vy;
            }
            vy = (vy - 0.08) * drag;
            (y, vy)
        })
        .collect()
}

/// A stand dropped from above falls as the jar's object tick by tick, then rests and loses its physics.
#[test]
fn a_stand_falls_and_rests_as_the_jar_does_tick_by_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let top = floor_at(&h, 0, 0, 40);
    let start = f64::from(top) + 6.0;
    let e = spawn_stand(&mut h, Vec3::new(0.5, start, 0.5), 0);
    for (t, &(y, vy)) in fall_reference(start, f64::from(top), 40).iter().enumerate() {
        h.tick();
        assert!((h.world.get::<Pos>(e).unwrap().0.y - y).abs() < 1e-9, "height after tick {}", t + 1);
        assert!((h.world.get::<Vel>(e).unwrap().0.y - vy).abs() < 1e-9, "vertical speed after tick {}", t + 1);
    }
    assert!(h.world.get::<Unsettled>(e).is_none(), "a stand at rest is settled");
}

/// A stand that lands in a fire is lit by the generic fire state (`cow::Burning`, the wheel's `Due::Burn`); its hurt is the stand's own.
#[test]
fn a_stand_that_lands_in_fire_is_lit() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let top = floor_at(&h, 0, 0, 40);
    h.set_block(0, top, 0, "minecraft:fire");
    h.tick();
    let e = spawn_stand(&mut h, Vec3::new(0.5, f64::from(top) + 3.0, 0.5), 0);
    for _ in 0..30 {
        h.tick();
    }
    assert!(h.world.get::<Burning>(e).is_some(), "a stand in the fire is set alight");
}

/// A block removed under a resting stand wakes it: it falls again to the next floor.
#[test]
fn a_block_removed_under_a_resting_stand_makes_it_fall() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let top = floor_at(&h, 0, 0, 40);
    let e = spawn_stand(&mut h, Vec3::new(0.5, f64::from(top), 0.5), 0);
    // The first tick moves by a speed of zero (no contact), the second falls the 0.0784 onto the floor: `onGround` comes from that contact.
    h.tick();
    h.tick();
    assert!(h.world.get::<Unsettled>(e).is_none(), "the stand rests on the floor");
    h.set_block(0, top - 1, 0, "minecraft:air");
    h.tick();
    assert!(h.world.get::<Unsettled>(e).is_some(), "the block under it went");
    for _ in 0..40 {
        h.tick();
    }
    assert!(h.world.get::<Pos>(e).unwrap().0.y < f64::from(top) - 0.5, "it fell below its old floor");
}

/// A stand set alight takes 4 on the burn's cadence (its first tick, then every 20), and breaks on the fifth burn at half a heart or less.
#[test]
fn a_burning_stand_takes_four_every_twenty_ticks_then_breaks() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let top = floor_at(&h, 0, 0, 40);
    let e = spawn_stand(&mut h, Vec3::new(0.5, f64::from(top), 0.5), 0);
    set_on_fire(&mut h.world, e, 100);
    for _ in 0..25 {
        h.tick();
    }
    assert_eq!(h.world.get::<Health>(e).unwrap().0, 12.0, "two burns by the 25th tick");
    for _ in 0..80 {
        h.tick();
    }
    assert!(h.world.get::<Stand>(e).is_none(), "the burn that comes at half a heart breaks it");
}

/// An invisible stand is set alight but takes no burn.
#[test]
fn an_invisible_stand_takes_no_burn() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab(16, true, false);
    let top = floor_at(&h, 0, 0, 40);
    let e = spawn_stand(&mut h, Vec3::new(0.5, f64::from(top), 0.5), INVISIBLE);
    set_on_fire(&mut h.world, e, 100);
    for _ in 0..30 {
        h.tick();
    }
    assert_eq!(h.world.get::<Health>(e).unwrap().0, 20.0);
}
