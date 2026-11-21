//! The species' synched slots of the second half (greffe L0, lot 06): each slot gives the entry the Mob sends for the state its
//! component holds, and nothing at the default.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::armadillo_state::{ArmadilloState, IDLE, ROLLING};
use horizon_ecs_proto::behaviours::attached_face::Attached;
use horizon_ecs_proto::behaviours::burst_shot;
use horizon_ecs_proto::behaviours::convert_timer::Converting;
use horizon_ecs_proto::behaviours::panda_state::{Gene, PandaState};
use horizon_ecs_proto::behaviours::turtle_state::Turtle;
use horizon_ecs_proto::bridge::sync::SyncValue;
use horizon_ecs_proto::bridge::sync_b;
use horizon_ecs_proto::explosion::{Charged, Fuse};
use horizon_ecs_proto::raids::Celebrating;
use horizon_protocol::entity_data::serializer;

/// The entry of `index` that the species' table gives `e` now.
fn read(kind: &str, index: u8, w: &World, e: Entity) -> Option<SyncValue> {
    let (_, slots) = sync_b::TABLE.iter().find(|(k, _)| *k == kind).expect("a table for the species");
    slots.iter().find(|s| s.index == index).and_then(|s| (s.read)(w, e))
}

fn fuse(dir: i8, ignited: bool) -> Fuse {
    Fuse { value: 0, at: 0, dir, max: 30, radius: 3.0, armed: 0, ignited }
}

#[test]
fn a_creeper_sends_its_swell_direction_always_and_its_flags_when_set() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    w.entity_mut(e).insert(fuse(-1, false));
    assert_eq!(read("minecraft:creeper", 16, &w, e), Some(SyncValue::Int(-1)), "a falling fuse sends -1");
    assert_eq!(read("minecraft:creeper", 17, &w, e), None, "not struck by lightning: powered is the default");
    assert_eq!(read("minecraft:creeper", 18, &w, e), None, "not lit: ignited is the default");
    w.entity_mut(e).insert((fuse(1, true), Charged));
    assert_eq!(read("minecraft:creeper", 16, &w, e), Some(SyncValue::Int(1)));
    assert_eq!(read("minecraft:creeper", 17, &w, e), Some(SyncValue::Bool(true)));
    assert_eq!(read("minecraft:creeper", 18, &w, e), Some(SyncValue::Bool(true)));
}

#[test]
fn a_panda_sends_its_eat_counter_flags_and_main_gene_only_when_set() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    w.entity_mut(e).insert(PandaState { flags: 0, eat: 0 });
    assert_eq!(read("minecraft:panda", 20, &w, e), None);
    assert_eq!(read("minecraft:panda", 23, &w, e), None);
    w.entity_mut(e).insert((PandaState { flags: 2, eat: 5 }, Gene::Weak));
    assert_eq!(read("minecraft:panda", 20, &w, e), Some(SyncValue::Int(5)));
    assert_eq!(read("minecraft:panda", 23, &w, e), Some(SyncValue::Byte(2)));
    assert_eq!(read("minecraft:panda", 21, &w, e), Some(SyncValue::Byte(5)), "weak is gene 5 in vanilla");
}

#[test]
fn a_shulker_sends_its_face_peek_and_colour_and_nothing_at_the_defaults() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    w.entity_mut(e).insert(Attached { face: 0, color: 16, raw: 0, eased: 0.0 });
    for index in [16, 17, 18] {
        assert_eq!(read("minecraft:shulker", index, &w, e), None, "entry {index} at its default");
    }
    w.entity_mut(e).insert(Attached { face: 3, color: 2, raw: 40, eased: 0.0 });
    assert_eq!(read("minecraft:shulker", 16, &w, e), Some(SyncValue::Id(serializer::DIRECTION, 3)));
    assert_eq!(read("minecraft:shulker", 17, &w, e), Some(SyncValue::Byte(40)));
    assert_eq!(read("minecraft:shulker", 18, &w, e), Some(SyncValue::Byte(2)));
}

#[test]
fn an_armadillo_sends_its_state_unless_idle() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    w.entity_mut(e).insert(ArmadilloState(IDLE));
    assert_eq!(read("minecraft:armadillo", 18, &w, e), None);
    w.entity_mut(e).insert(ArmadilloState(ROLLING));
    assert_eq!(read("minecraft:armadillo", 18, &w, e), Some(SyncValue::Id(serializer::ARMADILLO_STATE, 1)));
}

#[test]
fn a_zombie_in_conversion_shakes_and_a_calm_one_sends_nothing() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    assert_eq!(read("minecraft:zombie", 18, &w, e), None);
    w.entity_mut(e).insert(Converting { until: 100 });
    assert_eq!(read("minecraft:zombie", 18, &w, e), Some(SyncValue::Bool(true)));
    assert_eq!(read("minecraft:husk", 18, &w, e), Some(SyncValue::Bool(true)), "the husk shares the zombie's entry");
}

#[test]
fn a_raider_that_celebrates_sends_its_flag() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    assert_eq!(read("minecraft:pillager", 16, &w, e), None);
    w.entity_mut(e).insert(Celebrating);
    assert_eq!(read("minecraft:pillager", 16, &w, e), Some(SyncValue::Bool(true)));
    assert_eq!(read("minecraft:evoker", 16, &w, e), Some(SyncValue::Bool(true)));
}

#[test]
fn a_blaze_charged_sends_flag_1_and_a_turtle_laying_sends_its_flag() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    w.entity_mut(e).insert(burst_shot::Charged);
    assert_eq!(read("minecraft:blaze", 16, &w, e), Some(SyncValue::Byte(1)));
    let t = w.spawn_empty().id();
    w.entity_mut(t).insert(Turtle { laying: false, ..Default::default() });
    assert_eq!(read("minecraft:turtle", 19, &w, t), None);
    w.entity_mut(t).insert(Turtle { laying: true, ..Default::default() });
    assert_eq!(read("minecraft:turtle", 19, &w, t), Some(SyncValue::Bool(true)));
}
