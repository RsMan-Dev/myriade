/// `LivingEntity.readAdditionalSaveData` applies the saved permanent attribute
/// modifiers (`AttributeInstance.apply`), then Zombie/Piglin `setBaby(IsBaby)`
/// removes a saved `minecraft:baby`. Expected values come from the vanilla
/// formula (`AttributeInstance.calculateValue`) worked by hand.
#[cfg(test)]
mod saved_attribute_modifier_tests {
    /// `AttributeInstance.calculateValue` on a saved `attack_damage` with a
    /// flat, a base and a total modifier: the loaded value is 36 and the save
    /// writes the modifier back.
    #[test]
    fn saved_modifiers_apply_by_operation_and_raise_the_loaded_health() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A saved Health above the max health is clamped to it; a saved Health
    /// Boost is not counted, since vanilla reads Health before its modifier.
    #[test]
    fn a_saved_health_above_the_max_health_is_clamped_to_it() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A modifier with the same id replaces the one held, and an unreadable
    /// modifier is skipped without dropping the others.
    #[test]
    fn a_modifier_replaces_its_id_and_an_unreadable_one_is_skipped() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// The save does not echo a `minecraft:baby` modifier the load removed,
    /// and the unread armor entry is kept as it was saved.
    #[test]
    fn the_save_does_not_echo_a_modifier_the_load_removed() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `setBaby` removes a saved `minecraft:baby` modifier for either age, a
    /// baby holds the transient one, and the permanent one is not saved again.
    #[test]
    fn a_saved_baby_modifier_is_removed_by_set_baby_for_either_age() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// `readAdditionalSaveData` stores `AbsorptionAmount` through
    /// `internalSetAbsorptionAmount`, which does not clamp (only
    /// `setAbsorptionAmount` does), and `addAdditionalSaveData` writes it back
    /// as it is: a negative amount, or one above `max_absorption` (0 without
    /// the Absorption effect), survives a load and a save.
    #[test]
    fn absorption_amount_loads_and_saves_unclamped() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}
