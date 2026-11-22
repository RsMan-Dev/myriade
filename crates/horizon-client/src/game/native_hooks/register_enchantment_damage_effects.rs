use super::*;
impl Game<'_> {
    /// Register damage value effects before loading enchantment content.
    pub(in crate::game) fn native_register_enchantment_damage_effects(
        context: &mod_api::RegistrationContext,
        effects: &mut dyn EnchantmentDamageEffects,
    ) {
        Self::fire_register_enchantment_damage_effects(context, effects);
    }
}
