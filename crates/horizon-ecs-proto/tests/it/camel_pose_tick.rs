//! `Camel.LAST_POSE_CHANGE_TICK` (index 20): the game time of the last change of pose, negative while sitting, and the saved
//! `LastPoseTick` that carries it. `Posture.since` is a game time, so a pause of the ECS clock leaves it true.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::posture::{Posture, last_pose_tick, load_pose_tick, pose_time};
use horizon_ecs_proto::bridge::sync::SyncValue;
use horizon_ecs_proto::bridge::sync_b;
use horizon_ecs_proto::cow::Clock;

fn world(base: i64) -> World {
    let mut w = World::new();
    let mut c = Clock::default();
    c.set_game_base(base);
    w.insert_resource(c);
    w
}

fn slot(w: &World, e: Entity, kind: &str) -> Option<SyncValue> {
    let (_, slots) = sync_b::TABLE.iter().find(|(k, _)| *k == kind).expect("a table for the species");
    slots.iter().find(|s| s.index == 20).and_then(|s| (s.read)(w, e))
}

#[test]
fn a_sitting_camel_sends_the_opposite_of_the_game_time_of_its_sit_down() {
    let mut w = world(1000);
    let e = w.spawn(Posture { sitting: true, since: 1030, sit_down: 40, stand_up: 52 }).id();
    assert_eq!(slot(&w, e, "minecraft:camel"), Some(SyncValue::Long(-1030)));
    assert_eq!(slot(&w, e, "minecraft:camel_husk"), Some(SyncValue::Long(-1030)));
    w.get_mut::<Posture>(e).unwrap().sitting = false;
    assert_eq!(slot(&w, e, "minecraft:camel"), Some(SyncValue::Long(1030)), "standing: positive");
}

#[test]
fn last_pose_tick_loads_back_and_the_pose_time_counts_game_ticks() {
    let mut w = world(1000);
    w.resource_mut::<Clock>().0 = 200;
    let e = w.spawn(Posture { sitting: false, since: 0, sit_down: 40, stand_up: 52 }).id();
    load_pose_tick(&mut w, e, -1150);
    assert!(w.get::<Posture>(e).unwrap().sitting);
    assert_eq!(last_pose_tick(&w, e), Some(-1150));
    assert_eq!(pose_time(&w, e), 50, "game time 1200 less 1150");
    // The clock stands still while the game time moves: the pose time follows the game time.
    w.resource_mut::<Clock>().set_game_base(1100);
    assert_eq!(pose_time(&w, e), 150);
    assert_eq!(last_pose_tick(&w, e), Some(-1150), "the date does not move");
}
