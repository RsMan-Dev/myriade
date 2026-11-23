//! Lot 13 of the rules of the undead and the equines (`undead.rs`, `equine.rs` of the old Mob): a drowned keeps the name, the
//! visible name, the persistence and the drop chances of its zombie; a husk keeps its silence, its baby age and its left hand
//! when it turns into a zombie. Expected values are the vanilla 26.3 ones (`ConversionType.convertCommon`, javap).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::elder_curse::Silent;
use horizon_ecs_proto::conversion::{self, ConvertKeep};
use horizon_ecs_proto::cow::{spawn_species, GrowAt, Herd, SpeciesTable};
use horizon_ecs_proto::custom_name::{CustomName, NameVisible};
use horizon_ecs_proto::despawn::Persistent;
use horizon_ecs_proto::equipment_drop::DropChances;
use horizon_ecs_proto::left_handed::LeftHanded;

fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// `ConversionType.convertCommon` (javap): a zombie that turns into a drowned keeps its custom name and its visibility, its
/// persistence and its drop chances (`copy_common` in `conversion.rs`).
#[test]
fn a_drowning_zombie_keeps_its_gear_and_name_tag() {
    if !have_data() || wrong_species(&["minecraft:zombie"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let z = spawn(&mut h, "minecraft:zombie", 0.5, 0.5);
    h.world.entity_mut(z).insert((CustomName("Bob".to_string()), NameVisible, Persistent, DropChances([2.0; 8])));
    let d = conversion::convert(&mut h.world, z, "minecraft:drowned", ConvertKeep::ALL).expect("the drowned is a species");
    assert_eq!(h.world.get::<CustomName>(d).map(|c| c.0.as_str()), Some("Bob"), "the name tag is kept");
    assert!(h.world.get::<NameVisible>(d).is_some(), "the name is shown");
    assert!(h.world.get::<Persistent>(d).is_some(), "the drowned is persistent");
    assert_eq!(h.world.get::<DropChances>(d).map(|c| c.0), Some([2.0; 8]), "the drop chances are kept");
}

/// `ConversionType.convertCommon` (javap): a husk that turns into a zombie keeps its silence, its baby age and its left hand.
#[test]
fn a_husk_turns_into_a_zombie_with_its_flags() {
    if !have_data() || wrong_species(&["minecraft:husk"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let husk = spawn(&mut h, "minecraft:husk", 0.5, 0.5);
    h.world.entity_mut(husk).insert((Silent, GrowAt(1_000_000), LeftHanded));
    let z = conversion::convert(&mut h.world, husk, "minecraft:zombie", ConvertKeep::ALL).expect("the zombie is a species");
    assert!(h.world.get::<Silent>(z).is_some(), "the zombie is silent");
    assert_eq!(h.world.get::<GrowAt>(z).map(|g| g.0), Some(1_000_000), "the zombie is still a baby");
    assert!(h.world.get::<LeftHanded>(z).is_some(), "the zombie is left-handed");
}
