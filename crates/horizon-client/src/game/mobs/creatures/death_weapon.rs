//! Recorded projectile weapons and the death-loot producer.
use super::*;

impl Game<'_> {
    /// Only listeners need a saved weapon copy; physical family classification never does.
    pub(in crate::game) fn set_projectile_weapon(attack: &mut Attack, weapon: Option<&ItemStack>) {
        if mod_api::__private::listened(mod_api::HookId::LivingIncomingDamage)
            || mod_api::__private::listened(mod_api::HookId::LivingDeath)
            || mod_api::__private::listened(mod_api::HookId::LootDrops)
        {
            attack.projectile_weapon = weapon.cloned();
        }
    }
}
