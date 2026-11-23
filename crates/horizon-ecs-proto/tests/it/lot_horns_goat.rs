//! The goat's horns in the save (`HORIZON_SPECIES=minecraft:goat`, `HORIZON_TEST_RESOURCES` set). Written without a run:
//! the keys `HasLeftHorn` / `HasRightHorn` are written for a whole goat, and a snapped horn reads back on the ECS.
use crate::common::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::{behaviours::ram::Shed, cow::*};

fn horn(c: &Compound, key: &str) -> Option<i8> {
    match c.get(&JavaString::from(key)) {
        Some(Tag::Byte(v)) => Some(*v),
        _ => None,
    }
}

#[test]
fn a_goat_with_a_snapped_left_horn_saves_its_horn_flags() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7101);
    h.world.entity_mut(e).insert(Shed(1));
    let ecs_save = horizon_ecs_proto::nbt::save(&h.world, e).expect("a goat is saved");

    // The left horn is gone (0), the right one is there (1).
    assert_eq!(horn(&ecs_save, "HasLeftHorn"), Some(0));
    assert_eq!(horn(&ecs_save, "HasRightHorn"), Some(1));

    // The flags read back on a fresh goat put the snapped horn back in the component.
    let back = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7102);
    horizon_ecs_proto::nbt::load(&mut h.world, back, &ecs_save);
    assert_eq!(h.world.get::<Shed>(back).map(|s| s.0), Some(1));
}

#[test]
fn a_whole_goat_writes_both_horn_flags_true() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7103);
    let ecs_save = horizon_ecs_proto::nbt::save(&h.world, e).expect("a goat is saved");
    assert_eq!(horn(&ecs_save, "HasLeftHorn"), Some(1));
    assert_eq!(horn(&ecs_save, "HasRightHorn"), Some(1));
    assert!(h.world.get::<Shed>(e).is_none());
}
