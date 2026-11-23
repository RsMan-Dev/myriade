//! Règles d'usure de l'armure, reprises de `armor_hurt_tests.rs` (valeurs vanilla 26.3 inchangées) : `LivingEntity.doHurtEquipment`
//! use chaque pièce de max(1, dégâts / 4), mesurés avant la réduction par l'armure.
use horizon_ecs_proto::rules::equipment_wear::{body_wears, wear_amount};

#[test]
fn only_a_horse_wears_its_body_piece_and_a_fall_does_not() {
    // Cheval : un coup use la pièce de corps ; une chute (#bypasses_armor) ne l'use pas ; l'âne, sans surcharge, non.
    assert!(body_wears(true, true), "a horse's hit");
    assert!(!body_wears(false, true), "a fall bypasses the armour");
    assert!(!body_wears(true, false), "a donkey has no override");
}

#[test]
fn a_hurt_horse_reports_the_body_armour_wear() {
    // Un coup de 6 dégâts use la pièce d'un point ; rien sans dégâts.
    assert_eq!(wear_amount(6.0), 1);
    assert_eq!(wear_amount(0.0), 0);
}

#[test]
fn the_wear_is_the_damage_before_the_armour_reduction() {
    // 8 dégâts bruts valent 8 / 4 = 2 de durabilité, quel que soit ce que la santé encaisse.
    assert_eq!(wear_amount(8.0), 2);
}
