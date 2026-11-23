//! Recorded projectile weapons and the death-loot producer.
use super::*;

impl Game<'_> {
    /// Only listeners need a saved weapon copy; physical family classification never does.
    pub(in crate::game) fn set_projectile_weapon(attack: &mut Attack, weapon: Option<&ItemStack>) {
        if false
            || false
            || false
        {
            attack.projectile_weapon = weapon.cloned();
        }
    }
}
