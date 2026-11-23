use horizon_core::random::RandomSource;

/// `ConversionParams` of a `SINGLE` conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params {
    /// `keepEquipment`: the worn and held items move to the new mob, with
    /// their drop chances.
    pub keep_equipment: bool,
    /// `preserveCanPickUpLoot`.
    pub preserve_can_pick_up_loot: bool,
}
impl Params {
    /// `ConversionParams.single(mob, true, true)`, what `ConversionTracker`
    /// uses (zombie to drowned, husk to zombie, skeleton to stray).
    pub const KEEP_ALL: Self = Self {
        keep_equipment: true,
        preserve_can_pick_up_loot: true,
    };
    /// `ConversionParams.single(mob, false, false)`: the old mob's gear stays
    /// behind and the new mob keeps its own `canPickUpLoot` (`MushroomCow.shear`,
    /// `Villager.thunderHit`).
    pub const NONE: Self = Self {
        keep_equipment: false,
        preserve_can_pick_up_loot: false,
    };
}

/// `ConversionParams.AfterConversion`: what the converting mob does once the
/// new one exists (`MobEvent::Convert`'s `after`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum After {
    /// No step: `Mob.convertTo` with a bare `ConversionParams.single`.
    #[default]
    None,
    /// `AbstractPiglin.finishConversion` and `Hoglin.finishConversion`: the
    /// new mob is nauseous for 10 seconds.
    Nausea,
    /// `Pig.thunderHit`: `populateDefaultEquipmentSlots` off the pig's own
    /// random, then `setPersistenceRequired`.
    ZombifiedPiglinGear,
    /// `Villager.thunderHit`: `finalizeSpawn(CONVERSION)`, then
    /// `setPersistenceRequired`.
    Witch,
    /// `Tadpole.ageUp`: `finalizeSpawn(CONVERSION)`, then
    /// `setPersistenceRequired`.
    Finalize,
}

impl After {
    /// Whether the caller already asked `LivingConversionEvent.Pre`: the
    /// lightning picks its outcome from the answer, so `convert_to` must not
    /// ask again.
    pub fn pre_checked(self) -> bool {
        matches!(self, Self::ZombifiedPiglinGear | Self::Witch)
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::{items::ItemStack, mobs::world::with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}
