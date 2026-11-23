//! The mob `equipment` compound is saved as `ar/main` saved it. The `.nbt`
//! files in `tests/data/mob_equipment` were written by that codec (set
//! `HORIZON_WRITE_FIXTURES=1` and run the ignored test to regenerate them);
//! each must load and save back to the same bytes. An armor stand keeps all
//! eight slots in the same compound, body and saddle too.
use super::*;
use horizon_core::nbt::{Limits, NamedTag, parse_snbt, read_named, write_named};

const UUID: [i32; 4] = [1, 2, 3, 4];

/// A vanilla armor stand holding body and saddle beside its armor.
const STAND: (&str, &str) = (
    "armor_stand_body_saddle",
    r#"{id:"minecraft:armor_stand",Pos:[0.5d,64.0d,0.5d],ShowArms:1b,equipment:{mainhand:{id:"minecraft:iron_sword",count:1,components:{"minecraft:damage":4}},head:{id:"minecraft:iron_helmet",count:1},body:{id:"minecraft:diamond_horse_armor",count:1,components:{"minecraft:damage":6,"minecraft:custom_name":"Plate"}},saddle:{id:"minecraft:saddle",count:1}}}"#,
);

/// Loads `tag` as an armor stand and saves it back over itself.
fn stand_round_trip(tag: &Tag) -> Tag {
    let a = armor_stand_from_nbt(tag, 1).unwrap();
    armor_stand_to_nbt(&a, UUID, Some(tag))
}

fn bytes(tag: &Tag) -> Vec<u8> {
    let root = NamedTag {
        name: "".into(),
        tag: tag.clone(),
    };
    write_named(&root, Limits::default()).unwrap()
}

fn path(name: &str) -> std::path::PathBuf {
    // This module is compiled into horizon-client-lib, beside horizon-client.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../horizon-client/tests/data/mob_equipment/{name}.nbt"
    ))
}

#[test]
#[ignore = "writes the fixtures; run with HORIZON_WRITE_FIXTURES=1 on the codec to record"]
fn write_fixtures() {
    todo!("phase 2 : réécrire sur l'ECS")
}

#[test]
fn a_saved_armor_stand_keeps_its_body_and_saddle_byte_identically() {
    let file = std::fs::read(path(STAND.0)).unwrap();
    let fixture = read_named(&file, Limits::default()).unwrap().0.tag;
    let equipment = fixture.get("equipment").unwrap();
    assert!(equipment.get("body").is_some() && equipment.get("saddle").is_some());
    let stand = armor_stand_from_nbt(&fixture, 1).unwrap();
    assert!(stand.equipment[horizon_items::equipment::BODY].is_some());
    assert!(stand.equipment[horizon_items::equipment::SADDLE].is_some());
    assert_eq!(bytes(&stand_round_trip(&fixture)), file);
}

/// `ar/main` equipment: a mob's eight slots load and save back byte for byte
/// against the recorded fixtures (zombie, horse, llama, enchanted zombie).
#[test]
fn saved_mob_equipment_loads_and_saves_byte_identically() {
    todo!("phase 2 : réécrire sur l'ECS")
}
