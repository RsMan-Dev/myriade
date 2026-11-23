use crate::*;
use std::sync::Arc;

/// Context of one value node in ordinary enchantment damage computation.
pub struct EnchantmentDamageContext<'a> {
    /// The actual attacker of this hit.
    pub attacker: LivingRef,
    /// The actual victim of this hit.
    pub victim: EntityRef,
    /// Weapon snapshot taken before the hit.
    pub weapon: &'a ItemStack,
    /// Enchantment level on the weapon.
    pub level: u32,
    /// This effect node's content, converted once to NBT at load, including its type and custom fields.
    pub config: &'a horizon_core::nbt::Tag,
}
