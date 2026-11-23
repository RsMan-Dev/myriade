//! `LivingEntity.updateFallFlying` for a mob: the glide's checks and its gliders' wear.
use super::*;

/// `EquipmentSlot`'s serialized name, which `equippable` names its slot by.
fn glider_slot_name(slot: horizon_core::equipment_slot::EquipmentSlot) -> &'static str {
    use horizon_core::equipment_slot::EquipmentSlot as S;
    match slot {
        S::MainHand => "mainhand",
        S::OffHand => "offhand",
        S::Feet => "feet",
        S::Legs => "legs",
        S::Chest => "chest",
        S::Head => "head",
        S::Body => "body",
        S::Saddle => "saddle",
    }
}
