use crate::game::{answered_wear, durability_damage};
use mod_api::EquipmentSlot;

fn entry(slot: EquipmentSlot, new_damage: f32) -> mod_api::ArmorHurt {
    mod_api::ArmorHurt {
        slot,
        stack: mod_api::ItemStack::new(
            mod_api::__private::item_key("minecraft:diamond_helmet").unwrap(),
            1,
        )
        .unwrap(),
        damage: 2.0,
        new_damage,
    }
}

/// A mod that clears the entries, or removes one, leaves vanilla's wear (no panic).
#[test]
fn a_cleared_armor_hurt_entry_keeps_vanilla_wear() {
    mod_platform::testing::with_mods(Vec::<mod_api::ModEntry>::new().into(), || {
        mod_api::__private::resolve_items(["minecraft:diamond_helmet"]).unwrap();
        assert_eq!(answered_wear(&[], EquipmentSlot::Body, 3), 3);
        let entries = [entry(EquipmentSlot::Head, 5.0)];
        assert_eq!(answered_wear(&entries, EquipmentSlot::Chest, 2), 2);
        assert_eq!(answered_wear(&entries, EquipmentSlot::Head, 2), 5);
        assert_eq!(
            answered_wear(&[entry(EquipmentSlot::Head, -1.0)], EquipmentSlot::Head, 2),
            0
        );
    });
}

/// `(int) Math.max(1.0F, damage / 4.0F)`.
#[test]
fn durability_damage_is_vanillas() {
    assert_eq!(durability_damage(0.5) as u32, 1);
    assert_eq!(durability_damage(8.0) as u32, 2);
    assert_eq!(durability_damage(11.9) as u32, 2);
}
