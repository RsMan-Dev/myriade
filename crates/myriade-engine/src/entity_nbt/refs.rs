//! `EntityReference`s a mob's compound holds: a UUID, resolved against what is in
//! the level.
use super::Who;
use std::collections::HashMap;


#[cfg(test)]
mod tests {
    /// `LivingEntity.readAdditionalSaveData`: `lastHurtByMob` is the UUID,
    /// `lastHurtByMobTimestamp = tickCount - ticks_since_last_hurt_by_mob`, and a
    /// freshly loaded entity has `tickCount` 0, so 30 saved ticks make -30.
    #[test]
    fn a_saved_hurt_by_mob_is_read_back_as_a_reference_and_a_timestamp() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A save with no hurt-by key has no reference and a timestamp stamped at
    /// `tickCount`; a key that is not a UUID int array is no reference.
    #[test]
    fn a_save_without_a_hurt_by_mob_has_none() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}
