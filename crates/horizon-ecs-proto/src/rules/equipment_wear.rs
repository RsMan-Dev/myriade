//! Wear of the armour when hurt: `LivingEntity.doHurtEquipment` (`max(1, damage / 4)` per piece) and `ItemStack.hurtAndBreak`.

/// The amount a hurt of `damage` wears off each piece: none for no damage (or NaN), else `max(1, damage / 4)`.
pub fn wear_amount(damage: f32) -> i32 {
    if damage.is_nan() || damage <= 0.0 {
        0
    } else {
        (damage / 4.0).max(1.0) as i32
    }
}

/// `LivingEntity.hurtArmor` reaches the body piece only when the hit is not in `#bypasses_armor` (`armor_applies`) and the mob
/// overrides `hurtArmor` (only a horse does: a donkey's body item wears nothing).
pub fn body_wears(armor_applies: bool, overrides_hurt_armor: bool) -> bool {
    armor_applies && overrides_hurt_armor
}

/// `ItemStack.hurtAndBreak` on one piece: its damage takes `amount`; true when it reaches `max`, the piece breaks.
pub fn hurt(damage: &mut i32, amount: i32, max: i32) -> bool {
    *damage += amount;
    *damage >= max
}
